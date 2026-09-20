// 零依赖的 Windows 快捷方式（.lnk）解析 + 图标提取。
//
// 纯标准库实现，不使用任何第三方 crate。行为规格见 server/shortcut.js：
//   - parse_lnk_buffer(): 解析 MS-SHLLINK 二进制（ShellLinkHeader / LinkFlags /
//     LinkTargetIDList / LinkInfo / StringData / ExtraData）。
//   - ico_from_pe(): 走 PE 资源表，取出图标档位并组装成合法 ICO 字节流。
//   - read_icon_data_url(): 返回可直接给 <img> 用的 data URL。
//
// 图标以 ICO 形式返回是刻意的：前端 canvas 负责归一化，这里不手写位图/PNG 编码。
// 找不到图标时返回空串——绝不用系统图标冒充（会让每个认不出的应用显示同一个图标）。

use std::collections::HashSet;
use std::fs;
use std::path::Path;

const LNK_CLSID: [u8; 16] = [
    0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46,
];

const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;

#[derive(Clone, Debug)]
pub struct ShortcutInfo {
    pub valid: bool,
    pub resolved_target: String,
    pub arguments: String,
    pub working_directory: String,
    pub icon_location: String,
    pub icon_index: i32,
    pub name: String,
    pub relative_path: String,
    pub is_folder: bool,
    pub run_as_admin: bool,
    pub target_exists: bool,
    pub icon_data_url: String,
    pub fallback_name: String,
}

/* ----------------------------- 字节小工具 ----------------------------- */

fn read_u8(buf: &[u8], off: usize) -> u8 {
    buf[off]
}

fn read_u16_le(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

fn read_u32_le(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

fn read_i32_le(buf: &[u8], off: usize) -> i32 {
    i32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

/* ----------------------------- 环境变量展开 ----------------------------- */

fn env_value(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

/// 返回 [(大写键, 值)]，供 %KEY% 展开使用。键名大小写不敏感匹配。
fn environment_map() -> Vec<(String, String)> {
    let windir = env_value("windir", &env_value("SystemRoot", "C:\\Windows"));
    vec![
        ("SystemRoot".into(), env_value("SystemRoot", "C:\\Windows")),
        ("windir".into(), windir.clone()),
        ("SystemDrive".into(), env_value("SystemDrive", "C:")),
        ("ProgramFiles".into(), env_value("ProgramFiles", "C:\\Program Files")),
        ("ProgramFiles(x86)".into(), env_value("ProgramFiles(x86)", "C:\\Program Files (x86)")),
        ("ProgramData".into(), env_value("ProgramData", "C:\\ProgramData")),
        ("LOCALAPPDATA".into(), env_value("LOCALAPPDATA", "")),
        ("APPDATA".into(), env_value("APPDATA", "")),
        ("USERPROFILE".into(), env_value("USERPROFILE", "")),
        ("TEMP".into(), env_value("TEMP", "")),
        ("TMP".into(), env_value("TMP", "")),
    ]
}

/// 把 %KEY% 形式的变量展开成实际路径。大小写不敏感。
fn expand_environment(value: &str) -> String {
    let map = environment_map();
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '%' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '%' {
                j += 1;
            }
            if j < chars.len() {
                let key: String = chars[i + 1..j].iter().collect();
                let lower = key.to_lowercase();
                if let Some((_, v)) = map.iter().find(|(k, _)| k.to_lowercase() == lower) {
                    out.push_str(v);
                    i = j + 1;
                    continue;
                }
            }
            out.push(chars[i]);
            i += 1;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

/// ANSI 字符串解码。无 GBK 表时退回 Latin-1（与 Node 无 ICU 时的兜底一致）。
/// 测试用的现代快捷方式均为 Unicode，不会走到这里。
fn decode_ansi(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| b as char).collect()
}

/// 完整 UTF-16LE 解码（遇到宽空字符也继续，用于 ID 列表全量扫描）。
fn decode_utf16le_full(buf: &[u8]) -> String {
    let mut s = String::new();
    let mut i = 0;
    while i + 1 < buf.len() {
        let lo = buf[i] as u16;
        let hi = buf[i + 1] as u16;
        let c = u32::from(hi) << 8 | u32::from(lo);
        s.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
        i += 2;
    }
    s
}

/// UTF-16LE 解码，遇到第一个宽空字符停止（用于正常的以空结尾字符串）。
fn read_utf16le(buf: &[u8], start: usize, end: usize) -> String {
    decode_utf16le_full(&buf[start.min(buf.len())..end.min(buf.len())])
}

fn read_null_terminated(buf: &[u8], offset: usize, wide: bool) -> String {
    if offset >= buf.len() {
        return String::new();
    }
    if wide {
        read_utf16le(buf, offset, buf.len())
    } else {
        let mut end = offset;
        while end < buf.len() && buf[end] != 0 {
            end += 1;
        }
        decode_ansi(&buf[offset..end])
    }
}

/* ----------------------------- .lnk 解析 ----------------------------- */

struct LnkParsed {
    valid: bool,
    arguments: String,
    working_directory: String,
    icon_location: String,
    icon_index: i32,
    name: String,
    relative_path: String,
    is_folder: bool,
    run_as_admin: bool,
    candidates: Vec<String>,
}

fn read_string_at(buf: &[u8], cursor: &mut usize, is_unicode: bool) -> String {
    if *cursor + 2 > buf.len() {
        return String::new();
    }
    let count = read_u16_le(buf, *cursor) as usize;
    *cursor += 2;
    let s = if is_unicode {
        let end = (*cursor + count * 2).min(buf.len());
        read_utf16le(buf, *cursor, end)
    } else {
        let end = (*cursor + count).min(buf.len());
        decode_ansi(&buf[*cursor..end])
    };
    *cursor = if is_unicode {
        (*cursor + count * 2).min(buf.len())
    } else {
        (*cursor + count).min(buf.len())
    };
    // JS 的 .replace(/\0.*$/, '') 等价于截到第一个空字符再 trim。
    let s = match s.find('\0') {
        Some(pos) => s[..pos].to_string(),
        None => s,
    };
    s.trim().to_string()
}

struct ExtraBlock {
    offset: usize,
    signature: u32,
    #[allow(dead_code)]
    size: u32,
}

fn walk_extra_data_blocks(buf: &[u8], mut at: usize) -> Vec<ExtraBlock> {
    let mut blocks = Vec::new();
    let mut guard = 0;
    while at + 4 <= buf.len() && guard < 64 {
        guard += 1;
        let size = read_u32_le(buf, at);
        if size < 4 {
            break;
        }
        if at + size as usize > buf.len() {
            break;
        }
        let signature = if at + 8 <= buf.len() {
            read_u32_le(buf, at + 4)
        } else {
            0
        };
        blocks.push(ExtraBlock {
            offset: at,
            signature,
            size,
        });
        at += size as usize;
    }
    blocks
}

/// 从目标 ID 列表（LinkTargetIDList）里扫 UTF-16 路径。
/// MSI 广播式快捷方式（LibreOffice / 格式工厂等）没有 LinkInfo，本地路径只在 ID 列表里。
fn path_from_id_list(region: &[u8]) -> String {
    if region.len() < 8 {
        return String::new();
    }
    for shift in [0usize, 1usize] {
        if shift >= region.len() {
            continue;
        }
        let text = decode_utf16le_full(&region[shift..]);
        let chars: Vec<char> = text.chars().collect();
        let n = chars.len();
        let exts = ["exe", "com", "bat", "cmd", "msc", "cpl", "pif", "scr"];
        let forbidden = |c: char| -> bool {
            let code = u32::from(c);
            code <= 0x1f || (0x202a..=0x202e).contains(&code)
        };
        let mut i = 0;
        while i + 2 < n {
            let c0 = chars[i];
            if c0.is_ascii_alphabetic() && chars[i + 1] == ':' && (chars[i + 2] == '\\' || chars[i + 2] == '/') {
                let drive_end = i + 3;
                let max_m = (n).min(drive_end + 240);
                let mut m = drive_end;
                let mut matched: Option<String> = None;
                while m < max_m {
                    if m > drive_end && m + 4 <= n && chars[m] == '.' {
                        let ext: String = chars[m + 1..m + 4].iter().collect();
                        if exts.contains(&ext.to_ascii_lowercase().as_str()) {
                            matched = Some(chars[i..m + 4].iter().collect());
                            break;
                        }
                    }
                    if m < n && forbidden(chars[m]) {
                        break;
                    }
                    m += 1;
                }
                if let Some(p) = matched {
                    let cleaned = p.replace("\\\\", "\\");
                    if cleaned.len() < 320 {
                        return cleaned;
                    }
                }
            }
            i += 1;
        }
    }
    String::new()
}

fn parse_lnk_buffer(buf: &[u8]) -> LnkParsed {
    let mut info = LnkParsed {
        valid: false,
        arguments: String::new(),
        working_directory: String::new(),
        icon_location: String::new(),
        icon_index: 0,
        name: String::new(),
        relative_path: String::new(),
        is_folder: false,
        run_as_admin: false,
        candidates: Vec::new(),
    };
    if buf.len() < 76 {
        return info;
    }
    if read_u32_le(buf, 0) != 0x4c {
        return info;
    }
    if &buf[4..20] != LNK_CLSID {
        return info;
    }
    info.valid = true;

    let flags = read_u32_le(buf, 0x14);
    let attributes = read_u32_le(buf, 0x18);
    info.icon_index = read_i32_le(buf, 0x38);
    let _show_command = read_u32_le(buf, 0x3c);
    info.is_folder = (attributes & 0x10) != 0;
    info.run_as_admin = (flags & (1 << 13)) != 0;

    let has_id_list = (flags & 1) != 0;
    let has_link_info = (flags & 2) != 0;
    let is_unicode = (flags & 128) != 0;

    let mut cursor = 76usize;
    let mut id_list_region: Option<&[u8]> = None;
    if has_id_list {
        if cursor + 2 > buf.len() {
            return info;
        }
        let id_list_size = read_u16_le(buf, cursor) as usize;
        let start = cursor + 2;
        let end = (start + id_list_size).min(buf.len());
        id_list_region = Some(&buf[start..end]);
        cursor = end;
    }

    let mut link_info_base = -1i64;
    if has_link_info && cursor + 8 <= buf.len() {
        link_info_base = cursor as i64;
        cursor += usize::try_from(read_u32_le(buf, cursor)).unwrap_or(0);
    }

    if flags & 4 != 0 {
        info.name = read_string_at(buf, &mut cursor, is_unicode);
    }
    if flags & 8 != 0 {
        info.relative_path = read_string_at(buf, &mut cursor, is_unicode);
    }
    if flags & 16 != 0 {
        info.working_directory = read_string_at(buf, &mut cursor, is_unicode);
    }
    if flags & 32 != 0 {
        info.arguments = read_string_at(buf, &mut cursor, is_unicode);
    }
    if flags & 64 != 0 {
        info.icon_location = read_string_at(buf, &mut cursor, is_unicode);
    }

    // LinkInfo 里的 LocalBasePath + CommonPathSuffix 通常是最完整的绝对路径。
    if link_info_base >= 0 {
        let lbase = link_info_base as usize;
        let flags_at = read_u32_le(buf, lbase + 8);
        if flags_at & 1 != 0 {
            let local_offset = read_u32_le(buf, lbase + 16);
            let suffix_offset = read_u32_le(buf, lbase + 24);
            if local_offset != 0 {
                let base = read_null_terminated(buf, lbase + local_offset as usize, false);
                let suffix = if suffix_offset != 0 {
                    read_null_terminated(buf, lbase + suffix_offset as usize, false)
                } else {
                    String::new()
                };
                if !base.is_empty() {
                    info.candidates.push(base + &suffix);
                } else if !suffix.is_empty() {
                    info.candidates.push(suffix);
                }
            }
        }
    }

    // 环境变量数据块（0xA0000001）里往往存着未展开的完整路径。
    for block in walk_extra_data_blocks(buf, cursor) {
        if block.signature == 0xa0000001 {
            let at = block.offset + 8;
            let wide = read_null_terminated(buf, at + 260, true);
            let ansi = read_null_terminated(buf, at, false);
            if !wide.is_empty() {
                info.candidates.push(expand_environment(&wide));
            }
            if !ansi.is_empty() {
                info.candidates.push(expand_environment(&ansi));
            }
        }
    }

    if !info.relative_path.is_empty() {
        info.candidates.push(info.relative_path.clone());
    }
    if !info.name.is_empty() {
        let cs: Vec<char> = info.name.chars().collect();
        if cs.first().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
            && cs.len() >= 3
            && cs[1] == ':'
            && (cs[2] == '\\' || cs[2] == '/')
        {
            info.candidates.push(info.name.clone());
        }
    }

    // MSI 广播式快捷方式没有 LinkInfo，只能从目标 ID 列表里捞路径，放最前。
    if let Some(region) = id_list_region {
        let from_id_list = path_from_id_list(region);
        if !from_id_list.is_empty() {
            info.candidates.insert(0, from_id_list);
        }
    }

    info.candidates = info
        .candidates
        .into_iter()
        .map(|c| c.replace('"', "").trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();

    info
}

fn resolve_relative_to_lnk(lnk_path: &Path, candidate: &str) -> String {
    if candidate.is_empty() {
        return String::new();
    }
    let expanded = expand_environment(candidate);
    let p = Path::new(&expanded);
    if p.is_absolute() {
        return expanded;
    }
    if expanded.starts_with("\\\\") {
        return expanded;
    }
    if let Some(parent) = lnk_path.parent() {
        if let Some(joined) = parent.join(&expanded).to_str() {
            return joined.to_string();
        }
    }
    expanded
}

/// 解析快捷方式并挑出最可能存在（存在则优先，否则退而求其次）的目标路径。
pub fn read_shortcut(lnk_path: &Path) -> ShortcutInfo {
    let fallback_name = lnk_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let buf = match fs::read(lnk_path) {
        Ok(b) => b,
        Err(_) => {
            return ShortcutInfo {
                valid: false,
                resolved_target: String::new(),
                arguments: String::new(),
                working_directory: String::new(),
                icon_location: String::new(),
                icon_index: 0,
                name: String::new(),
                relative_path: String::new(),
                is_folder: false,
                run_as_admin: false,
                target_exists: false,
                icon_data_url: String::new(),
                fallback_name,
            };
        }
    };

    let parsed = parse_lnk_buffer(&buf);
    let mut existing: Vec<String> = Vec::new();
    let mut guesses: Vec<String> = Vec::new();
    for cand in &parsed.candidates {
        let resolved = resolve_relative_to_lnk(lnk_path, cand);
        if resolved.is_empty() {
            continue;
        }
        if Path::new(&resolved).is_file() {
            existing.push(resolved);
        } else {
            guesses.push(resolved);
        }
    }
    let resolved_target = existing
        .first()
        .cloned()
        .or_else(|| guesses.first().cloned())
        .unwrap_or_default();
    let target_exists = !existing.is_empty();

    let icon_data_url = read_icon_data_url(&parsed.icon_location, parsed.icon_index, &resolved_target);
    let fallback_name = if !parsed.name.is_empty() {
        parsed.name.clone()
    } else {
        fallback_name
    };

    ShortcutInfo {
        valid: parsed.valid,
        resolved_target,
        arguments: parsed.arguments,
        working_directory: parsed.working_directory,
        icon_location: parsed.icon_location,
        icon_index: parsed.icon_index,
        name: parsed.name,
        relative_path: parsed.relative_path,
        is_folder: parsed.is_folder,
        run_as_admin: parsed.run_as_admin,
        target_exists,
        icon_data_url,
        fallback_name,
    }
}

/* ----------------------------- 图标提取 ----------------------------- */

fn mime_for(ext: &str) -> &'static str {
    match ext {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "image/x-icon",
    }
}

fn to_data_url(buf: &[u8], ext: &str) -> String {
    if buf.is_empty() {
        return String::new();
    }
    format!("data:{};base64,{}", mime_for(ext), base64_encode(buf))
}

fn looks_like_ico(buf: &[u8]) -> bool {
    buf.len() > 22 && read_u16_le(buf, 0) == 0 && read_u16_le(buf, 2) == 1 && read_u16_le(buf, 4) > 0
}

/// 解析 "C:\x\y.exe,3" / "%SystemRoot%\system32\shell32.dll,-16" 这类图标位置写法。
fn split_icon_location(icon_location: &str, fallback_index: i32) -> (String, i32) {
    let expanded = expand_environment(icon_location.trim().trim_matches('"'));
    let mut index = fallback_index;
    if expanded.is_empty() {
        return (String::new(), index);
    }
    // 只把末尾的 ",数字" 当作索引，避免误伤带逗号的路径。
    if let Some(comma) = expanded.rfind(',') {
        let after = &expanded[comma + 1..];
        if let Ok(parsed) = after.parse::<i32>() {
            if parsed != 0 || fallback_index == 0 {
                index = parsed;
            }
            let value = expanded[..comma].trim_matches('"').to_string();
            return (normalize_icon_path(&value), index);
        }
    }
    (normalize_icon_path(&expanded), index)
}

fn normalize_icon_path(value: &str) -> String {
    let bytes = value.as_bytes();
    let drive_unc = bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
        || value.starts_with("\\\\");
    if drive_unc {
        return value.to_string();
    }
    let system_root = env_value("SystemRoot", "C:\\Windows");
    Path::new(&system_root)
        .join(value)
        .to_string_lossy()
        .to_string()
}

fn attempt_icon(file_path: &str, index: i32, tried: &mut HashSet<String>) -> String {
    if file_path.is_empty() {
        return String::new();
    }
    let key = format!("{}|{}", file_path, index);
    if tried.contains(&key) {
        return String::new();
    }
    tried.insert(key);
    let buf = match fs::read(file_path) {
        Ok(b) => b,
        Err(_) => return String::new(),
    };
    let ext = Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if ext == "ico" {
        return if looks_like_ico(&buf) {
            to_data_url(&buf, "ico")
        } else {
            String::new()
        };
    }
    if ["png", "jpg", "jpeg", "gif", "bmp", "webp"].contains(&ext.as_str()) {
        return to_data_url(&buf, &ext);
    }
    if ["exe", "dll", "scr", "cpl", "msc", "ocx"].contains(&ext.as_str())
        || (buf.len() > 2 && read_u16_le(&buf, 0) == 0x5a4d)
    {
        if let Some(ico) = ico_from_pe(&buf, index, 128) {
            return to_data_url(&ico, "ico");
        }
        return String::new();
    }
    String::new()
}

/// 依次尝试图标位置文件、目标程序、目标同目录同名 .ico，返回 data URL。
/// 全部失败时返回空串。
pub fn read_icon_data_url(icon_location: &str, icon_index: i32, target: &str) -> String {
    let mut tried: HashSet<String> = HashSet::new();
    let (file, index) = split_icon_location(icon_location, icon_index);
    let direct = attempt_icon(&file, index, &mut tried);
    if !direct.is_empty() {
        return direct;
    }
    if !target.is_empty() {
        let from_target = attempt_icon(target, 0, &mut tried);
        if !from_target.is_empty() {
            return from_target;
        }
        if let Some(parent) = Path::new(target).parent() {
            if let Some(stem) = Path::new(target).file_stem() {
                let beside = parent.join(format!("{}.ico", stem.to_string_lossy()));
                if let Some(s) = beside.to_str() {
                    let from_beside = attempt_icon(s, 0, &mut tried);
                    if !from_beside.is_empty() {
                        return from_beside;
                    }
                }
            }
        }
    }
    String::new()
}

/* ----------------------- PE 资源表 -> 组装 ICO ----------------------- */

struct Section {
    virtual_address: u32,
    raw_pointer: u32,
    span: u32,
}

#[derive(Clone)]
struct Entry {
    id: u32,
    is_directory: bool,
    offset: u32,
}

struct Image {
    width: u8,
    height: u8,
    color_count: u8,
    planes: u16,
    bit_count: u16,
    data: Vec<u8>,
}

/// 把 PE 文件里的图标资源拼装成标准 .ico 字节流；失败返回 None。
fn ico_from_pe(buf: &[u8], wanted_index: i32, max_size: usize) -> Option<Vec<u8>> {
    if buf.len() < 0x40 {
        return None;
    }
    if read_u16_le(buf, 0) != 0x5a4d {
        return None; // "MZ"
    }
    let pe_offset = read_u32_le(buf, 0x3c) as usize;
    if pe_offset + 24 > buf.len() {
        return None;
    }
    if &buf[pe_offset..pe_offset + 4] != b"PE\0\0" {
        return None;
    }
    let coff = pe_offset + 4;
    let section_count = read_u16_le(buf, coff + 2) as usize;
    let optional_size = read_u16_le(buf, coff + 16) as usize;
    let optional = coff + 20;
    if optional + optional_size > buf.len() {
        return None;
    }
    let magic = read_u16_le(buf, optional);
    let data_directory = optional + if magic == 0x20b { 112 } else { 96 };
    if data_directory + 16 > buf.len() {
        return None;
    }
    let resource_rva = read_u32_le(buf, data_directory + 16);
    if resource_rva == 0 {
        return None;
    }

    let mut sections: Vec<Section> = Vec::new();
    let section_start = optional + optional_size;
    for idx in 0..section_count {
        let at = section_start + idx * 40;
        if at + 40 > buf.len() {
            break;
        }
        sections.push(Section {
            virtual_address: read_u32_le(buf, at + 12),
            raw_pointer: read_u32_le(buf, at + 20),
            span: read_u32_le(buf, at + 8).max(read_u32_le(buf, at + 16)),
        });
    }

    let to_offset = |rva: u32| -> i64 {
        for s in &sections {
            if rva >= s.virtual_address && rva < s.virtual_address + s.span {
                return i64::from(s.raw_pointer) + i64::from(rva - s.virtual_address);
            }
        }
        if rva as usize <= buf.len() {
            return i64::from(rva);
        }
        -1
    };

    let resource_base = to_offset(resource_rva);
    if resource_base < 0 || (resource_base as usize) + 16 > buf.len() {
        return None;
    }
    let resource_base = resource_base as usize;

    let read_directory = |relative: u32| -> Vec<Entry> {
        let at = resource_base + relative as usize;
        if at + 16 > buf.len() {
            return Vec::new();
        }
        let named = read_u16_le(buf, at + 12) as usize;
        let ids = read_u16_le(buf, at + 14) as usize;
        let mut entries = Vec::new();
        for idx in 0..named + ids {
            let entry = at + 16 + idx * 8;
            if entry + 8 > buf.len() {
                break;
            }
            let name_field = read_u32_le(buf, entry);
            let offset_field = read_u32_le(buf, entry + 4);
            entries.push(Entry {
                id: name_field & 0x7fff_ffff,
                is_directory: (offset_field & 0x8000_0000) != 0,
                offset: offset_field & 0x7fff_ffff,
            });
        }
        entries
    };

    let read_resource_data = |language_entry_offset: u32| -> Option<Vec<u8>> {
        let at = resource_base + language_entry_offset as usize;
        if at + 16 > buf.len() {
            return None;
        }
        let rva = read_u32_le(buf, at);
        let size = read_u32_le(buf, at + 4);
        let start = to_offset(rva);
        if start < 0 || (start as usize) + size as usize > buf.len() {
            return None;
        }
        Some(buf[start as usize..start as usize + size as usize].to_vec())
    };

    let icon_root = read_directory(0)
        .into_iter()
        .find(|e| e.is_directory && e.id == RT_ICON)?;
    let icon_index_entries = read_directory(icon_root.offset);
    let mut groups: Vec<Entry> = read_directory(0)
        .into_iter()
        .filter(|e| e.is_directory && e.id == RT_GROUP_ICON)
        .collect();
    if groups.is_empty() {
        return None;
    }
    groups.sort_by_key(|e| e.id);
    let mut group = groups[0].clone();
    if wanted_index < 0 {
        let want = wanted_index.unsigned_abs();
        if let Some(g) = groups.iter().find(|e| e.id == want) {
            group = g.clone();
        }
    } else if wanted_index > 0 && (wanted_index as usize) < groups.len() {
        group = groups[wanted_index as usize].clone();
    }

    let group_entry = read_directory(group.offset)
        .into_iter()
        .find(|e| e.is_directory)?;
    let language_entry = read_directory(group_entry.offset)
        .into_iter()
        .find(|e| !e.is_directory)?;
    let group_data = read_resource_data(language_entry.offset)?;
    if group_data.len() < 6 {
        return None;
    }
    let count = read_u16_le(&group_data, 4) as usize;
    if count < 1 || group_data.len() < 6 + count * 14 {
        return None;
    }

    let mut images: Vec<Image> = Vec::new();
    for idx in 0..count {
        let entry_at = 6 + idx * 14;
        let resource_id = read_u16_le(&group_data, entry_at + 12);
        let icon_directory = icon_index_entries
            .iter()
            .find(|e| e.id == u32::from(resource_id))?;
        let icon_language = read_directory(icon_directory.offset)
            .into_iter()
            .find(|e| !e.is_directory)?;
        let blob = read_resource_data(icon_language.offset)?;
        if blob.is_empty() {
            continue;
        }
        images.push(Image {
            width: read_u8(&group_data, entry_at),
            height: read_u8(&group_data, entry_at + 1),
            color_count: read_u8(&group_data, entry_at + 2),
            planes: read_u16_le(&group_data, entry_at + 4),
            bit_count: read_u16_le(&group_data, entry_at + 6),
            data: blob,
        });
    }
    if images.is_empty() {
        return None;
    }

    // 图标只在 42px 的格子里显示，没必要塞 256px 及以上的大图。
    // 首选不超过 max_size 的最大一档；都没有就退回最小的一档。
    let effective = |w: u8| -> usize {
        if w == 0 {
            256
        } else {
            usize::from(w)
        }
    };
    let sized: Vec<&Image> = images
        .iter()
        .filter(|im| effective(im.width) <= max_size)
        .collect();
    let chosen: Vec<&Image> = if !sized.is_empty() {
        let mut s = sized;
        s.sort_by(|a, b| effective(b.width).cmp(&effective(a.width)));
        s
    } else {
        let mut all: Vec<&Image> = images.iter().collect();
        all.sort_by(|a, b| effective(a.width).cmp(&effective(b.width)));
        vec![all[0]]
    };

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&[0, 0]); // reserved
    out.extend_from_slice(&[1, 0]); // type = icon
    out.extend_from_slice(&(chosen.len() as u16).to_le_bytes()); // count
    let mut offset = 6 + chosen.len() * 16;
    for im in &chosen {
        let mut dir = [0u8; 16];
        dir[0] = im.width;
        dir[1] = im.height;
        dir[2] = im.color_count;
        dir[3] = 0;
        dir[4..6].copy_from_slice(&im.planes.to_le_bytes());
        dir[6..8].copy_from_slice(&im.bit_count.to_le_bytes());
        dir[8..12].copy_from_slice(&(im.data.len() as u32).to_le_bytes());
        dir[12..16].copy_from_slice(&(offset as u32).to_le_bytes());
        out.extend_from_slice(&dir);
        offset += im.data.len();
    }
    for im in &chosen {
        out.extend_from_slice(&im.data);
    }
    Some(out)
}

/* ----------------------------- Base64 ----------------------------- */

fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let len = input.len();
    let mut i = 0;
    while i + 3 <= len {
        let n = (u32::from(input[i]) << 16)
            | (u32::from(input[i + 1]) << 8)
            | u32::from(input[i + 2]);
        out.push(char::from(CHARS[((n >> 18) & 63) as usize]));
        out.push(char::from(CHARS[((n >> 12) & 63) as usize]));
        out.push(char::from(CHARS[((n >> 6) & 63) as usize]));
        out.push(char::from(CHARS[(n & 63) as usize]));
        i += 3;
    }
    let rem = len - i;
    if rem == 1 {
        let n = u32::from(input[i]) << 16;
        out.push(char::from(CHARS[((n >> 18) & 63) as usize]));
        out.push(char::from(CHARS[((n >> 12) & 63) as usize]));
        out.push('=');
        out.push('=');
    } else if rem == 2 {
        let n = (u32::from(input[i]) << 16) | (u32::from(input[i + 1]) << 8);
        out.push(char::from(CHARS[((n >> 18) & 63) as usize]));
        out.push(char::from(CHARS[((n >> 12) & 63) as usize]));
        out.push(char::from(CHARS[((n >> 6) & 63) as usize]));
        out.push('=');
    }
    out
}

#[allow(dead_code)]
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    let mut table = [-1i16; 256];
    let chars = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    for (i, &c) in chars.iter().enumerate() {
        table[c as usize] = i as i16;
    }
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 4 <= bytes.len() {
        let c0 = table[bytes[i] as usize];
        let c1 = table[bytes[i + 1] as usize];
        let c2 = if bytes[i + 2] == b'=' {
            0
        } else {
            table[bytes[i + 2] as usize]
        };
        let c3 = if bytes[i + 3] == b'=' {
            0
        } else {
            table[bytes[i + 3] as usize]
        };
        if c0 < 0 || c1 < 0 || c2 < 0 || c3 < 0 {
            return None;
        }
        let n = (u32::from(c0 as u8) << 18)
            | (u32::from(c1 as u8) << 12)
            | (u32::from(c2 as u8) << 6)
            | u32::from(c3 as u8);
        out.push((n >> 16) as u8);
        out.push((n >> 8) as u8);
        out.push(n as u8);
        i += 4;
    }
    Some(out)
}

/* ----------------------------- 单测 ----------------------------- */

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机真实快捷方式样本。用户目录部分用 %APPDATA% 拼接，
    /// 避免把开发机的个人路径写进公开仓库。
    fn test_lnks() -> Vec<String> {
        let mut paths = vec![
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\微信\微信.lnk".to_string(),
            r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Steam\Steam.lnk".to_string(),
        ];
        if let Ok(appdata) = std::env::var("APPDATA") {
            paths.push(format!(
                r"{}\Microsoft\Windows\Start Menu\Programs\Visual Studio Code.lnk",
                appdata
            ));
        }
        paths
    }

    #[test]
    fn parse_real_shortcuts() {
        for lnk in test_lnks() {
            let path = Path::new(&lnk);
            assert!(path.exists(), "测试快捷方式不存在: {}", lnk);
            let info = read_shortcut(path);
            assert!(info.valid, "LNK 解析无效: {}", lnk);
            assert!(
                !info.resolved_target.is_empty(),
                "解析出的 target 为空: {}",
                lnk
            );
            assert!(
                info.resolved_target.to_lowercase().ends_with(".exe"),
                "target 不是 .exe: {} -> {}",
                lnk,
                info.resolved_target
            );
            assert!(
                Path::new(&info.resolved_target).exists(),
                "target 文件不存在: {} -> {}",
                lnk,
                info.resolved_target
            );
            assert!(
                info.icon_data_url.starts_with("data:image/x-icon;base64,"),
                "icon 前缀错误: {}",
                lnk
            );
            assert!(
                info.icon_data_url.len() > 1000,
                "icon 太短（可能没提取到）: {} (len={})",
                lnk,
                info.icon_data_url.len()
            );

            // ICO 结构校验
            let b64 = &info.icon_data_url["data:image/x-icon;base64,".len()..];
            let bytes = base64_decode(b64).expect("base64 解码失败");
            assert!(bytes.len() >= 6, "ICO 太短: {}", lnk);
            assert_eq!(bytes[0], 0, "ICO 头部[0] 应为 0: {}", lnk);
            assert_eq!(bytes[1], 0, "ICO 头部[1] 应为 0: {}", lnk);
            assert_eq!(bytes[2], 1, "ICO 头部[2] 应为 1（icon 类型）: {}", lnk);
            assert_eq!(bytes[3], 0, "ICO 头部[3] 应为 0: {}", lnk);
            let count = u16::from_le_bytes([bytes[4], bytes[5]]) as usize;
            for idx in 0..count {
                let at = 6 + idx * 16;
                assert!(at + 16 <= bytes.len(), "ICO 目录项越界: {} idx {}", lnk, idx);
                let size = u32::from_le_bytes([
                    bytes[at + 8],
                    bytes[at + 9],
                    bytes[at + 10],
                    bytes[at + 11],
                ]);
                let off = u32::from_le_bytes([
                    bytes[at + 12],
                    bytes[at + 13],
                    bytes[at + 14],
                    bytes[at + 15],
                ]);
                assert!(
                    off + size <= bytes.len() as u32,
                    "ICO 条目 (offset+size) 越界: {} idx {} ({} + {} > {})",
                    lnk,
                    idx,
                    off,
                    size,
                    bytes.len()
                );
            }
        }
    }

    #[test]
    fn invalid_shortcut_is_safe() {
        let info = read_shortcut(Path::new("C:\\不存在的路径\\nope.lnk"));
        assert!(!info.valid);
        assert!(info.icon_data_url.is_empty());
    }
}
