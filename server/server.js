'use strict';

const http = require('node:http');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { spawn } = require('node:child_process');

const ROOT = path.resolve(__dirname, '..');
const DATA = path.join(ROOT, 'data');
const PORT = 8765;
const CATALOG_PATH = path.join(DATA, 'catalog.json');
const DIRECTORIES = ['脚本', '网址', '待整理', '密码库', '备份'];

for (const directory of DIRECTORIES) fs.mkdirSync(path.join(DATA, directory), { recursive: true });

function readCatalog() {
  try {
    const parsed = JSON.parse(fs.readFileSync(CATALOG_PATH, 'utf8'));
    return Array.isArray(parsed.items) ? parsed : { version: 1, items: [] };
  } catch {
    return { version: 1, items: [] };
  }
}

function writeCatalog(catalog) {
  const temporary = `${CATALOG_PATH}.tmp`;
  fs.writeFileSync(temporary, `${JSON.stringify(catalog, null, 2)}\n`, 'utf8');
  fs.renameSync(temporary, CATALOG_PATH);
}

function safeFilename(value) {
  const cleaned = String(value || 'untitled').trim().replace(/[<>:"/\\|?*\x00-\x1F]/g, '-').replace(/\.+$/g, '');
  return (cleaned || 'untitled').slice(0, 80);
}

function parseUrlShortcut(filePath) {
  const match = fs.readFileSync(filePath, 'utf8').match(/^URL=(.+)$/mi);
  return match ? match[1].trim() : '';
}

function itemFilePath(sourcePath) {
  if (typeof sourcePath !== 'string' || sourcePath.split(/[\\/]/).length !== 2) return null;
  const resolved = path.resolve(DATA, sourcePath);
  return resolved.startsWith(`${DATA}${path.sep}`) ? resolved : null;
}

function scan(catalog) {
  const known = new Set(catalog.items.filter((item) => item.sourcePath).map((item) => item.sourcePath));
  const discovered = [];
  const removed = [];
  // Keep the catalog in sync when a managed file is deleted outside the workbench.
  catalog.items = catalog.items.filter((item) => {
    const filePath = itemFilePath(item.sourcePath);
    if (item.sourcePath && !filePath) return true;
    if (filePath && !fs.existsSync(filePath)) { removed.push(item); return false; }
    return true;
  });
  const scanDirectory = (name, category) => {
    for (const entry of fs.readdirSync(path.join(DATA, name), { withFileTypes: true })) {
      if (!entry.isFile() || entry.name === '.gitkeep') continue;
      const sourcePath = `${name}/${entry.name}`;
      if (known.has(sourcePath)) continue;
      const item = {
        id: crypto.randomUUID(),
        category,
        title: path.parse(entry.name).name,
        description: '',
        tags: [],
        sourcePath,
        status: 'needs_metadata',
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        openCount: 0,
        lastOpenedAt: ''
      };
      if (category === 'link' && entry.name.toLowerCase().endsWith('.url')) item.url = parseUrlShortcut(path.join(DATA, sourcePath));
      catalog.items.push(item);
      discovered.push(item);
    }
  };
  scanDirectory('脚本', 'script');
  scanDirectory('网址', 'link');
  scanDirectory('待整理', 'inbox');
  writeCatalog(catalog);
  return { discovered, removed };
}

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
      if (body.length > 1024 * 1024) reject(new Error('请求过大'));
    });
    request.on('end', () => {
      try { resolve(body ? JSON.parse(body) : {}); } catch { reject(new Error('JSON 格式无效')); }
    });
  });
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
      return send(response, 200, { items: catalog.items });
    }
    if (request.method === 'POST' && url.pathname === '/api/scan') {
      const catalog = readCatalog();
      const result = scan(catalog);
      return send(response, 200, { ...result, items: catalog.items });
    }
    if (request.method === 'POST' && url.pathname === '/api/items') {
      const input = await readBody(request);
      const category = input.category;
      const title = String(input.title || '').trim();
      if (!['script', 'link'].includes(category) || !title) return send(response, 400, { error: '类别和名称不能为空。' });
      const catalog = readCatalog();
      const now = new Date().toISOString();
      const item = { id: crypto.randomUUID(), category, title, description: String(input.description || '').trim(), tags: Array.isArray(input.tags) ? input.tags.map(String).filter(Boolean).slice(0, 20) : [], icon: typeof input.icon === 'string' ? input.icon.slice(0, 2 * 1024 * 1024) : '', status: 'complete', createdAt: now, updatedAt: now, openCount: 0, lastOpenedAt: '' };
      if (category === 'script') {
        const extension = String(input.extension || 'txt').replace(/[^a-z0-9]/gi, '').slice(0, 10) || 'txt';
        const fileName = `${safeFilename(title)}.${extension}`;
        item.sourcePath = `脚本/${fileName}`;
        fs.writeFileSync(path.join(DATA, item.sourcePath), String(input.content || ''), 'utf8');
      } else {
        try {
          const parsed = new URL(String(input.url || ''));
          if (!['https:', 'http:'].includes(parsed.protocol)) throw new Error();
          item.url = parsed.href;
        } catch { return send(response, 400, { error: '请输入有效的 http 或 https 网址。' }); }
      }
      catalog.items.push(item);
      writeCatalog(catalog);
      return send(response, 201, { item });
    }
    if (request.method === 'POST' && url.pathname === '/api/items/metadata') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = catalog.items.find((entry) => entry.id === input.id);
      const title = String(input.title || '').trim();
      if (!item || !title) return send(response, 400, { error: '找不到待处理条目，或名称为空。' });
      const category = input.category === 'link' ? 'link' : 'script';
      if (item.category === 'inbox' && item.sourcePath) {
        const oldPath = path.join(DATA, item.sourcePath);
        const isShortcut = item.sourcePath.toLowerCase().endsWith('.url');
        if (category === 'link' && !isShortcut) return send(response, 400, { error: '网址分类仅支持 .url 快捷方式文件。' });
        const targetFolder = category === 'link' ? '网址' : '脚本';
        const fileName = path.basename(item.sourcePath);
        const targetPath = path.join(DATA, targetFolder, fileName);
        if (fs.existsSync(oldPath) && !fs.existsSync(targetPath)) fs.renameSync(oldPath, targetPath);
        item.sourcePath = `${targetFolder}/${fileName}`;
      }
      item.category = category;
      item.title = title;
      item.description = String(input.description || '').trim();
      item.tags = Array.isArray(input.tags) ? input.tags.map(String).filter(Boolean).slice(0, 20) : [];
      item.status = 'complete';
      item.updatedAt = new Date().toISOString();
      writeCatalog(catalog);
      return send(response, 200, { item });
    }
    if (request.method === 'POST' && url.pathname === '/api/items/update') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = catalog.items.find((entry) => entry.id === input.id);
      const title = String(input.title || '').trim();
      if (!item || !title) return send(response, 400, { error: '找不到条目，或名称为空。' });
      item.title = title;
      item.description = String(input.description || '').trim();
      item.tags = Array.isArray(input.tags) ? input.tags.map(String).filter(Boolean).slice(0, 20) : [];
      if (typeof input.icon === 'string') item.icon = input.icon.slice(0, 2 * 1024 * 1024);
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
      const item = catalog.items.find((entry) => entry.id === input.id);
      if (!item || item.status !== 'needs_metadata') return send(response, 404, { error: '找不到待补充条目。' });
      item.status = 'hidden';
      item.updatedAt = new Date().toISOString();
      writeCatalog(catalog);
      return send(response, 200, { item });
    }
    if (request.method === 'POST' && url.pathname === '/api/items/open') {
      const input = await readBody(request);
      const catalog = readCatalog();
      const item = catalog.items.find((entry) => entry.id === input.id);
      if (!item || item.status !== 'complete') return send(response, 404, { error: '找不到可打开的条目。' });
      let target;
      if (item.category === 'script') {
        target = itemFilePath(item.sourcePath);
        if (!target || !fs.existsSync(target)) return send(response, 400, { error: '找不到脚本文件。请先扫描并同步。' });
      } else if (item.category === 'link' && /^https?:\/\//i.test(item.url || '')) target = item.url;
      else return send(response, 400, { error: '该条目无法打开。' });
      const child = spawn('cmd.exe', ['/c', 'start', '', target], { detached: true, stdio: 'ignore', windowsHide: true });
      child.unref();
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

server.listen(PORT, '127.0.0.1', () => {
  console.log(`个人工作台本地服务已启动：http://127.0.0.1:${PORT}`);
  console.log(`数据目录：${DATA}`);
});
