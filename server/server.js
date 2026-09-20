/**
 * ============================================================================
 * 模块 M1：浏览器后端（本机 HTTP 服务，127.0.0.1:8765）
 * 接口文档：docs/interfaces/http-api.md      总纲：docs/INTERFACES.md
 *
 * 职责：扫描 data/ 分类目录、读写 catalog.json、打开脚本/网址/应用/文档。
 * 外部依赖：./shortcut.js（模块 M2，只调用不修改）。
 *
 * 边界：
 *   - 改后端只改本文件；数据结构以 docs/interfaces/data-catalog.md 为准。
 *   - 不要在这里实现 .lnk 解析或图标提取 —— 那是 M2，调用 readShortcut()/readIconDataUrl()。
 *   - 不要引入 npm 依赖；不要把监听地址改成 127.0.0.1 之外。
 *   - data/ 下的路径必须经过 itemFilePath() 校验（只允许两层相对路径）。
 *
 * 新增/修改接口时，必须在同一次改动里更新 docs/interfaces/http-api.md。
 * 改动会影响前端（M4）时，请在回复中明确提示需要同步前端映射表。
 * ============================================================================
 */

'use strict';

const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const os = require('node:os');
const { spawn } = require('node:child_process');
const { readShortcut, readIconDataUrl } = require('./shortcut.js');

const ROOT = path.resolve(__dirname, '..');
const DATA = path.join(ROOT, 'data');
const PORT = 8765;
const CATALOG_PATH = path.join(DATA, 'catalog.json');

const KIND_SCRIPT = 'script';
const KIND_LINK = 'link';
const KIND_APP = 'app';
const KIND_FILE = 'file';
const KINDS = [KIND_SCRIPT, KIND_LINK, KIND_APP, KIND_FILE];

/** 分类注册表：kind 决定编辑器长什么样、扫描时归到哪、点击后怎么打开。 */
const DEFAULT_CATEGORIES = [
  { key: 'script', label: '脚本', folder: '脚本', kind: KIND_SCRIPT, symbol: '▣', note: '点击运行 · 可拖动排序', order: 0, builtin: true },
  { key: 'link', label: '网址', folder: '网址', kind: KIND_LINK, symbol: '↗', note: '点击打开 · 可拖动排序', order: 1, builtin: true },
  { key: 'app', label: '应用', folder: '应用', kind: KIND_APP, symbol: '◈', note: '点击启动 · 保留原快捷方式', order: 2, builtin: true },
  { key: 'file', label: '文档', folder: '文档', kind: KIND_FILE, symbol: '▤', note: '点击用默认程序打开', order: 3, builtin: true },
  { key: 'inbox', label: '待整理', folder: '待整理', kind: 'inbox', symbol: '!', note: '补充分类后变成快捷图标', order: 90, builtin: true, inbox: true }
];

function defaultCatalog() {
  return { version: 2, categories: DEFAULT_CATEGORIES.map((entry) => ({ ...entry })), items: [] };
}

function slugKey(label) {
  return `cat-${crypto.createHash('sha1').update(String(label)).digest('hex').slice(0, 8)}`;
}

function safeFolderName(value) {
  const cleaned = String(value || '')
    .trim()
    .replace(/[<>:"/\\|?*\x00-\x1F]/g, '')
    .replace(/\.+$/g, '')
    .replace(/\s+/g, ' ');
  return (cleaned || '').slice(0, 24);
}

function safeFilename(value) {
  const cleaned = String(value || 'untitled').trim().replace(/[<>:"/\\|?*\x00-\x1F]/g, '-').replace(/\.+$/g, '');
  return (cleaned || 'untitled').slice(0, 80);
}

/* ------------------------------------------------------------------ *
 * 目录与目录读写
 * ------------------------------------------------------------------ */

function ensureDirectories(categories) {
  for (const category of categories) {
    if (!category.folder) continue;
    const folder = path.join(DATA, category.folder);
    fs.mkdirSync(folder, { recursive: true });
    // git 不跟踪空目录，放个占位文件让分类文件夹能被提交。
    const keep = path.join(folder, '.gitkeep');
    if (!fs.existsSync(keep)) { try { fs.writeFileSync(keep, ''); } catch { /* 忽略只读目录 */ } }
  }
}

function normalizeCategory(raw, index) {
  const label = String(raw?.label || '').trim().slice(0, 24);
  if (!label) return null;
  return {
    key: typeof raw?.key === 'string' && raw.key ? raw.key : slugKey(label),
    label,
    folder: safeFolderName(raw?.folder || label) || label,
    kind: raw?.inbox === true ? 'inbox' : (KINDS.includes(raw?.kind) ? raw.kind : KIND_FILE),
    symbol: typeof raw?.symbol === 'string' && raw.symbol.trim() ? raw.symbol.trim().slice(0, 2) : '▤',
    note: typeof raw?.note === 'string' ? raw.note.slice(0, 40) : '',
    order: Number.isFinite(raw?.order) ? raw.order : index,
    builtin: raw?.builtin === true,
    inbox: raw?.inbox === true
  };
}

function readCatalog() {
  let parsed = null;
  try {
    parsed = JSON.parse(fs.readFileSync(CATALOG_PATH, 'utf8'));
  } catch {
    parsed = null;
  }
  if (!parsed || !Array.isArray(parsed.items)) return defaultCatalog();

  const catalog = { version: 2, categories: [], items: parsed.items };

  // 首次升级：老目录没有 categories，补上默认分类；用户已有的自定义文件夹也一并收录。
  const source = Array.isArray(parsed.categories) && parsed.categories.length ? parsed.categories : DEFAULT_CATEGORIES;
  catalog.categories = source.map((entry, index) => normalizeCategory(entry, index)).filter(Boolean);
  for (const name of listDataFolders()) {
    if (catalog.categories.some((entry) => entry.folder === name)) continue;
    const fallback = DEFAULT_CATEGORIES.find((entry) => entry.folder === name);
    if (fallback) catalog.categories.push(normalizeCategory(fallback, catalog.categories.length));
    else if (!catalog.categories.some((entry) => entry.key === slugKey(name))) {
      catalog.categories.push({ key: slugKey(name), label: name, folder: name, kind: KIND_FILE, symbol: '▤', note: '', order: catalog.categories.length, builtin: false, inbox: false });
    }
  }
  if (!catalog.categories.some((entry) => entry.inbox)) {
    const inbox = DEFAULT_CATEGORIES.find((entry) => entry.inbox);
    catalog.categories.push(normalizeCategory(inbox, 90));
  }

  // 老数据把网址存在 url 字段，统一迁移到 target。
  for (const item of catalog.items) {
    if (!item.target && item.url) item.target = item.url;
    if (typeof item.icon !== 'string') item.icon = '';
  }
  return catalog;
}

function listDataFolders() {
  try {
    return fs.readdirSync(DATA, { withFileTypes: true })
      .filter((entry) => entry.isDirectory() && !entry.name.startsWith('.'))
      .map((entry) => entry.name);
  } catch {
    return [];
  }
}

function writeCatalog(catalog) {
  const temporary = `${CATALOG_PATH}.tmp`;
  fs.writeFileSync(temporary, `${JSON.stringify(catalog, null, 2)}\n`, 'utf8');
  fs.renameSync(temporary, CATALOG_PATH);
}

function categoryOf(catalog, key) {
  return catalog.categories.find((entry) => entry.key === key) || null;
}

function inboxCategory(catalog) {
  return catalog.categories.find((entry) => entry.inbox) || null;
}

function findItem(catalog, id) {
  return catalog.items.find((entry) => entry.id === id) || null;
}

/** 只允许访问 data/ 下的两层相对路径（分类文件夹/文件名）。 */
function itemFilePath(relative) {
  if (typeof relative !== 'string' || relative.split(/[\\/]/).length !== 2) return null;
  const resolved = path.resolve(DATA, relative);
  return resolved.startsWith(`${DATA}${path.sep}`) ? resolved : null;
}

function itemAbsolutePath(item) {
  const base = itemFilePath(item.sourcePath);
  if (base) return base;
  if (typeof item.target === 'string' && path.isAbsolute(item.target)) return item.target;
  return null;
}

/* ------------------------------------------------------------------ *
 * 扫描
 * ------------------------------------------------------------------ */

function scan(catalog) {
  const discovered = [];
  const removed = [];

  // 外部删掉的文件同步出目录。
  catalog.items = catalog.items.filter((item) => {
    if (!item.sourcePath) return true;
    const filePath = itemFilePath(item.sourcePath);
    if (!filePath) return true;
    if (fs.existsSync(filePath)) return true;
    removed.push(item);
    return false;
  });

  const known = new Set(catalog.items.filter((item) => item.sourcePath).map((item) => item.sourcePath));

  for (const category of catalog.categories) {
    if (!category.folder) continue;
    const folderPath = path.join(DATA, category.folder);
    fs.mkdirSync(folderPath, { recursive: true });
    let entries = [];
    try {
      entries = fs.readdirSync(folderPath, { withFileTypes: true });
    } catch {
      continue;
    }
    for (const entry of entries) {
      if (!entry.isFile() || entry.name === '.gitkeep') continue;
      const sourcePath = `${category.folder}/${entry.name}`;
      if (known.has(sourcePath)) continue;
      known.add(sourcePath);
      const title = category.kind === KIND_APP
        ? shortcutTitle(path.join(folderPath, entry.name)) || path.parse(entry.name).name
        : path.parse(entry.name).name;
      const item = {
        id: crypto.randomUUID(),
        category: category.key,
        title,
        description: '',
        tags: [],
        sourcePath,
        icon: '',
        status: 'needs_metadata',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        openCount: 0,
        lastOpenedAt: ''
      };
      if (category.inbox) {
        item.status = 'needs_metadata';
        if (entry.name.toLowerCase().endsWith('.url')) item.target = readUrlShortcut(path.join(folderPath, entry.name));
      } else if (category.kind === KIND_LINK) {
        item.target = readUrlShortcut(path.join(folderPath, entry.name));
        item.status = item.target ? 'complete' : 'needs_metadata';
      } else if (category.kind === KIND_APP) {
        applyShortcutMetadata(item, path.join(folderPath, entry.name), { copy: false });
        item.status = 'complete';
      } else if (category.kind === KIND_FILE) {
        item.status = 'complete';
        item.target = path.join(folderPath, entry.name);
      }
      catalog.items.push(item);
      discovered.push(item);
    }
  }
  writeCatalog(catalog);
  return { discovered, removed };
}

function shortcutTitle(filePath) {
  if (!filePath.toLowerCase().endsWith('.lnk')) return '';
  try {
    const info = readShortcut(filePath, { iconMaxSize: 32 });
    const name = path.parse(filePath).name;
    // LNK 里的 NAME_STRING 常常是描述而不是名字，只有明显不像描述时才采用。
    if (info.name && info.name.length <= 40 && !/http|https|\.com|参见/.test(info.name)) return info.name;
    return name;
  } catch {
    return '';
  }
}

function readUrlShortcut(filePath) {
  try {
    const match = fs.readFileSync(filePath, 'utf8').match(/^URL=(.+)$/mi);
    return match ? match[1].trim() : '';
  } catch {
    return '';
  }
}

/** 把快捷方式的属性写进条目，必要时把 .lnk 复制进 data/应用/ 以保留全部行为。 */
function applyShortcutMetadata(item, sourceFilePath, { copy }) {
  const extension = path.extname(sourceFilePath).toLowerCase();
  if (extension === '.lnk') {
    // 写进目录时用 64px 就够：图标只在 42px 的格子里显示，128 的 DIB 会让 catalog.json 膨胀十倍。
    const info = readShortcut(sourceFilePath, { iconMaxSize: 64 });
    item.target = info.resolvedTarget || '';
    item.arguments = info.arguments || '';
    item.workingDirectory = info.workingDirectory || '';
    item.iconLocation = info.iconLocation || '';
    if (copy) {
      const folder = itemFilePath(item.sourcePath) ? path.dirname(itemFilePath(item.sourcePath)) : path.join(DATA, item._folder || '应用');
      let fileName = safeFilename(path.parse(sourceFilePath).name) + '.lnk';
      let target = path.join(folder, fileName);
      let suffix = 1;
      while (fs.existsSync(target) && path.resolve(target) !== path.resolve(sourceFilePath)) {
        fileName = `${safeFilename(path.parse(sourceFilePath).name)} (${suffix}).lnk`;
        target = path.join(folder, fileName);
        suffix += 1;
      }
      if (path.resolve(target) !== path.resolve(sourceFilePath)) fs.copyFileSync(sourceFilePath, target);
      item.sourcePath = `${path.basename(folder)}/${fileName}`;
    }
    if (!item.icon) item.icon = info.iconDataUrl || '';
    if (!item.title) item.title = info.name || path.parse(sourceFilePath).name;
    if (!item.target) item.target = info.resolvedTarget || sourceFilePath;
    return;
  }
  if (!item.target) item.target = sourceFilePath;
  if (!item.icon) item.icon = readIconDataUrl({ iconLocation: '', iconIndex: 0, target: sourceFilePath, maxSize: 64 });
}

/* ------------------------------------------------------------------ *
 * 开始菜单应用清单
 * ------------------------------------------------------------------ */

function startMenuRoots() {
  const roots = [];
  const home = os.homedir();
  const programData = process.env.ProgramData || 'C:\\ProgramData';
  const candidates = [
    path.join(home, 'AppData', 'Roaming', 'Microsoft', 'Windows', 'Start Menu'),
    path.join(programData, 'Microsoft', 'Windows', 'Start Menu'),
    path.join(home, 'Desktop'),
    path.join(process.env.PUBLIC || 'C:\\Users\\Public', 'Desktop')
  ];
  for (const candidate of candidates) {
    if (fs.existsSync(candidate)) roots.push(candidate);
  }
  return roots;
}

function collectShortcuts() {
  const found = new Map();
  const walk = (directory, depth) => {
    if (depth > 5) return;
    let entries = [];
    try {
      entries = fs.readdirSync(directory, { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) {
        walk(full, depth + 1);
      } else if (entry.isFile() && entry.name.toLowerCase().endsWith('.lnk')) {
        const key = entry.name.toLowerCase();
        const group = path.relative(directory, full).split(path.sep)[0];
        if (!found.has(key)) {
          found.set(key, { name: path.parse(entry.name).name, path: full, group: '' });
        }
      }
    }
  };
  for (const root of startMenuRoots()) walk(root, 0);
  const list = [...found.values()];
  // 归类到"程序 / 桌面"，并按名字排好，供前端搜索。
  for (const root of startMenuRoots()) {
    const isDesktop = /Desktop$/i.test(root);
    for (const entry of list) {
      if (!entry.group && entry.path.startsWith(root)) entry.group = isDesktop ? '桌面' : (/ProgramData/i.test(root) ? '所有用户' : '当前用户');
    }
  }
  return list.sort((a, b) => a.name.localeCompare(b.name, 'zh-Hans-CN'));
}

/* ------------------------------------------------------------------ *
 * HTTP
 * ------------------------------------------------------------------ */

function send(response, status, payload) {
  response.writeHead(status, {
    'Content-Type': 'application/json; charset=utf-8',
    'Access-Control-Allow-Origin': '*',
    'Access-Control-Allow-Methods': 'GET, POST, OPTIONS',
    'Access-Control-Allow-Headers': 'Content-Type',
    'Cache-Control': 'no-store'
  });
  response.end(JSON.stringify(payload));
}

function readBody(request) {
  return new Promise((resolve, reject) => {
    let body = '';
    request.on('data', (chunk) => {
      body += chunk;
      if (body.length > 8 * 1024 * 1024) reject(new Error('请求过大'));
    });
    request.on('end', () => {
      try { resolve(body ? JSON.parse(body) : {}); } catch { reject(new Error('JSON 格式无效')); }
    });
  });
}

function normalizeTags(value) {
  return Array.isArray(value) ? value.map(String).map((tag) => tag.trim()).filter(Boolean).slice(0, 20) : [];
}

function normalizeIcon(value) {
  return typeof value === 'string' && value.startsWith('data:image/') ? value.slice(0, 2 * 1024 * 1024) : '';
}

function startDetached(target) {
  const child = spawn('cmd.exe', ['/c', 'start', '', target], { detached: true, stdio: 'ignore', windowsHide: true });
  child.unref();
}

/** 新建分类：注册表加一条，同时在 data/ 下真的建出文件夹。 */
function createCategory(catalog, input) {
  const label = String(input?.label || '').trim();
  if (!label) return { error: '分类名称不能为空。' };
  if (catalog.categories.some((entry) => entry.label === label)) return { error: `分类「${label}」已存在。` };
  const folder = safeFolderName(label);
  if (!folder) return { error: '这个名称不能用作文件夹名，换一个试试。' };
  if (catalog.categories.some((entry) => entry.folder === folder)) return { error: `已经有一个分类使用「${folder}」文件夹了。` };
  const category = {
    key: slugKey(label),
    label,
    folder,
    kind: KINDS.includes(input?.kind) ? input.kind : KIND_FILE,
    symbol: typeof input?.symbol === 'string' && input.symbol.trim() ? input.symbol.trim().slice(0, 2) : '▤',
    note: typeof input?.note === 'string' ? input.note.slice(0, 40) : '',
    order: Math.max(...catalog.categories.map((entry) => Number(entry.order) || 0), 0) + 1,
    builtin: false,
    inbox: false
  };
  fs.mkdirSync(path.join(DATA, category.folder), { recursive: true });
  fs.writeFileSync(path.join(DATA, category.folder, '.gitkeep'), '');
  catalog.categories.push(category);
  return { category };
}

const server = http.createServer(async (request, response) => {
  if (request.method === 'OPTIONS') return send(response, 204, {});
  const url = new URL(request.url, `http://${request.headers.host}`);
  try {
    if (request.method === 'GET' && url.pathname === '/api/status') {
      return send(response, 200, { ok: true, root: ROOT, dataPath: DATA });
    }

    if (request.method === 'GET' && url.pathname === '/api/items') {
      const catalog = readCatalog();
      return send(response, 200, { items: catalog.items, categories: catalog.categories, kinds: KINDS });
    }

    if (request.method === 'POST' && url.pathname === '/api/scan') {
      const catalog = readCatalog();
      ensureDirectories(catalog.categories);
      const result = scan(catalog);
      return send(response, 200, { ...result, items: catalog.items, categories: catalog.categories });
    }

    /* ---------------- 分类管理 ---------------- */

    if (request.method === 'GET' && url.pathname === '/api/categories') {
      const catalog = readCatalog();
      return send(response, 200, { categories: catalog.categories });
    }

    if (request.method === 'POST' && url.pathname === '/api/categories') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const result = createCategory(catalog, input);
      if (result.error) return send(response, 400, { error: result.error });
      writeCatalog(catalog);
      return send(response, 201, { category: result.category, categories: catalog.categories });
    }

    if (request.method === 'POST' && url.pathname === '/api/categories/update') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const category = categoryOf(catalog, String(input.key || ''));
      if (!category) return send(response, 404, { error: '找不到该分类。' });
      const label = String(input.label || '').trim();
      if (!label) return send(response, 400, { error: '分类名称不能为空。' });
      const conflict = catalog.categories.some((entry) => entry.key !== category.key && entry.label === label);
      if (conflict) return send(response, 400, { error: `分类「${label}」已存在。` });
      if (!category.builtin && safeFolderName(label) !== category.folder) {
        const oldPath = path.join(DATA, category.folder);
        const newFolder = safeFolderName(label);
        const newPath = path.join(DATA, newFolder);
        if (newFolder && !fs.existsSync(newPath)) {
          try { fs.renameSync(oldPath, newPath); } catch { /* 文件夹被占用就只改显示名 */ }
          if (fs.existsSync(newPath)) {
            const previous = category.folder;
            category.folder = newFolder;
            for (const item of catalog.items) {
              if (item.category === category.key && item.sourcePath?.startsWith(`${previous}/`)) {
                item.sourcePath = `${newFolder}/${item.sourcePath.slice(previous.length + 1)}`;
              }
            }
          }
        }
      }
      category.label = label;
      if (typeof input.symbol === 'string' && input.symbol.trim()) category.symbol = input.symbol.trim().slice(0, 2);
      if (typeof input.note === 'string') category.note = input.note.slice(0, 40);
      if (KINDS.includes(input.kind) && !category.builtin) category.kind = input.kind;
      if (Number.isFinite(input.order)) category.order = input.order;
      writeCatalog(catalog);
      return send(response, 200, { category, categories: catalog.categories });
    }

    if (request.method === 'POST' && url.pathname === '/api/categories/delete') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const category = categoryOf(catalog, String(input.key || ''));
      if (!category) return send(response, 404, { error: '找不到该分类。' });
      if (category.builtin) return send(response, 400, { error: '内置分类不能删除。' });
      const used = catalog.items.filter((item) => item.category === category.key);
      if (used.length && !input.force) return send(response, 400, { error: `这个分类里还有 ${used.length} 个条目，先移走或勾选一并删除。`, count: used.length });
      for (const item of used) {
        const filePath = itemFilePath(item.sourcePath);
        if (filePath && fs.existsSync(filePath)) { try { fs.unlinkSync(filePath); } catch { /* 忽略占用 */ } }
      }
      catalog.items = catalog.items.filter((item) => item.category !== category.key);
      catalog.categories = catalog.categories.filter((entry) => entry.key !== category.key);
      if (input.removeFolder) {
        try { fs.rmSync(path.join(DATA, category.folder), { recursive: true, force: true }); } catch { /* 忽略占用 */ }
      }
      writeCatalog(catalog);
      return send(response, 200, { ok: true, categories: catalog.categories });
    }

    /* ---------------- 应用（快捷方式） ---------------- */

    if (request.method === 'GET' && url.pathname === '/api/shortcuts') {
      return send(response, 200, { shortcuts: collectShortcuts() });
    }

    if (request.method === 'GET' && url.pathname === '/api/shortcuts/icon') {
      const target = String(url.searchParams.get('path') || '');
      if (!target || !fs.existsSync(target)) return send(response, 404, { error: '文件不存在。' });
      const extension = path.extname(target).toLowerCase();
      if (!['.lnk', '.exe', '.ico', '.dll', '.png', '.jpg', '.jpeg'].includes(extension)) {
        return send(response, 400, { error: '不支持的文件类型。' });
      }
      if (extension === '.lnk') {
        const info = readShortcut(target);
        return send(response, 200, { icon: info.iconDataUrl || '', detail: publicShortcutDetail(info) });
      }
      return send(response, 200, { icon: readIconDataUrl({ iconLocation: '', iconIndex: 0, target, maxSize: 128 }) });
    }

    /* ---------------- 条目的增删改 ---------------- */

    if (request.method === 'POST' && url.pathname === '/api/items') {
      const input = await readBody(request);
      const catalog = readCatalog();
      let category = categoryOf(catalog, String(input.category || ''));
      // 允许在等待补充的流程里顺手新建分类。
      if (!category && input.newCategory?.label) {
        const created = createCategory(catalog, input.newCategory);
        if (created.error) return send(response, 400, { error: created.error });
        category = created.category;
      }
      if (!category || category.inbox) return send(response, 400, { error: '请选择一个有效的分类。' });
      const title = String(input.title || '').trim();
      if (!title) return send(response, 400, { error: '名称不能为空。' });

      const now = new Date().toISOString();
      const item = {
        id: crypto.randomUUID(),
        category: category.key,
        title,
        description: String(input.description || '').trim(),
        tags: normalizeTags(input.tags),
        icon: normalizeIcon(input.icon),
        status: 'complete',
        createdAt: now,
        updatedAt: now,
        openCount: 0,
        lastOpenedAt: ''
      };

      if (category.kind === KIND_SCRIPT) {
        const extension = String(input.extension || 'txt').replace(/[^a-z0-9]/gi, '').slice(0, 10) || 'txt';
        const fileName = `${safeFilename(title)}.${extension}`;
        item.sourcePath = `${category.folder}/${fileName}`;
        if (fs.existsSync(path.join(DATA, item.sourcePath)) && !input.overwrite) {
          return send(response, 409, { error: `「${fileName}」已经存在，换个名字或勾选覆盖。` });
        }
        fs.writeFileSync(path.join(DATA, item.sourcePath), String(input.content || ''), 'utf8');
      } else if (category.kind === KIND_LINK) {
        const normalized = normalizeUrl(input.target || input.url);
        if (!normalized) return send(response, 400, { error: '请输入有效的 http 或 https 网址。' });
        item.target = normalized;
      } else if (category.kind === KIND_APP) {
        const sourcePath = String(input.path || input.target || '').trim();
        if (!sourcePath || !fs.existsSync(sourcePath)) return send(response, 400, { error: '找不到该程序或快捷方式文件。' });
        item._folder = category.folder;
        applyShortcutMetadata(item, sourcePath, { copy: true });
        delete item._folder;
        if (!item.target) item.target = sourcePath;
        if (!item.title) item.title = path.parse(sourcePath).name;
      } else {
        const targetPath = String(input.target || '').trim();
        if (targetPath) {
          item.target = targetPath;
          if (!fs.existsSync(targetPath) && !/^[a-zA-Z]:[\\/]/.test(targetPath)) {
            return send(response, 400, { error: '请填写存在的文件或文件夹路径。' });
          }
        } else {
          return send(response, 400, { error: '请填写文件或文件夹路径。' });
        }
      }

      catalog.items.push(item);
      writeCatalog(catalog);
      return send(response, 201, { item, categories: catalog.categories });
    }

    if (request.method === 'POST' && url.pathname === '/api/items/metadata') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = findItem(catalog, String(input.id || ''));
      const title = String(input.title || '').trim();
      if (!item || !title) return send(response, 400, { error: '找不到待处理条目，或名称为空。' });

      let category = categoryOf(catalog, String(input.category || ''));
      if (!category && input.newCategory?.label) {
        const created = createCategory(catalog, input.newCategory);
        if (created.error) return send(response, 400, { error: created.error });
        category = created.category;
      }
      if (!category || category.inbox) return send(response, 400, { error: '请选择一个分类。' });

      if (item.category === catalog.categories.find((entry) => entry.inbox)?.key && item.sourcePath) {
        const oldFile = itemFilePath(item.sourcePath);
        const extension = path.extname(item.sourcePath).toLowerCase();
        const isUrlShortcut = extension === '.url';
        if (category.kind === KIND_LINK && !isUrlShortcut) {
          return send(response, 400, { error: '「网址」分类只收 .url 快捷方式；其它文件建议选「文档」或「应用」。' });
        }
        if (category.kind === KIND_SCRIPT && !isUrlShortcut) {
          item.sourcePath = `${category.folder}/${path.basename(item.sourcePath)}`;
          const targetFile = path.join(DATA, item.sourcePath);
          fs.mkdirSync(path.dirname(targetFile), { recursive: true });
          if (oldFile && fs.existsSync(oldFile) && !fs.existsSync(targetFile)) fs.renameSync(oldFile, targetFile);
        } else if (category.kind === KIND_APP) {
          item.sourcePath = `${category.folder}/${path.basename(item.sourcePath)}`;
          const targetFile = path.join(DATA, item.sourcePath);
          fs.mkdirSync(path.dirname(targetFile), { recursive: true });
          if (oldFile && fs.existsSync(oldFile) && !fs.existsSync(targetFile)) fs.renameSync(oldFile, targetFile);
          applyShortcutMetadata(item, fs.existsSync(targetFile) ? targetFile : oldFile, { copy: false });
        } else if (category.kind === KIND_FILE || category.kind === KIND_LINK) {
          item.sourcePath = `${category.folder}/${path.basename(item.sourcePath)}`;
          const targetFile = path.join(DATA, item.sourcePath);
          fs.mkdirSync(path.dirname(targetFile), { recursive: true });
          if (oldFile && fs.existsSync(oldFile) && !fs.existsSync(targetFile)) fs.renameSync(oldFile, targetFile);
          if (category.kind === KIND_LINK) item.target = item.target || readUrlShortcut(targetFile);
          if (category.kind === KIND_FILE) item.target = targetFile;
        }
      }

      item.category = category.key;
      item.title = title;
      item.description = String(input.description || '').trim();
      item.tags = normalizeTags(input.tags);
      const icon = normalizeIcon(input.icon);
      if (icon) item.icon = icon;
      const target = String(input.target || '').trim();
      if (target) item.target = target;
      item.status = 'complete';
      item.updatedAt = new Date().toISOString();
      writeCatalog(catalog);
      return send(response, 200, { item, categories: catalog.categories });
    }

    if (request.method === 'POST' && url.pathname === '/api/items/update') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = findItem(catalog, String(input.id || ''));
      const title = String(input.title || '').trim();
      if (!item || !title) return send(response, 400, { error: '找不到条目，或名称为空。' });
      item.title = title;
      item.description = String(input.description || '').trim();
      item.tags = normalizeTags(input.tags);
      const icon = normalizeIcon(input.icon);
      if (icon) item.icon = icon;
      if (input.clearIcon) item.icon = '';
      const target = String(input.target || '').trim();
      if (target) {
        const category = categoryOf(catalog, item.category);
        if (category?.kind === KIND_LINK) {
          const normalized = normalizeUrl(target);
          if (!normalized) return send(response, 400, { error: '请输入有效的 http 或 https 网址。' });
          item.target = normalized;
        } else if (category?.kind !== KIND_SCRIPT) {
          item.target = target;
        }
      }
      item.updatedAt = new Date().toISOString();
      writeCatalog(catalog);
      return send(response, 200, { item });
    }

    if (request.method === 'POST' && url.pathname === '/api/items/delete') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const index = catalog.items.findIndex((entry) => entry.id === input.id);
      if (index < 0) return send(response, 404, { error: '找不到条目。' });
      const [item] = catalog.items.splice(index, 1);
      const filePath = itemFilePath(item.sourcePath);
      if (filePath && fs.existsSync(filePath)) fs.unlinkSync(filePath);
      writeCatalog(catalog);
      return send(response, 200, { ok: true });
    }

    if (request.method === 'POST' && url.pathname === '/api/items/reorder') {
      const input = await readBody(request);
      if (!Array.isArray(input.ids)) return send(response, 400, { error: '排序数据无效。' });
      const catalog = readCatalog();
      const position = new Map(input.ids.map((id, index) => [id, index]));
      catalog.items.forEach((item) => { if (position.has(item.id)) item.order = position.get(item.id); });
      writeCatalog(catalog);
      return send(response, 200, { items: catalog.items });
    }

    if (request.method === 'POST' && url.pathname === '/api/items/hide') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = findItem(catalog, String(input.id || ''));
      if (!item || item.status !== 'needs_metadata') return send(response, 404, { error: '找不到待补充条目。' });
      item.status = 'hidden';
      item.updatedAt = new Date().toISOString();
      writeCatalog(catalog);
      return send(response, 200, { item });
    }

    if (request.method === 'POST' && url.pathname === '/api/items/open') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = findItem(catalog, String(input.id || ''));
      if (!item || item.status !== 'complete') return send(response, 404, { error: '找不到可打开的条目。' });
      const category = categoryOf(catalog, item.category);
      const kind = category?.kind || KIND_SCRIPT;

      let target;
      if (kind === KIND_SCRIPT) {
        target = itemFilePath(item.sourcePath);
        if (!target || !fs.existsSync(target)) return send(response, 400, { error: '找不到脚本文件。请先扫描并同步。' });
      } else if (kind === KIND_LINK) {
        if (!/^https?:\/\//i.test(item.target || '')) return send(response, 400, { error: '这个网址无效。' });
        target = item.target;
      } else if (kind === KIND_APP) {
        // 优先用复制进工作台的 .lnk：启动参数、工作目录、管理员标记都原样保留。
        const shortcutPath = itemFilePath(item.sourcePath);
        target = shortcutPath && fs.existsSync(shortcutPath) ? shortcutPath : (itemAbsolutePath(item) || '');
        if (!target || !fs.existsSync(target)) return send(response, 400, { error: '找不到这个应用的目标文件，可能已被移动或卸载。' });
      } else {
        target = itemAbsolutePath(item) || '';
        if (!target || !fs.existsSync(target)) return send(response, 400, { error: '找不到这个文件，可能已被移动。' });
      }

      startDetached(target);
      item.openCount = Number(item.openCount || 0) + 1;
      item.lastOpenedAt = new Date().toISOString();
      item.updatedAt = item.lastOpenedAt;
      writeCatalog(catalog);
      return send(response, 200, { ok: true });
    }

    return send(response, 404, { error: '接口不存在。' });
  } catch (error) {
    return send(response, 500, { error: error.message || '本地服务发生错误。' });
  }
});

function normalizeUrl(value) {
  const text = String(value || '').trim();
  if (!text) return '';
  const withProtocol = /^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `https://${text}`;
  try {
    const parsed = new URL(withProtocol);
    if (!['http:', 'https:'].includes(parsed.protocol)) return '';
    return parsed.href;
  } catch {
    return '';
  }
}

function publicShortcutDetail(info) {
  return {
    target: info.resolvedTarget || '',
    targetExists: Boolean(info.targetExists),
    arguments: info.arguments || '',
    workingDirectory: info.workingDirectory || '',
    iconLocation: info.iconLocation || ''
  };
}

ensureDirectories(readCatalog().categories);

server.listen(PORT, '127.0.0.1', () => {
  console.log(`个人工作台本地服务已启动：http://127.0.0.1:${PORT}`);
  console.log(`数据目录：${DATA}`);
});
