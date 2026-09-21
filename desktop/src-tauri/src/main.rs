// ============================================================================
// 模块 M3：桌面客户端（Tauri 外壳 + IPC 命令）
// 接口文档：docs/interfaces/tauri-ipc.md      总纲：docs/INTERFACES.md
//
// 职责：便携式 Windows 客户端，直接读写 data/，不需要 Node 服务；
//       界面仍是 app/index.html（编译时内嵌，改前端后必须重新编译才生效）。
//
// 边界：
//   - 只改本目录。数据结构以 docs/interfaces/data-catalog.md 为准。
//   - 不要使用固定盘符/用户名/安装目录（必须保持便携）。
//   - 新增或改名 IPC 命令时，必须同步 docs/interfaces/tauri-ipc.md，
//     并提醒前端更新 app/index.html 里 request() 的路径→命令映射表。
//
// 本次改造（对齐 server/server.js / server/shortcut.js）：
//   - Catalog 增加 categories 字段，读时补齐默认分类、写时原样回写（修复分类注册表被丢的 Bug）。
//   - Item 增加 target/arguments/working_directory/icon_location，并迁移旧 url 字段到 target。
//   - 补齐分类管理（create/update/delete_category）、开始菜单快捷方式枚举（list_shortcuts）、
//     快捷方式解析（shortcut_detail）、4 种 kind 的 create_item / complete_metadata / open_item。
// ============================================================================

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod shortcut;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use shortcut::{read_icon_data_url, read_shortcut};

static ITEM_COUNTER: AtomicU64 = AtomicU64::new(0);

const KINDS: &[&str] = &["script", "link", "app", "file"];

/* ----------------------------- 数据模型 ----------------------------- */

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Item {
    id: String,
    category: String,
    title: String,
    description: String,
    tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<String>,
    /// 仅「网址」类型使用：缺省 = 跟随系统默认浏览器；否则是浏览器 key 或 exe 绝对路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    browser: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    working_directory: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_location: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
    status: String,
    created_at: String,
    updated_at: String,
    #[serde(default)]
    open_count: u64,
    #[serde(default)]
    last_opened_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    order: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Category {
    key: String,
    label: String,
    folder: String,
    kind: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    order: i64,
    #[serde(default)]
    builtin: bool,
    #[serde(default)]
    inbox: bool,
}

#[derive(Serialize, Deserialize)]
struct Catalog {
    #[serde(default = "default_version")]
    version: u8,
    #[serde(default)]
    categories: Vec<Category>,
    items: Vec<Item>,
}

fn default_version() -> u8 {
    2
}

/// 读目录用的宽松结构（字段均与 JSON 单字 key 同名，无需额外 rename）。
#[derive(Deserialize)]
struct RawCategory {
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    folder: Option<String>,
    #[serde(default)]
    kind: Option<String>,
    #[serde(default)]
    symbol: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    order: Option<i64>,
    #[serde(default)]
    builtin: Option<bool>,
    #[serde(default)]
    inbox: Option<bool>,
}

/* ----------------------------- 路径与工具 ----------------------------- */

fn workspace_root() -> PathBuf {
    if let Ok(exe) = env::current_exe() {
        if let Some(parent) = exe.parent() {
            if parent.join("data").is_dir() {
                return parent.to_path_buf();
            }
        }
    }
    if let Ok(cwd) = env::current_dir() {
        for candidate in cwd.ancestors() {
            if candidate.join("data").is_dir() {
                return candidate.to_path_buf();
            }
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn data_root() -> PathBuf {
    workspace_root().join("data")
}
fn catalog_path() -> PathBuf {
    data_root().join("catalog.json")
}
fn now() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .to_string()
}
fn new_id() -> String {
    format!(
        "{}-{}-{}",
        now(),
        std::process::id(),
        ITEM_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn default_categories() -> Vec<Category> {
    vec![
        cat("script", "脚本", "脚本", "script", "▣", "点击运行 · 可拖动排序", 0, true, false),
        cat("link", "网址", "网址", "link", "↗", "点击打开 · 可拖动排序", 1, true, false),
        cat("app", "应用", "应用", "app", "◈", "点击启动 · 保留原快捷方式", 2, true, false),
        cat("file", "文档", "文档", "file", "▤", "点击用默认程序打开", 3, true, false),
        // 文件夹：kind 复用 file，条目只存绝对路径，点一下由 start_detached() 拉起资源管理器。
        cat("folder", "文件夹", "文件夹", "file", "▦", "点击跳转到该文件夹", 4, true, false),
        cat("inbox", "待整理", "待整理", "inbox", "!", "补充分类后变成快捷图标", 90, true, true),
    ]
}

#[allow(clippy::too_many_arguments)]
fn cat(
    key: &str,
    label: &str,
    folder: &str,
    kind: &str,
    symbol: &str,
    note: &str,
    order: i64,
    builtin: bool,
    inbox: bool,
) -> Category {
    Category {
        key: key.to_string(),
        label: label.to_string(),
        folder: folder.to_string(),
        kind: kind.to_string(),
        symbol: symbol.to_string(),
        note: note.to_string(),
        order,
        builtin,
        inbox,
    }
}

/* ----------------------------- SHA-1（零依赖） ----------------------------- */

fn sha1(data: &[u8]) -> [u8; 20] {
    fn rol(v: u32, n: u32) -> u32 {
        (v << n) | (v >> (32 - n))
    }
    let mut h: [u32; 5] = [0x6745_2301, 0xEFCD_AB89, 0x98BA_DCFE, 0x1032_5476, 0xC3D2_E1F0];
    let mut msg = data.to_vec();
    let orig_len = (data.len() as u64) * 8;
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&orig_len.to_be_bytes());
    for chunk in msg.chunks(64) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([chunk[i * 4], chunk[i * 4 + 1], chunk[i * 4 + 2], chunk[i * 4 + 3]]);
        }
        for i in 16..80 {
            w[i] = rol(w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16], 1);
        }
        let (mut a, mut b, mut c, mut d, mut e) = (h[0], h[1], h[2], h[3], h[4]);
        for i in 0..80 {
            let (f, k) = if i < 20 {
                ((b & c) | ((!b) & d), 0x5A82_7999u32)
            } else if i < 40 {
                (b ^ c ^ d, 0x6ED9_EBA1)
            } else if i < 60 {
                ((b & c) | (b & d) | (c & d), 0x8F1B_BCDC)
            } else {
                (b ^ c ^ d, 0xCA62_C1D6)
            };
            let temp = rol(a, 5)
                .wrapping_add(f)
                .wrapping_add(e)
                .wrapping_add(k)
                .wrapping_add(w[i]);
            e = d;
            d = c;
            c = rol(b, 30);
            b = a;
            a = temp;
        }
        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
    }
    let mut out = [0u8; 20];
    for i in 0..5 {
        out[i * 4..i * 4 + 4].copy_from_slice(&h[i].to_be_bytes());
    }
    out
}

fn sha1_hex(s: &str) -> String {
    sha1(s.as_bytes())
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

fn slug_key(label: &str) -> String {
    format!("cat-{}", &sha1_hex(label)[..8])
}

/* ----------------------------- 文件名清洗 ----------------------------- */

/// 与 server.js 的 safeFolderName 对齐：移除非法字符、折叠空白、去掉末尾点、限长 24。
fn safe_folder_name(value: &str) -> String {
    let kept: String = value
        .chars()
        .filter(|&c| !"< >:\"/\\|?*".contains(c) && (c as u32) > 0x1f)
        .collect();
    let collapsed = kept.split_whitespace().collect::<Vec<&str>>().join(" ");
    let trimmed = collapsed.trim().trim_end_matches('.').to_string();
    trimmed.chars().take(24).collect()
}

/// 与 server.js 的 safeFilename 对齐：非法字符替换为 '-'、去掉末尾点、限长 80、空则 untitled。
fn safe_filename(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || (c as u32) <= 0x1f {
                '-'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim_end_matches('.').to_string();
    if trimmed.is_empty() {
        "untitled".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

fn source_join(source: &str) -> Option<PathBuf> {
    let candidate = Path::new(source);
    if candidate.components().count() != 2 {
        return None;
    }
    let full = data_root().join(candidate);
    if full.starts_with(data_root()) {
        Some(full)
    } else {
        None
    }
}

/* ----------------------------- 目录读写 ----------------------------- */

fn list_data_folders() -> Vec<String> {
    match fs::read_dir(data_root()) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn ensure_directories(categories: &[Category]) -> Result<(), String> {
    for c in categories {
        if !c.folder.is_empty() {
            fs::create_dir_all(data_root().join(&c.folder)).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

fn normalize_category(raw: &RawCategory, index: usize) -> Option<Category> {
    let label = raw.label.clone().unwrap_or_default().trim().to_string();
    if label.is_empty() {
        return None;
    }
    let key = if let Some(k) = &raw.key {
        if !k.is_empty() {
            k.clone()
        } else {
            slug_key(&label)
        }
    } else {
        slug_key(&label)
    };
    let folder_input = raw
        .folder
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| label.clone());
    let folder = safe_folder_name(&folder_input);
    let folder = if folder.is_empty() {
        label.clone()
    } else {
        folder
    };
    let kind = if raw.inbox == Some(true) {
        "inbox".to_string()
    } else if let Some(k) = &raw.kind {
        if KINDS.contains(&k.as_str()) {
            k.clone()
        } else {
            "file".to_string()
        }
    } else {
        "file".to_string()
    };
    let symbol = raw
        .symbol
        .clone()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "▤".to_string());
    let symbol = symbol.chars().take(2).collect();
    let note = raw.note.clone().unwrap_or_default().chars().take(40).collect();
    let order = raw.order.unwrap_or(index as i64);
    let builtin = raw.builtin.unwrap_or(false);
    let inbox = raw.inbox.unwrap_or(false);
    Some(Category {
        key,
        label,
        folder,
        kind,
        symbol,
        note,
        order,
        builtin,
        inbox,
    })
}

fn default_catalog() -> Catalog {
    Catalog {
        version: 2,
        categories: default_categories(),
        items: Vec::new(),
    }
}

fn read_catalog() -> Catalog {
    let text = match fs::read_to_string(catalog_path()) {
        Ok(t) => t,
        Err(_) => return default_catalog(),
    };
    let parsed: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return default_catalog(),
    };

    let mut items: Vec<Item> = parsed
        .get("items")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // 老数据把网址存在 url 字段，统一迁移到 target。
    for item in &mut items {
        if item.target.as_deref().map_or(true, |t| t.is_empty()) {
            if let Some(u) = &item.url {
                if !u.is_empty() {
                    item.target = Some(u.clone());
                }
            }
        }
    }

    let mut catalog = Catalog {
        version: 2,
        categories: Vec::new(),
        items,
    };

    let raw_categories: Vec<RawCategory> = parsed
        .get("categories")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let source: Vec<RawCategory> = if !raw_categories.is_empty() {
        raw_categories
    } else {
        default_categories()
            .into_iter()
            .map(|c| RawCategory {
                key: Some(c.key),
                label: Some(c.label),
                folder: Some(c.folder),
                kind: Some(c.kind),
                symbol: Some(c.symbol),
                note: Some(c.note),
                order: Some(c.order),
                builtin: Some(c.builtin),
                inbox: Some(c.inbox),
            })
            .collect()
    };

    let mut index = 0;
    for rc in &source {
        if let Some(c) = normalize_category(rc, index) {
            catalog.categories.push(c);
        }
        index += 1;
    }

    // 兜底：data/ 下已存在但没注册的文件夹收编成 kind:file 的自定义分类。
    for name in list_data_folders() {
        if catalog.categories.iter().any(|c| c.folder == name) {
            continue;
        }
        if let Some(fb) = default_categories().iter().find(|c| c.folder == name) {
            let raw = RawCategory {
                key: Some(fb.key.clone()),
                label: Some(fb.label.clone()),
                folder: Some(fb.folder.clone()),
                kind: Some(fb.kind.clone()),
                symbol: Some(fb.symbol.clone()),
                note: Some(fb.note.clone()),
                order: Some(fb.order),
                builtin: Some(fb.builtin),
                inbox: Some(fb.inbox),
            };
            if let Some(c) = normalize_category(&raw, catalog.categories.len()) {
                catalog.categories.push(c);
            }
        } else if !catalog.categories.iter().any(|c| c.key == slug_key(&name)) {
            catalog.categories.push(Category {
                key: slug_key(&name),
                label: name.clone(),
                folder: name.clone(),
                kind: "file".to_string(),
                symbol: "▤".to_string(),
                note: String::new(),
                order: catalog.categories.len() as i64,
                builtin: false,
                inbox: false,
            });
        }
    }

    // 内置分类补齐：上面「有 categories 就用它」会让后来新增的内置分类对已有数据不可见，
    // 这里按 key（或 folder）把缺失的内置分类补进去，保证与 M1 看到同一套分类。
    for preset in default_categories() {
        if preset.inbox {
            continue; // 收件箱由下面那一段专门补齐
        }
        let exists = catalog
            .categories
            .iter()
            .any(|c| c.key == preset.key || c.folder == preset.folder);
        if !exists {
            let raw = RawCategory {
                key: Some(preset.key.clone()),
                label: Some(preset.label.clone()),
                folder: Some(preset.folder.clone()),
                kind: Some(preset.kind.clone()),
                symbol: Some(preset.symbol.clone()),
                note: Some(preset.note.clone()),
                order: Some(preset.order),
                builtin: Some(preset.builtin),
                inbox: Some(preset.inbox),
            };
            if let Some(c) = normalize_category(&raw, catalog.categories.len()) {
                catalog.categories.push(c);
            }
        }
    }

    if !catalog.categories.iter().any(|c| c.inbox) {
        let inbox = default_categories().into_iter().find(|c| c.inbox).unwrap();
        catalog.categories.push(inbox);
    }

    catalog
}

fn write_catalog(catalog: &Catalog) -> Result<(), String> {
    let path = catalog_path();
    let temp = path.with_extension("json.tmp");
    fs::write(
        &temp,
        serde_json::to_string_pretty(catalog).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    fs::rename(temp, path).map_err(|e| e.to_string())
}

/* ----------------------------- 快捷方式辅助 ----------------------------- */

fn read_url_shortcut(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().and_then(|s| {
        s.lines().find_map(|line| {
            let l = line.trim_start();
            if l.len() >= 4 && l[..4].eq_ignore_ascii_case("URL=") {
                Some(l[4..].trim().to_string())
            } else {
                None
            }
        })
    })
}

fn shortcut_title(file_path: &Path) -> String {
    let stem = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    if !file_path.to_string_lossy().to_lowercase().ends_with(".lnk") {
        return stem;
    }
    let info = read_shortcut(file_path);
    let lower = info.name.to_lowercase();
    if !info.name.is_empty()
        && info.name.len() <= 40
        && !lower.contains("http")
        && !lower.contains("https")
        && !lower.contains(".com")
        && !info.name.contains("参见")
    {
        info.name
    } else {
        stem
    }
}

/// 把快捷方式属性写进条目；copy=true 时把 .lnk 复制进 folder 以保留启动参数/工作目录/管理员标记。
fn apply_shortcut_metadata(item: &mut Item, source_file_path: &Path, copy: bool, folder: &str) {
    let ext = source_file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if ext == "lnk" {
        let info = read_shortcut(source_file_path);
        item.target = Some(info.resolved_target.clone());
        if !info.arguments.is_empty() {
            item.arguments = Some(info.arguments.clone());
        }
        if !info.working_directory.is_empty() {
            item.working_directory = Some(info.working_directory.clone());
        }
        if !info.icon_location.is_empty() {
            item.icon_location = Some(info.icon_location.clone());
        }
        if copy {
            let folder = if folder.is_empty() { "应用" } else { folder };
            let base_dir = data_root().join(folder);
            let _ = fs::create_dir_all(&base_dir);
            let stem = source_file_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("app")
                .to_string();
            let mut file_name = format!("{}.lnk", safe_filename(&stem));
            let mut target = base_dir.join(&file_name);
            let mut suffix = 1;
            while target.is_file() && target != source_file_path {
                file_name = format!("{} ({}).lnk", safe_filename(&stem), suffix);
                target = base_dir.join(&file_name);
                suffix += 1;
            }
            if target != source_file_path {
                let _ = fs::copy(source_file_path, &target);
            }
            item.source_path = Some(format!("{}/{}", folder, file_name));
        }
        if item.icon.as_deref().map_or(true, |s| s.is_empty()) {
            item.icon = if info.icon_data_url.is_empty() {
                None
            } else {
                Some(info.icon_data_url.clone())
            };
        }
        if item.title.is_empty() {
            item.title = if !info.name.is_empty() {
                info.name.clone()
            } else {
                source_file_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("")
                    .to_string()
            };
        }
        if item.target.as_deref().map_or(true, |s| s.is_empty()) {
            item.target = Some(info.resolved_target.clone());
        }
    } else {
        if item.target.is_none() {
            item.target = Some(source_file_path.to_string_lossy().to_string());
        }
        if item.icon.as_deref().map_or(true, |s| s.is_empty()) {
            item.icon = Some(read_icon_data_url(
                "",
                0,
                &source_file_path.to_string_lossy(),
            ));
        }
    }
}

fn is_absolute_drive(p: &str) -> bool {
    let b = p.as_bytes();
    b.len() >= 3 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/')
}

/* ------------- 网址：本机浏览器探测（表与 M1 的 BROWSERS 保持一致） ------------- */

/// key / 显示名 / [环境变量, exe 相对路径] 候选。系统默认浏览器不在表内（空值即默认）。
fn browser_specs() -> Vec<(&'static str, &'static str, Vec<(&'static str, &'static str)>)> {
    vec![
        (
            "chrome",
            "Google Chrome",
            vec![
                ("ProgramFiles", r"Google\Chrome\Application\chrome.exe"),
                ("ProgramFiles(x86)", r"Google\Chrome\Application\chrome.exe"),
                ("LOCALAPPDATA", r"Google\Chrome\Application\chrome.exe"),
            ],
        ),
        (
            "edge",
            "Microsoft Edge",
            vec![
                ("ProgramFiles", r"Microsoft\Edge\Application\msedge.exe"),
                ("ProgramFiles(x86)", r"Microsoft\Edge\Application\msedge.exe"),
            ],
        ),
        (
            "firefox",
            "Mozilla Firefox",
            vec![
                ("ProgramFiles", r"Mozilla Firefox\firefox.exe"),
                ("ProgramFiles(x86)", r"Mozilla Firefox\firefox.exe"),
            ],
        ),
        (
            "brave",
            "Brave",
            vec![
                ("ProgramFiles", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
                ("ProgramFiles(x86)", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
                ("LOCALAPPDATA", r"BraveSoftware\Brave-Browser\Application\brave.exe"),
            ],
        ),
        (
            "vivaldi",
            "Vivaldi",
            vec![
                ("LOCALAPPDATA", r"Vivaldi\Application\vivaldi.exe"),
                ("ProgramFiles", r"Vivaldi\Application\vivaldi.exe"),
            ],
        ),
        (
            "opera",
            "Opera",
            vec![
                ("LOCALAPPDATA", r"Programs\Opera\opera.exe"),
                ("ProgramFiles", r"Opera\opera.exe"),
            ],
        ),
        (
            "chromium",
            "Chromium",
            vec![
                ("LOCALAPPDATA", r"Chromium\Application\chrome.exe"),
                ("ProgramFiles", r"Chromium\Application\chrome.exe"),
            ],
        ),
    ]
}

/// 探测本机已安装的浏览器：(key, 显示名, exe 绝对路径)。非 Windows 返回空表。
fn detect_browsers() -> Vec<(String, String, String)> {
    let mut found = Vec::new();
    if !cfg!(windows) {
        return found;
    }
    for (key, label, paths) in browser_specs() {
        for (env_key, relative) in paths {
            let base = match std::env::var(env_key) {
                Ok(value) => value,
                Err(_) => continue,
            };
            if base.is_empty() {
                continue;
            }
            let full = Path::new(&base).join(relative);
            if full.is_file() {
                found.push((
                    key.to_string(),
                    label.to_string(),
                    full.to_string_lossy().to_string(),
                ));
                break;
            }
        }
    }
    found
}

/// 条目的 browser 值 → 浏览器 exe 绝对路径。空值、未知 key、路径不存在都返回 None。
fn resolve_browser_path(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    if is_absolute_drive(text) || text.starts_with("\\\\") {
        return if Path::new(text).is_file() {
            Some(text.to_string())
        } else {
            None
        };
    }
    detect_browsers()
        .into_iter()
        .find(|(key, _, _)| key.eq_ignore_ascii_case(text))
        .map(|(_, _, path)| path)
}

/// 规范化 browser 字段：空 → None（跟随系统默认）；已知 key 或绝对路径保留；其它丢弃。
fn normalize_browser(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    if is_absolute_drive(text) || text.starts_with("\\\\") {
        return Some(text.chars().take(260).collect());
    }
    browser_specs()
        .iter()
        .map(|(key, _, _)| *key)
        .find(|key| key.eq_ignore_ascii_case(text))
        .map(|key| key.to_string())
}

/// 用指定浏览器打开网址（DETACHED，不阻塞界面）。
fn start_with_browser(exe: &str, url: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        Command::new(exe)
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0000_0008) // DETACHED_PROCESS
            .spawn()
            .map_err(|e| format!("无法启动浏览器：{}", e))?;
    }
    #[cfg(not(windows))]
    {
        Command::new(exe)
            .arg(url)
            .spawn()
            .map_err(|e| format!("无法启动浏览器：{}", e))?;
    }
    Ok(())
}

fn normalize_url(value: &str) -> Option<String> {
    let text = value.trim();
    if text.is_empty() {
        return None;
    }
    let candidate = if has_scheme(text) {
        text.to_string()
    } else {
        format!("https://{}", text)
    };
    let lower = candidate.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        if let Some(rest) = candidate.splitn(2, "://").nth(1) {
            if !rest.is_empty() {
                return Some(candidate);
            }
        }
        None
    } else {
        None
    }
}

fn has_scheme(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i >= b.len() || !b[i].is_ascii_alphabetic() {
        return false;
    }
    i += 1;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'+' || b[i] == b'-' || b[i] == b'.') {
        i += 1;
    }
    i < b.len() && b[i] == b':' && i + 2 < b.len() && b[i + 1] == b'/' && b[i + 2] == b'/'
}

/* ----------------------------- 扫描 ----------------------------- */

fn scan(catalog: &mut Catalog) -> Result<(Vec<Item>, Vec<Item>), String> {
    ensure_directories(&catalog.categories)?;
    let mut discovered = Vec::new();
    let mut removed = Vec::new();

    catalog.items.retain(|item| {
        let Some(source) = item.source_path.as_deref() else {
            return true;
        };
        let Some(path) = source_join(source) else {
            return true;
        };
        if path.is_file() {
            true
        } else {
            removed.push(item.clone());
            false
        }
    });

    let mut known: std::collections::HashSet<String> = catalog
        .items
        .iter()
        .filter_map(|i| i.source_path.clone())
        .collect();

    for category in &catalog.categories {
        if category.folder.is_empty() {
            continue;
        }
        let folder_path = data_root().join(&category.folder);
        fs::create_dir_all(&folder_path).map_err(|e| e.to_string())?;
        let entries = match fs::read_dir(&folder_path) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.filter_map(|e| e.ok()) {
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if !ft.is_file() {
                continue;
            }
            let file_name = entry.file_name().to_string_lossy().to_string();
            if file_name == ".gitkeep" {
                continue;
            }
            let source_path = format!("{}/{}", category.folder, file_name);
            if known.contains(&source_path) {
                continue;
            }
            known.insert(source_path.clone());

            let stamp = now();
            let mut item = Item {
                id: new_id(),
                category: category.key.clone(),
                title: Path::new(&file_name)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                description: String::new(),
                tags: Vec::new(),
                source_path: Some(source_path.clone()),
                target: None,
                browser: None,
                arguments: None,
                working_directory: None,
                icon_location: None,
                url: None,
                status: "needs_metadata".into(),
                created_at: stamp.clone(),
                updated_at: stamp,
                open_count: 0,
                last_opened_at: String::new(),
                icon: None,
                order: None,
            };

            if category.inbox {
                item.status = "needs_metadata".into();
                if file_name.to_lowercase().ends_with(".url") {
                    item.target = read_url_shortcut(&entry.path());
                }
            } else if category.kind == "link" {
                item.target = read_url_shortcut(&entry.path());
                item.status = if item.target.as_deref().map_or(false, |t| !t.is_empty()) {
                    "complete".into()
                } else {
                    "needs_metadata".into()
                };
            } else if category.kind == "app" {
                item.title = shortcut_title(&entry.path());
                apply_shortcut_metadata(&mut item, &entry.path(), false, &category.folder);
                item.status = "complete".into();
            } else if category.kind == "file" {
                item.status = "complete".into();
                item.target = Some(entry.path().to_string_lossy().to_string());
            }

            catalog.items.push(item.clone());
            discovered.push(item);
        }
    }

    write_catalog(catalog)?;
    Ok((discovered, removed))
}

/* ----------------------------- 命令输入 ----------------------------- */

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NewCategoryInput {
    label: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    note: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCategoryInput {
    label: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    note: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCategoryInput {
    key: String,
    label: String,
    #[serde(default)]
    symbol: String,
    #[serde(default)]
    note: String,
    #[serde(default)]
    kind: String,
    #[serde(default)]
    order: Option<i64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteCategoryInput {
    key: String,
    #[serde(default)]
    force: bool,
    #[serde(default)]
    remove_folder: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutDetailInput {
    path: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateItemInput {
    category: String,
    title: String,
    description: String,
    tags: Vec<String>,
    #[serde(default)]
    extension: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    browser: String,
    #[serde(default)]
    path: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    overwrite: bool,
    #[serde(default)]
    new_category: Option<NewCategoryInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CompleteMetadataInput {
    id: String,
    title: String,
    description: String,
    tags: Vec<String>,
    category: String,
    #[serde(default)]
    target: String,
    #[serde(default)]
    browser: String,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    new_category: Option<NewCategoryInput>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateItemInput {
    id: String,
    title: String,
    description: String,
    tags: Vec<String>,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    clear_icon: bool,
    #[serde(default)]
    target: String,
    #[serde(default)]
    browser: String,
}

#[derive(Deserialize)]
struct OpenItem {
    id: String,
}
#[derive(Deserialize)]
struct HideItem {
    id: String,
}
#[derive(Deserialize)]
struct DeleteItem {
    id: String,
}
#[derive(Deserialize)]
struct ReorderItems {
    ids: Vec<String>,
}

/* ----------------------------- 命令实现 ----------------------------- */

#[tauri::command]
fn get_status() -> Value {
    json!({ "ok": true, "dataPath": data_root().to_string_lossy() })
}

#[tauri::command]
fn get_items() -> Value {
    let catalog = read_catalog();
    json!({ "items": catalog.items, "categories": catalog.categories, "kinds": KINDS })
}

#[tauri::command]
fn get_categories() -> Value {
    let catalog = read_catalog();
    json!({ "categories": catalog.categories })
}

/// 本机已安装的浏览器（不含「系统默认」这一项，条目 browser 为空即跟随系统）。
#[tauri::command]
fn list_browsers() -> Value {
    let browsers: Vec<Value> = detect_browsers()
        .into_iter()
        .map(|(key, label, path)| json!({ "key": key, "label": label, "path": path }))
        .collect();
    json!({ "browsers": browsers })
}

#[tauri::command]
fn scan_items() -> Result<Value, String> {
    let mut catalog = read_catalog();
    let (discovered, removed) = scan(&mut catalog)?;
    Ok(json!({
        "discovered": discovered,
        "removed": removed,
        "items": catalog.items,
        "categories": catalog.categories
    }))
}

/// 在 catalog 内新建分类（不入参 kind 校验外的其它逻辑），返回建好的分类。
fn create_category_in(catalog: &mut Catalog, nc: &NewCategoryInput) -> Result<Category, String> {
    let label = nc.label.trim().to_string();
    if label.is_empty() {
        return Err("分类名称不能为空。".into());
    }
    if catalog.categories.iter().any(|c| c.label == label) {
        return Err(format!("分类「{}」已存在。", label));
    }
    let folder = safe_folder_name(&label);
    if folder.is_empty() {
        return Err("这个名称不能用作文件夹名，换一个试试。".into());
    }
    if catalog.categories.iter().any(|c| c.folder == folder) {
        return Err(format!("已经有一个分类使用「{}」文件夹了。", folder));
    }
    let kind = if KINDS.contains(&nc.kind.as_str()) {
        nc.kind.clone()
    } else {
        "file".to_string()
    };
    let symbol = if nc.symbol.trim().is_empty() {
        "▤".to_string()
    } else {
        nc.symbol.trim().chars().take(2).collect()
    };
    let note = nc.note.chars().take(40).collect();
    let order = catalog.categories.iter().map(|c| c.order).max().unwrap_or(0) + 1;
    let category = Category {
        key: slug_key(&label),
        label: label.clone(),
        folder: folder.clone(),
        kind,
        symbol,
        note,
        order,
        builtin: false,
        inbox: false,
    };
    fs::create_dir_all(data_root().join(&folder)).map_err(|e| e.to_string())?;
    fs::write(data_root().join(&folder).join(".gitkeep"), "").map_err(|e| e.to_string())?;
    catalog.categories.push(category.clone());
    Ok(category)
}

#[tauri::command]
fn create_category(input: CreateCategoryInput) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let nc = NewCategoryInput {
        label: input.label,
        kind: input.kind,
        symbol: input.symbol,
        note: input.note,
    };
    let category = create_category_in(&mut catalog, &nc)?;
    write_catalog(&catalog)?;
    Ok(json!({ "category": category, "categories": catalog.categories }))
}

#[tauri::command]
fn update_category(input: UpdateCategoryInput) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let key = input.key.clone();
    let label = input.label.trim().to_string();
    if label.is_empty() {
        return Err("分类名称不能为空。".into());
    }
    if catalog
        .categories
        .iter()
        .any(|c| c.key != key && c.label == label)
    {
        return Err(format!("分类「{}」已存在。", label));
    }
    let idx = catalog
        .categories
        .iter()
        .position(|c| c.key == key)
        .ok_or("找不到该分类。")?;

    if !catalog.categories[idx].builtin && safe_folder_name(&label) != catalog.categories[idx].folder {
        let old_path = data_root().join(&catalog.categories[idx].folder);
        let new_folder = safe_folder_name(&label);
        let new_path = data_root().join(&new_folder);
        if !new_folder.is_empty() && !new_path.exists() {
            if fs::rename(&old_path, &new_path).is_ok() && new_path.exists() {
                let previous = catalog.categories[idx].folder.clone();
                catalog.categories[idx].folder = new_folder;
                for item in &mut catalog.items {
                    if item.category == key
                        && item
                            .source_path
                            .as_deref()
                            .map(|s| s.starts_with(&format!("{}/", previous)))
                            .unwrap_or(false)
                    {
                        item.source_path = Some(format!(
                            "{}{}",
                            catalog.categories[idx].folder,
                            &item.source_path.as_ref().unwrap()[previous.len()..]
                        ));
                    }
                }
            }
        }
    }

    if !input.symbol.trim().is_empty() {
        catalog.categories[idx].symbol = input.symbol.trim().chars().take(2).collect();
    }
    if !input.note.is_empty() {
        catalog.categories[idx].note = input.note.chars().take(40).collect();
    }
    if KINDS.contains(&input.kind.as_str()) && !catalog.categories[idx].builtin {
        catalog.categories[idx].kind = input.kind.clone();
    }
    if let Some(o) = input.order {
        catalog.categories[idx].order = o;
    }
    catalog.categories[idx].label = label;
    let result = catalog.categories[idx].clone();
    write_catalog(&catalog)?;
    Ok(json!({ "category": result, "categories": catalog.categories }))
}

#[tauri::command]
fn delete_category(input: DeleteCategoryInput) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let key = input.key.clone();
    let idx = catalog
        .categories
        .iter()
        .position(|c| c.key == key)
        .ok_or("找不到该分类。")?;
    let category = &catalog.categories[idx];
    if category.builtin {
        return Err("内置分类不能删除。".into());
    }
    let used: Vec<&Item> = catalog.items.iter().filter(|i| i.category == key).collect();
    if !used.is_empty() && !input.force {
        return Err(format!(
            "这个分类里还有 {} 个条目，先移走或勾选一并删除。",
            used.len()
        ));
    }
    for item in &catalog.items {
        if item.category == key {
            if let Some(p) = item.source_path.as_deref().and_then(source_join) {
                if p.exists() {
                    let _ = fs::remove_file(p);
                }
            }
        }
    }
    let folder = category.folder.clone();
    catalog.items.retain(|i| i.category != key);
    catalog.categories.retain(|c| c.key != key);
    if input.remove_folder {
        let _ = fs::remove_dir_all(data_root().join(&folder));
    }
    write_catalog(&catalog)?;
    Ok(json!({ "ok": true, "categories": catalog.categories }))
}

/* ----------------------------- 开始菜单应用清单 ----------------------------- */

fn start_menu_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home = env::var("USERPROFILE")
        .or_else(|_| env::var("HOME"))
        .unwrap_or_default();
    let program_data = env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into());
    let public = env::var("PUBLIC").unwrap_or_else(|_| "C:\\Users\\Public".into());
    let candidates = [
        PathBuf::from(&home)
            .join("AppData")
            .join("Roaming")
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu"),
        PathBuf::from(&program_data)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu"),
        PathBuf::from(&home).join("Desktop"),
        PathBuf::from(&public).join("Desktop"),
    ];
    for c in candidates {
        if c.exists() {
            roots.push(c);
        }
    }
    roots
}

#[derive(Serialize)]
struct ShortcutEntry {
    name: String,
    path: String,
    group: String,
}

fn walk_shortcuts(dir: &Path, found: &mut HashMap<String, ShortcutEntry>, group: &str, depth: usize) {
    if depth > 5 {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_dir() {
            walk_shortcuts(&path, found, group, depth + 1);
        } else if ft.is_file() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.to_lowercase().ends_with(".lnk") {
                let key = name.to_lowercase();
                if !found.contains_key(&key) {
                    let display = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .to_string();
                    found.insert(
                        key,
                        ShortcutEntry {
                            name: display,
                            path: path.to_string_lossy().to_string(),
                            group: group.to_string(),
                        },
                    );
                }
            }
        }
    }
}

fn collect_shortcuts() -> Vec<ShortcutEntry> {
    let mut found: HashMap<String, ShortcutEntry> = HashMap::new();
    for root in &start_menu_roots() {
        let lower = root.to_string_lossy().to_lowercase();
        let group = if lower.ends_with("desktop") {
            "桌面".to_string()
        } else if lower.contains("programdata") {
            "所有用户".to_string()
        } else {
            "当前用户".to_string()
        };
        walk_shortcuts(root, &mut found, &group, 0);
    }
    let mut list: Vec<ShortcutEntry> = found.into_values().collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list
}

#[tauri::command]
fn list_shortcuts() -> Value {
    json!({ "shortcuts": collect_shortcuts() })
}

#[tauri::command]
fn shortcut_detail(input: ShortcutDetailInput) -> Result<Value, String> {
    let target = &input.path;
    if target.is_empty() || !Path::new(target).exists() {
        return Err("文件不存在。".into());
    }
    let ext = Path::new(target)
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();
    if ext == "lnk" {
        let info = read_shortcut(Path::new(target));
        Ok(json!({
            "icon": info.icon_data_url,
            "detail": {
                "target": info.resolved_target,
                "targetExists": info.target_exists,
                "arguments": info.arguments,
                "workingDirectory": info.working_directory,
                "iconLocation": info.icon_location
            }
        }))
    } else {
        let icon = read_icon_data_url("", 0, target);
        Ok(json!({ "icon": icon }))
    }
}

/* ----------------------------- 条目增删改 ----------------------------- */

#[tauri::command]
fn create_item(input: CreateItemInput) -> Result<Value, String> {
    ensure_directories(&read_catalog().categories)?;
    if input.title.trim().is_empty() {
        return Err("名称不能为空。".into());
    }
    let mut catalog = read_catalog();
    let mut category = catalog.categories.iter().find(|c| c.key == input.category).cloned();
    if category.is_none() {
        if let Some(nc) = &input.new_category {
            if !nc.label.trim().is_empty() {
                let created = create_category_in(&mut catalog, nc)?;
                category = Some(created);
            }
        }
    }
    let category = category.ok_or("请选择一个有效的分类。")?;
    if category.inbox {
        return Err("请选择一个有效的分类。".into());
    }

    let stamp = now();
    let mut item = Item {
        id: new_id(),
        category: category.key.clone(),
        title: input.title.trim().to_string(),
        description: input.description.trim().to_string(),
        tags: input
            .tags
            .iter()
            .filter(|t| !t.is_empty())
            .take(20)
            .cloned()
            .collect(),
        source_path: None,
        target: None,
        browser: None,
        arguments: None,
        working_directory: None,
        icon_location: None,
        url: None,
        status: "complete".into(),
        created_at: stamp.clone(),
        updated_at: stamp,
        open_count: 0,
        last_opened_at: String::new(),
        icon: input.icon.filter(|x| x.len() <= 2 * 1024 * 1024),
        order: None,
    };

    if category.kind == "script" {
        let ext: String = input
            .extension
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .take(10)
            .collect();
        let file = format!(
            "{}.{}",
            safe_filename(&item.title),
            if ext.is_empty() { "txt".to_string() } else { ext }
        );
        item.source_path = Some(format!("{}/{}", category.folder, file));
        fs::write(data_root().join(&category.folder).join(&file), &input.content)
            .map_err(|e| e.to_string())?;
    } else if category.kind == "link" {
        let src = if !input.target.is_empty() {
            input.target.clone()
        } else {
            input.url.clone()
        };
        match normalize_url(&src) {
            Some(u) => item.target = Some(u),
            None => return Err("请输入有效的 http 或 https 网址。".into()),
        }
        item.browser = normalize_browser(&input.browser);
    } else if category.kind == "app" {
        let source = if !input.path.is_empty() {
            input.path.clone()
        } else {
            input.target.clone()
        };
        if source.is_empty() || !Path::new(&source).exists() {
            return Err("找不到该程序或快捷方式文件。".into());
        }
        apply_shortcut_metadata(&mut item, Path::new(&source), true, &category.folder);
        if item.target.is_none() {
            item.target = Some(source.clone());
        }
        if item.title.is_empty() {
            item.title = Path::new(&source)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
        }
    } else {
        // file
        let target_path = input.target.clone();
        if target_path.is_empty() {
            return Err("请填写文件或文件夹路径。".into());
        }
        item.target = Some(target_path.clone());
        if !Path::new(&target_path).exists() && !is_absolute_drive(&target_path) {
            return Err("请填写存在的文件或文件夹路径。".into());
        }
    }

    catalog.items.push(item.clone());
    write_catalog(&catalog)?;
    Ok(json!({ "item": item, "categories": catalog.categories }))
}

#[tauri::command]
fn complete_metadata(input: CompleteMetadataInput) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let idx = catalog
        .items
        .iter()
        .position(|i| i.id == input.id)
        .ok_or("找不到待处理条目。")?;
    if input.title.trim().is_empty() {
        return Err("名称不能为空。".into());
    }
    let mut category = catalog.categories.iter().find(|c| c.key == input.category).cloned();
    if category.is_none() {
        if let Some(nc) = &input.new_category {
            if !nc.label.trim().is_empty() {
                let created = create_category_in(&mut catalog, nc)?;
                category = Some(created);
            }
        }
    }
    let category = category.ok_or("请选择一个分类。")?;
    if category.inbox {
        return Err("请选择一个分类。".into());
    }

    let inbox_key = catalog
        .categories
        .iter()
        .find(|c| c.inbox)
        .map(|c| c.key.clone());

    if let Some(ik) = inbox_key {
        let needs_move = {
            let item = &catalog.items[idx];
            item.category == ik && item.source_path.is_some()
        };
        if needs_move {
            let source_path = catalog.items[idx].source_path.clone().unwrap();
            let old_file = source_join(&source_path);
            let file_name = Path::new(&source_path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string();
            let ext = Path::new(&source_path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase())
                .unwrap_or_default();
            let is_url = ext == "url";

            if category.kind == "link" && !is_url {
                return Err("「网址」分类只收 .url 快捷方式；其它文件建议选「文档」或「应用」。".into());
            }

            let new_source = format!("{}/{}", category.folder, file_name);
            let target_file = data_root().join(&category.folder).join(&file_name);
            fs::create_dir_all(data_root().join(&category.folder)).map_err(|e| e.to_string())?;

            if category.kind == "script" && !is_url {
                if let Some(of) = &old_file {
                    if of.exists() && !target_file.exists() {
                        let _ = fs::rename(of, &target_file);
                    }
                }
                catalog.items[idx].source_path = Some(new_source);
            } else if category.kind == "app" {
                if let Some(of) = &old_file {
                    if of.exists() && !target_file.exists() {
                        let _ = fs::rename(of, &target_file);
                    }
                }
                let tf = if target_file.exists() {
                    target_file.clone()
                } else {
                    old_file.clone().unwrap_or_default()
                };
                apply_shortcut_metadata(&mut catalog.items[idx], &tf, false, &category.folder);
                catalog.items[idx].source_path = Some(new_source);
            } else if category.kind == "file" || category.kind == "link" {
                if let Some(of) = &old_file {
                    if of.exists() && !target_file.exists() {
                        let _ = fs::rename(of, &target_file);
                    }
                }
                if category.kind == "link" {
                    catalog.items[idx].target = catalog.items[idx]
                        .target
                        .clone()
                        .or_else(|| read_url_shortcut(&target_file));
                }
                if category.kind == "file" {
                    catalog.items[idx].target = Some(target_file.to_string_lossy().to_string());
                }
                catalog.items[idx].source_path = Some(new_source);
            }
        }
    }

    let item = &mut catalog.items[idx];
    item.category = category.key.clone();
    item.title = input.title.trim().to_string();
    item.description = input.description.trim().to_string();
    item.tags = input
        .tags
        .iter()
        .filter(|t| !t.is_empty())
        .take(20)
        .cloned()
        .collect();
    if let Some(icon) = &input.icon {
        if !icon.is_empty() {
            item.icon = Some(icon.clone());
        }
    }
    let target = input.target.trim().to_string();
    if !target.is_empty() {
        item.target = Some(target);
    }
    if category.kind == "link" {
        item.browser = normalize_browser(&input.browser);
    }
    item.status = "complete".into();
    item.updated_at = now();
    let result = item.clone();
    write_catalog(&catalog)?;
    Ok(json!({ "item": result, "categories": catalog.categories }))
}

#[tauri::command]
fn update_item(input: UpdateItemInput) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let idx = catalog
        .items
        .iter()
        .position(|i| i.id == input.id)
        .ok_or("找不到条目。")?;
    if input.title.trim().is_empty() {
        return Err("名称不能为空。".into());
    }
    let kind = catalog
        .categories
        .iter()
        .find(|c| c.key == catalog.items[idx].category)
        .map(|c| c.kind.clone())
        .unwrap_or_else(|| "script".to_string());

    catalog.items[idx].title = input.title.trim().to_string();
    catalog.items[idx].description = input.description.trim().to_string();
    catalog.items[idx].tags = input
        .tags
        .iter()
        .filter(|t| !t.is_empty())
        .take(20)
        .cloned()
        .collect();
    if input.clear_icon {
        catalog.items[idx].icon = None;
    } else if let Some(icon) = input.icon {
        if icon.len() <= 2 * 1024 * 1024 {
            catalog.items[idx].icon = Some(icon);
        } else {
            return Err("图片文件过大，请选择 1.5MB 以下的图片。".into());
        }
    }
    let target = input.target.trim().to_string();
    if !target.is_empty() {
        if kind == "link" {
            match normalize_url(&target) {
                Some(u) => catalog.items[idx].target = Some(u),
                None => return Err("请输入有效的 http 或 https 网址。".into()),
            }
        } else if kind != "script" {
            catalog.items[idx].target = Some(target);
        }
    }
    /* browser 只对「网址」类型有意义：传空即清掉（回到跟随系统），其它类型忽略该字段。 */
    if kind == "link" {
        catalog.items[idx].browser = normalize_browser(&input.browser);
    }
    catalog.items[idx].updated_at = now();
    let result = catalog.items[idx].clone();
    write_catalog(&catalog)?;
    Ok(json!({ "item": result }))
}

#[tauri::command]
fn delete_item(input: DeleteItem) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let idx = catalog
        .items
        .iter()
        .position(|i| i.id == input.id)
        .ok_or("找不到条目。")?;
    let item = catalog.items.remove(idx);
    if let Some(path) = item.source_path.as_deref().and_then(source_join) {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }
    write_catalog(&catalog)?;
    Ok(json!({ "ok": true }))
}

#[tauri::command]
fn reorder_items(input: ReorderItems) -> Result<Value, String> {
    let mut catalog = read_catalog();
    for item in &mut catalog.items {
        if let Some(position) = input.ids.iter().position(|id| id == &item.id) {
            item.order = Some(position as u64);
        }
    }
    write_catalog(&catalog)?;
    Ok(json!({ "items": catalog.items }))
}

#[tauri::command]
fn hide_item(input: HideItem) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let idx = catalog
        .items
        .iter()
        .position(|i| i.id == input.id && i.status == "needs_metadata")
        .ok_or("找不到待补充条目。")?;
    catalog.items[idx].status = "hidden".into();
    catalog.items[idx].updated_at = now();
    let result = catalog.items[idx].clone();
    write_catalog(&catalog)?;
    Ok(json!({ "item": result }))
}

fn start_detached(target: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = Command::new("cmd");
        cmd.arg("/C")
            .arg("start")
            .arg("")
            .arg(target)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .creation_flags(0x0000_0008); // DETACHED_PROCESS
        cmd.spawn().map_err(|e| format!("无法启动：{}", e))?;
    }
    #[cfg(not(windows))]
    {
        Command::new("cmd")
            .arg("/C")
            .arg("start")
            .arg("")
            .arg(target)
            .spawn()
            .map_err(|e| format!("无法启动：{}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn open_item(input: OpenItem) -> Result<Value, String> {
    let mut catalog = read_catalog();
    let idx = catalog
        .items
        .iter()
        .position(|i| i.id == input.id)
        .ok_or("找不到该条目。")?;
    if catalog.items[idx].status != "complete" {
        return Err("请先补充该条目的分类信息。".into());
    }
    let category_key = catalog.items[idx].category.clone();
    let kind = catalog
        .categories
        .iter()
        .find(|c| c.key == category_key)
        .map(|c| c.kind.clone())
        .unwrap_or_else(|| "script".to_string());

    let target = match kind.as_str() {
        "script" => {
            let sp = catalog.items[idx]
                .source_path
                .as_deref()
                .and_then(source_join)
                .ok_or("找不到脚本文件。请先扫描并同步。")?;
            if !sp.is_file() {
                return Err("找不到脚本文件。请先扫描并同步。".into());
            }
            sp.to_string_lossy().to_string()
        }
        "link" => {
            let u = catalog.items[idx].target.as_deref().ok_or("网址无效。")?;
            if !has_scheme(u)
                || !(u.to_lowercase().starts_with("http://")
                    || u.to_lowercase().starts_with("https://"))
            {
                return Err("网址仅支持 http 或 https。".into());
            }
            u.to_string()
        }
        "app" => {
            let sp = catalog.items[idx].source_path.as_deref().and_then(source_join);
            let shortcut = sp.filter(|p| p.is_file());
            // 优先启动复制进来的 .lnk（启动参数/工作目录/管理员标记都原样保留）。
            let t = shortcut
                .map(|p| p.to_string_lossy().to_string())
                .or_else(|| catalog.items[idx].target.clone())
                .unwrap_or_default();
            if t.is_empty() || !Path::new(&t).exists() {
                return Err("找不到这个应用的目标文件，可能已被移动或卸载。".into());
            }
            t
        }
        _ => {
            let t = catalog.items[idx].target.clone().unwrap_or_default();
            if t.is_empty() || !Path::new(&t).exists() {
                return Err("找不到这个文件，可能已被移动。".into());
            }
            t
        }
    };

    /* 「网址」类型可以指定浏览器：指定了就按它启动；找不到那个浏览器时不算错误，
       回落系统默认并在返回值里带 warning 提示一次。 */
    let browser = if kind == "link" {
        catalog.items[idx]
            .browser
            .clone()
            .filter(|b| !b.trim().is_empty())
    } else {
        None
    };
    let mut warning = String::new();
    match browser {
        Some(wanted) => match resolve_browser_path(&wanted) {
            Some(exe) => start_with_browser(&exe, &target)?,
            None => {
                warning = "指定的浏览器在本机找不到，已改用系统默认浏览器。".into();
                start_detached(&target)?;
            }
        },
        None => start_detached(&target)?,
    }
    catalog.items[idx].open_count += 1;
    catalog.items[idx].last_opened_at = now();
    catalog.items[idx].updated_at = now();
    write_catalog(&catalog)?;
    Ok(if warning.is_empty() {
        json!({ "ok": true })
    } else {
        json!({ "ok": true, "warning": warning })
    })
}

/* ----------------------------- 入口 ----------------------------- */

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_status,
            get_items,
            get_categories,
            list_browsers,
            scan_items,
            create_category,
            update_category,
            delete_category,
            list_shortcuts,
            shortcut_detail,
            create_item,
            complete_metadata,
            update_item,
            delete_item,
            reorder_items,
            hide_item,
            open_item
        ])
        .run(tauri::generate_context!())
        .expect("启动客户端失败");
}
