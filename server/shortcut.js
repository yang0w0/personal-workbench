/**
 * ============================================================================
 * 模块 M2：Windows 快捷方式（.lnk）解析 + 图标提取
 * 接口文档：docs/interfaces/shortcut-lib.md      总纲：docs/INTERFACES.md
 *
 * 纯 Node 实现，零第三方依赖，也不调用 PowerShell / COM：
 *   - readShortcut()      解析 MS-SHLLINK 二进制，取出目标程序、启动参数、工作目录、图标位置等属性。
 *   - readIconDataUrl()   从 .ico / .exe / .dll 中取出图标，组装成合法的 ICO 后返回 data URL。
 *   - parseLnkBuffer() / icoFromPe() / expandEnvironment() / splitIconLocation() 为低层工具函数。
 *
 * 图标以 ICO 形式返回是刻意的：浏览器（含 WebView2）的 <img> 能直接渲染 ICO，
 * 前端再用 canvas 归一化成 128×128 PNG，从而避免在这里手写位图解码。
 *
 * 边界：
 *   - 本模块是纯函数库：不联网、不写盘、不认识 catalog.json，也绝不向调用方抛异常。
 *   - 只改本文件。调用方（M1）要改用法时，先更新 docs/interfaces/http-api.md。
 *   - 改解析逻辑后用 `node server/shortcut.js "<某个真实 .lnk>"` 跑一次命令行自测。
 * ============================================================================
 */

'use strict';

const fs = require('node:fs');
const path = require('node:path');

const LNK_CLSID = Buffer.from([
  0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00,
  0xc0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46
]);

const RT_ICON = 3;
const RT_GROUP_ICON = 14;

const ENVIRONMENT = {
  SystemRoot: process.env.SystemRoot || 'C:\\Windows',
  windir: process.env.windir || process.env.SystemRoot || 'C:\\Windows',
  SystemDrive: process.env.SystemDrive || 'C:',
  ProgramFiles: process.env.ProgramFiles || 'C:\\Program Files',
  'ProgramFiles(x86)': process.env['ProgramFiles(x86)'] || 'C:\\Program Files (x86)',
  ProgramData: process.env.ProgramData || 'C:\\ProgramData',
  LOCALAPPDATA: process.env.LOCALAPPDATA || '',
  APPDATA: process.env.APPDATA || '',
  USERPROFILE: process.env.USERPROFILE || '',
  TEMP: process.env.TEMP || '',
  TMP: process.env.TMP || ''
};

function expandEnvironment(value) {
  return String(value || '').replace(/%([^%]+)%/g, (whole, key) => {
    const hit = Object.keys(ENVIRONMENT).find((name) => name.toLowerCase() === String(key).toLowerCase());
    return hit && ENVIRONMENT[hit] ? ENVIRONMENT[hit] : whole;
  });
}

/** ANSI 字符串在老快捷方式里可能是系统代码页（简体中文 = GBK），优先用 ICU 解码。 */
function decodeAnsi(buffer) {
  try {
    return new TextDecoder('gbk').decode(buffer);
  } catch {
    return buffer.toString('latin1');
  }
}

/* ------------------------------------------------------------------ *
 * .lnk 解析
 * ------------------------------------------------------------------ */

function parseLnkBuffer(buf) {
  const info = {
    valid: false,
    target: '',
    arguments: '',
    workingDirectory: '',
    iconLocation: '',
    iconIndex: 0,
    description: '',
    name: '',
    relativePath: '',
    showCommand: 1,
    runAsAdmin: false,
    isFolder: false,
    candidates: []
  };
  if (!Buffer.isBuffer(buf) || buf.length < 76) return info;
  if (buf.readUInt32LE(0) !== 0x4c) return info;
  if (!buf.subarray(4, 20).equals(LNK_CLSID)) return info;
  info.valid = true;

  const flags = buf.readUInt32LE(0x14);
  const attributes = buf.readUInt32LE(0x18);
  info.iconIndex = buf.readInt32LE(0x38);
  info.showCommand = buf.readUInt32LE(0x3c);
  info.isFolder = (attributes & 0x10) !== 0;
  info.runAsAdmin = (flags & (1 << 13)) !== 0;

  const hasIdList = (flags & 1) !== 0;
  const hasLinkInfo = (flags & 2) !== 0;
  const isUnicode = (flags & 128) !== 0;

  let cursor = 76;
  let idListRegion = null;
  if (hasIdList) {
    if (cursor + 2 > buf.length) return info;
    const idListSize = buf.readUInt16LE(cursor);
    const start = cursor + 2;
    const end = Math.min(start + idListSize, buf.length);
    idListRegion = buf.subarray(start, end);
    cursor = end;
  }

  let linkInfoBase = -1;
  if (hasLinkInfo && cursor + 8 <= buf.length) {
    linkInfoBase = cursor;
    cursor += Math.max(buf.readUInt32LE(cursor), 0);
  }

  const readString = () => {
    if (cursor + 2 > buf.length) return '';
    const count = buf.readUInt16LE(cursor);
    cursor += 2;
    let value = '';
    if (isUnicode) {
      const end = Math.min(cursor + count * 2, buf.length);
      value = buf.toString('utf16le', cursor, end);
      cursor += count * 2;
    } else {
      const end = Math.min(cursor + count, buf.length);
      value = decodeAnsi(buf.subarray(cursor, end));
      cursor += count;
    }
    return value.replace(/\0.*$/s, '').trim();
  };

  if (flags & 4) info.name = readString();
  if (flags & 8) info.relativePath = readString();
  if (flags & 16) info.workingDirectory = readString();
  if (flags & 32) info.arguments = readString();
  if (flags & 64) info.iconLocation = readString();

  // LinkInfo 里的 LocalBasePath + CommonPathSuffix 通常就是最完整的绝对路径。
  if (linkInfoBase >= 0) {
    const flagsAt = buf.readUInt32LE(linkInfoBase + 8);
    if (flagsAt & 1) {
      const localOffset = buf.readUInt32LE(linkInfoBase + 16);
      const suffixOffset = buf.readUInt32LE(linkInfoBase + 24);
      if (localOffset) {
        const base = readNullTerminated(buf, linkInfoBase + localOffset, false);
        const suffix = suffixOffset ? readNullTerminated(buf, linkInfoBase + suffixOffset, false) : '';
        if (base) info.candidates.push(base + suffix);
        else if (suffix) info.candidates.push(suffix);
      }
    }
  }

  // 环境变量数据块里往往存着未展开的完整路径。
  for (const block of walkExtraDataBlocks(buf, cursor)) {
    if (block.signature === 0xa0000001) {
      const at = block.offset + 8;
      const wide = readNullTerminated(buf, at + 260, true);
      const ansi = readNullTerminated(buf, at, false);
      if (wide) info.candidates.push(expandEnvironment(wide));
      if (ansi) info.candidates.push(expandEnvironment(ansi));
    }
  }

  if (info.relativePath) info.candidates.push(info.relativePath);
  if (info.name && /^[a-zA-Z]:\\/.test(info.name)) info.candidates.push(info.name);
  // MSI 广播式快捷方式（如 LibreOffice / 格式工厂）没有 LinkInfo，只能从目标 ID 列表里捞路径。
  const fromIdList = pathFromIdList(idListRegion);
  if (fromIdList) info.candidates.unshift(fromIdList);

  info.candidates = info.candidates.map((item) => item.replace(/"/g, '').trim()).filter(Boolean);
  info.target = info.candidates[0] || '';
  return info;
}

function readNullTerminated(buf, offset, wide) {
  if (offset < 0 || offset >= buf.length) return '';
  if (wide) {
    let end = offset;
    while (end + 1 < buf.length && !(buf[end] === 0 && buf[end + 1] === 0)) end += 2;
    return buf.toString('utf16le', offset, end).trim();
  }
  let end = offset;
  while (end < buf.length && buf[end] !== 0) end += 1;
  return decodeAnsi(buf.subarray(offset, end)).trim();
}

function walkExtraDataBlocks(buf, cursor) {
  const blocks = [];
  let at = cursor;
  let guard = 0;
  while (at + 4 <= buf.length && guard < 64) {
    guard += 1;
    const size = buf.readUInt32LE(at);
    if (size < 4) break;
    if (at + size > buf.length) break;
    blocks.push({ signature: at + 8 <= buf.length ? buf.readUInt32LE(at + 4) : 0, offset: at, size });
    at += size;
  }
  return blocks;
}

/**
 * 目标 ID 列表（LinkTargetIDList）里以 UTF-16 存着完整的文件系统路径。
 * 这里不做完整的 shell item 解码，只在两种字节对齐下扫描形如 "C:\...\x.exe" 的路径并取最后一个，
 * 足以覆盖 MSI 广播式快捷方式（它们没有 LinkInfo，本地路径只存在于这里）。
 */
function pathFromIdList(region) {
  if (!region || region.length < 8) return '';
  const pattern = /[A-Za-z]:[\\/][^\u0000-\u001f\u202a-\u202e]{1,240}?\.(?:exe|com|bat|cmd|msc|cpl|pif|scr)/gi;
  for (const shift of [0, 1]) {
    const text = region.subarray(shift).toString('utf16le');
    const matches = text.match(pattern);
    if (matches && matches.length) {
      const cleaned = matches[matches.length - 1].replace(/[\\/]{2,}/g, '\\');
      if (cleaned.length < 320) return cleaned;
    }
  }
  return '';
}

/** 在当前 .lnk 所在目录下把相对路径补全（部分快捷方式只存相对路径）。 */
function resolveRelativeToLnk(lnkPath, candidate) {
  if (!candidate) return '';
  const expanded = expandEnvironment(candidate);
  if (/^[a-zA-Z]:[\\/]/.test(expanded) || expanded.startsWith('\\\\')) return expanded;
  try {
    return path.resolve(path.dirname(lnkPath), expanded);
  } catch {
    return expanded;
  }
}

/**
 * 解析快捷方式并挑出一个"最可能存在"的目标路径。
 * 依次尝试：LinkInfo 绝对路径 → 相对路径 → 环境变量块 → 文件夹/未解析目标的兜底。
 */
function readShortcut(lnkPath, options) {
  const iconMaxSize = Number.isFinite(options?.iconMaxSize) ? options.iconMaxSize : 128;
  let info = { valid: false, candidates: [], target: '', relativePath: '', iconLocation: '', iconIndex: 0, arguments: '', workingDirectory: '', description: '', name: '', isFolder: false, runAsAdmin: false, showCommand: 1 };
  try {
    info = parseLnkBuffer(fs.readFileSync(lnkPath));
  } catch {
    return { ...info, resolvedTarget: '', iconDataUrl: '', fallbackName: path.basename(lnkPath, path.extname(lnkPath)) };
  }

  const existing = [];
  const guesses = [];
  for (const candidate of info.candidates) {
    const resolved = resolveRelativeToLnk(lnkPath, candidate);
    if (!resolved) continue;
    if (fs.existsSync(resolved)) existing.push(resolved);
    else guesses.push(resolved);
  }
  const resolvedTarget = existing[0] || guesses[0] || '';
  const iconDataUrl = readIconDataUrl({
    iconLocation: info.iconLocation,
    iconIndex: info.iconIndex,
    target: resolvedTarget,
    maxSize: iconMaxSize
  });

  return {
    ...info,
    resolvedTarget,
    targetExists: existing.length > 0,
    resolvedCandidates: existing.concat(guesses),
    iconDataUrl,
    fallbackName: info.name || path.basename(lnkPath, path.extname(lnkPath))
  };
}

/* ------------------------------------------------------------------ *
 * 图标提取
 * ------------------------------------------------------------------ */

/** 解析 "C:\x\y.exe,3" / "%SystemRoot%\system32\shell32.dll,-16" 这类图标位置写法。 */
function splitIconLocation(iconLocation, fallbackIndex) {
  let value = expandEnvironment(String(iconLocation || '').trim().replace(/^"|"$/g, ''));
  let index = Number.isFinite(fallbackIndex) ? fallbackIndex : 0;
  if (!value) return { file: '', index };
  // 只把最后一段 ",数字" 当作索引，避免误伤带逗号的路径。
  const match = value.match(/^(.*),(-?\d+)$/);
  if (match) {
    value = match[1].replace(/^"|"$/g, '');
    const parsed = Number.parseInt(match[2], 10);
    if (parsed !== 0 || fallbackIndex === 0) index = parsed;
  }
  if (!/^[a-zA-Z]:[\\/]/.test(value) && !value.startsWith('\\\\')) {
    const percentFree = value.replace(/\\/g, path.sep);
    value = percentFree;
  }
  if (!path.isAbsolute(value)) value = path.join(ENVIRONMENT.SystemRoot, value);
  return { file: value, index };
}

function mimeFor(extension) {
  switch (extension) {
    case '.png': return 'image/png';
    case '.jpg':
    case '.jpeg': return 'image/jpeg';
    case '.gif': return 'image/gif';
    case '.bmp': return 'image/bmp';
    case '.svg': return 'image/svg+xml';
    case '.webp': return 'image/webp';
    default: return 'image/x-icon';
  }
}

function toDataUrl(buffer, extension) {
  if (!buffer || !buffer.length) return '';
  return `data:${mimeFor(extension)};base64,${buffer.toString('base64')}`;
}

/** 把 PE 文件里的图标资源拼装成标准 .ico 字节流；失败返回 null。 */
function icoFromPe(buf, wantedIndex, maxSize = 128) {
  if (!Buffer.isBuffer(buf) || buf.length < 0x40) return null;
  if (buf.readUInt16LE(0) !== 0x5a4d) return null; // "MZ"
  const peOffset = buf.readUInt32LE(0x3c);
  if (peOffset + 24 > buf.length) return null;
  if (buf.toString('latin1', peOffset, peOffset + 4) !== 'PE\0\0') return null;

  const coff = peOffset + 4;
  const sectionCount = buf.readUInt16LE(coff + 2);
  const optionalSize = buf.readUInt16LE(coff + 16);
  const optional = coff + 20;
  if (optional + optionalSize > buf.length) return null;
  const magic = buf.readUInt16LE(optional);
  const dataDirectory = optional + (magic === 0x20b ? 112 : 96);
  if (dataDirectory + 16 > buf.length) return null;

  const resourceRva = buf.readUInt32LE(dataDirectory + 2 * 8);
  if (!resourceRva) return null;

  const sections = [];
  const sectionStart = optional + optionalSize;
  for (let index = 0; index < sectionCount; index += 1) {
    const at = sectionStart + index * 40;
    if (at + 40 > buf.length) break;
    sections.push({
      virtualSize: buf.readUInt32LE(at + 8),
      virtualAddress: buf.readUInt32LE(at + 12),
      rawSize: buf.readUInt32LE(at + 16),
      rawPointer: buf.readUInt32LE(at + 20)
    });
  }
  const toOffset = (rva) => {
    for (const section of sections) {
      const span = Math.max(section.virtualSize, section.rawSize);
      if (rva >= section.virtualAddress && rva < section.virtualAddress + span) {
        return section.rawPointer + (rva - section.virtualAddress);
      }
    }
    // 有些文件的资源 RVA 就在文件头之后，直接当偏移用。
    return rva < buf.length ? rva : -1;
  };

  const resourceBase = toOffset(resourceRva);
  if (resourceBase < 0 || resourceBase + 16 > buf.length) return null;

  const readDirectory = (relative) => {
    const at = resourceBase + relative;
    if (at + 16 > buf.length) return [];
    const named = buf.readUInt16LE(at + 12);
    const ids = buf.readUInt16LE(at + 14);
    const entries = [];
    for (let index = 0; index < named + ids; index += 1) {
      const entry = at + 16 + index * 8;
      if (entry + 8 > buf.length) break;
      const nameField = buf.readUInt32LE(entry);
      const offsetField = buf.readUInt32LE(entry + 4);
      entries.push({
        named: (nameField & 0x80000000) !== 0,
        id: nameField & 0x7fffffff,
        isDirectory: (offsetField & 0x80000000) !== 0,
        offset: offsetField & 0x7fffffff
      });
    }
    return entries;
  };

  const readResourceData = (languageEntryOffset) => {
    const at = resourceBase + languageEntryOffset;
    if (at + 16 > buf.length) return null;
    const rva = buf.readUInt32LE(at);
    const size = buf.readUInt32LE(at + 4);
    const start = toOffset(rva);
    if (start < 0 || start + size > buf.length) return null;
    return buf.subarray(start, start + size);
  };

  const iconRoot = readDirectory(0).find((entry) => entry.isDirectory && entry.id === RT_ICON);
  if (!iconRoot) return null;
  // 第二层才是「图标 ID → 语言」的映射表。
  const iconIndex = readDirectory(iconRoot.offset);
  let groups = readDirectory(0).filter((entry) => entry.isDirectory && entry.id === RT_GROUP_ICON);
  if (!groups.length) return null;
  groups = groups.slice().sort((a, b) => a.id - b.id);

  let group = groups[0];
  if (Number.isFinite(wantedIndex) && wantedIndex < 0) {
    const byId = groups.find((entry) => entry.id === Math.abs(wantedIndex));
    if (byId) group = byId;
  } else if (Number.isFinite(wantedIndex) && wantedIndex > 0 && groups[wantedIndex]) {
    group = groups[wantedIndex];
  }

  const groupEntry = readDirectory(group.offset).find((entry) => entry.isDirectory);
  if (!groupEntry) return null;
  const languageEntry = readDirectory(groupEntry.offset).find((entry) => !entry.isDirectory);
  if (!languageEntry) return null;
  const groupData = readResourceData(languageEntry.offset);
  if (!groupData || groupData.length < 6) return null;
  const count = groupData.readUInt16LE(4);
  if (count < 1 || groupData.length < 6 + count * 14) return null;

  const images = [];
  for (let index = 0; index < count; index += 1) {
    const entryAt = 6 + index * 14;
    const resourceId = groupData.readUInt16LE(entryAt + 12);
    const iconDirectory = iconIndex.find((entry) => entry.id === resourceId);
    if (!iconDirectory) continue;
    const iconLanguage = readDirectory(iconDirectory.offset).find((entry) => !entry.isDirectory);
    if (!iconLanguage) continue;
    const blob = readResourceData(iconLanguage.offset);
    if (!blob || !blob.length) continue;
    images.push({
      width: groupData.readUInt8(entryAt),
      height: groupData.readUInt8(entryAt + 1),
      colorCount: groupData.readUInt8(entryAt + 2),
      planes: groupData.readUInt16LE(entryAt + 4),
      bitCount: groupData.readUInt16LE(entryAt + 6),
      data: blob
    });
  }
  if (!images.length) return null;

  // 图标只在 42px 的格子里显示，没必要把 256px 及以上的大图也塞进 data URL。
  // 首选不超过 maxSize 的最大一档；都没有就退回最小的一档。
  const sized = images.filter((image) => (image.width === 0 ? 256 : image.width) <= maxSize);
  const chosen = sized.length
    ? sized.sort((a, b) => (b.width === 0 ? 256 : b.width) - (a.width === 0 ? 256 : a.width))
    : [images.slice().sort((a, b) => (a.width === 0 ? 256 : a.width) - (b.width === 0 ? 256 : b.width))[0]];

  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(chosen.length, 4);
  const directory = Buffer.alloc(chosen.length * 16);
  let offset = 6 + chosen.length * 16;
  chosen.forEach((image, index) => {
    const at = index * 16;
    directory.writeUInt8(image.width, at);
    directory.writeUInt8(image.height, at + 1);
    directory.writeUInt8(image.colorCount, at + 2);
    directory.writeUInt8(0, at + 3);
    directory.writeUInt16LE(image.planes || 1, at + 4);
    directory.writeUInt16LE(image.bitCount || 32, at + 6);
    directory.writeUInt32LE(image.data.length, at + 8);
    directory.writeUInt32LE(offset, at + 12);
    offset += image.data.length;
  });
  return Buffer.concat([header, directory, ...chosen.map((image) => image.data)]);
}

function looksLikeIco(buf) {
  return Buffer.isBuffer(buf) && buf.length > 22 && buf.readUInt16LE(0) === 0 && buf.readUInt16LE(2) === 1 && buf.readUInt16LE(4) > 0;
}

/**
 * 依次尝试图标位置文件、目标程序、目标同目录同名 .ico，返回可直接给 <img> 用的 data URL。
 * 全部失败时返回空串——前端的字母头像比一张认错的系统图标更诚实。
 */
function readIconDataUrl({ iconLocation, iconIndex, target, maxSize = 128 }) {
  const tried = new Set();
  const attempt = (filePath, index) => {
    if (!filePath) return '';
    const key = `${filePath}|${index}`;
    if (tried.has(key)) return '';
    tried.add(key);
    let buf;
    try {
      buf = fs.readFileSync(filePath);
    } catch {
      return '';
    }
    const extension = path.extname(filePath).toLowerCase();
    if (extension === '.ico') return looksLikeIco(buf) ? toDataUrl(buf, '.ico') : '';
    if (['.png', '.jpg', '.jpeg', '.gif', '.bmp', '.webp'].includes(extension)) return toDataUrl(buf, extension);
    if (['.exe', '.dll', '.scr', '.cpl', '.msc', '.ocx'].includes(extension) || (buf.length > 2 && buf.readUInt16LE(0) === 0x5a4d)) {
      const ico = icoFromPe(buf, index, maxSize);
      return ico ? toDataUrl(ico, '.ico') : '';
    }
    return '';
  };

  const fromLocation = splitIconLocation(iconLocation, iconIndex);
  const direct = attempt(fromLocation.file, fromLocation.index);
  if (direct) return direct;

  if (target) {
    const fromTarget = attempt(target, 0);
    if (fromTarget) return fromTarget;
    const parsed = path.parse(target);
    const fromBeside = attempt(path.join(parsed.dir, `${parsed.name}.ico`), 0);
    if (fromBeside) return fromBeside;
  }
  return '';
}

module.exports = {
  parseLnkBuffer,
  readShortcut,
  readIconDataUrl,
  icoFromPe,
  expandEnvironment,
  splitIconLocation
};

/* 命令行自测：node server/shortcut.js "C:\path\to\app.lnk" */
if (require.main === module) {
  const target = process.argv[2];
  if (!target) {
    console.log('用法：node server/shortcut.js <快捷方式.lnk|程序.exe>');
    process.exit(1);
  }
  if (target.toLowerCase().endsWith('.lnk')) {
    const info = readShortcut(target);
    const { iconDataUrl, resolvedCandidates, ...rest } = info;
    console.log(JSON.stringify({ ...rest, iconBytes: iconDataUrl.length, resolvedCandidates }, null, 2));
  } else {
    const ico = icoFromPe(fs.readFileSync(target), 0);
    console.log(ico ? `ICO ok, ${ico.length} bytes` : 'ICO 提取失败');
  }
}
