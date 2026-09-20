#!/usr/bin/env node
/**
 * ============================================================================
 * 模块 M5：接口文档结构校验
 * 接口文档：docs/interfaces/build-and-verify.md      总纲：docs/INTERFACES.md
 *
 * 目的：把「每个模块必须有接口文档，且文档结构统一」从口头约定变成可执行约束。
 * 零依赖、纯 Node，不读写 data/，可在任意平台运行（CI 是 Ubuntu）。
 *
 * 校验内容：
 *   1. 接口总纲存在，且含模块地图与铁律
 *   2. 每个模块都有接口文档，且文档含四个固定二级标题
 *   3. 关键源码里保留了模块边界声明头（防止被误删）
 *   4. agent 薄壳文件存在，且都指回 AGENTS.md
 *   5. 总纲里引用的 docs/interfaces/*.md 链接都指向真实文件
 *
 * 用法：node scripts/check-structure.js
 * ============================================================================
 */

'use strict';

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const problems = [];
const lines = [];

/** 每份接口文档必须包含的二级标题（顺序不强制）。 */
const REQUIRED_SECTIONS = ['## 职责与边界', '## 对外接口', '## 禁止事项', '## 变更记录'];

/** 模块 → 接口文档 → 关键源码（marker 是源码里必须保留的边界声明关键字）。 */
const MODULES = [
  { id: 'M0', name: '数据契约', doc: 'docs/interfaces/data-catalog.md', source: null, marker: null },
  { id: 'M1', name: '浏览器后端', doc: 'docs/interfaces/http-api.md', source: 'server/server.js', marker: '模块 M1' },
  { id: 'M2', name: '快捷方式解析', doc: 'docs/interfaces/shortcut-lib.md', source: 'server/shortcut.js', marker: '模块 M2' },
  { id: 'M3', name: '桌面客户端', doc: 'docs/interfaces/tauri-ipc.md', source: 'desktop/src-tauri/src/main.rs', marker: '模块 M3' },
  { id: 'M4', name: '前端界面', doc: 'docs/interfaces/app-ui.md', source: 'app/index.html', marker: '模块 M4' },
  { id: 'M5', name: '构建与验证', doc: 'docs/interfaces/build-and-verify.md', source: 'package.json', marker: null }
];

/** agent 工具入口文件，应全部指回 AGENTS.md。 */
const AGENT_SHELLS = [
  'AGENTS.md',
  'CLAUDE.md',
  '.github/copilot-instructions.md',
  '.cursor/rules/module-boundaries.mdc',
  '.clinerules',
  '.windsurfrules'
];

function read(relative) {
  try {
    return fs.readFileSync(path.join(ROOT, relative), 'utf8');
  } catch {
    return null;
  }
}

function fail(message) {
  problems.push(message);
}

/* ---------- 1. 接口总纲 ---------- */
const overview = read('docs/INTERFACES.md');
if (!overview) {
  fail('缺少接口总纲 docs/INTERFACES.md');
} else {
  lines.push('✓ 接口总纲 docs/INTERFACES.md');
  // 允许标题带序号（如「## 2. 模块地图」），只要求关键词出现在某个二级标题里。
  for (const key of ['模块地图', '铁律', '任务路由表', '已知契约偏差']) {
    if (!new RegExp(`^##[^\\n]*${key}`, 'm').test(overview)) {
      fail(`docs/INTERFACES.md 缺少二级标题「${key}」`);
    }
  }
  for (const module of MODULES) {
    if (!overview.includes(`\`${module.id}\``)) fail(`docs/INTERFACES.md 的模块地图未列出 ${module.id}（${module.name}）`);
  }
}

/* ---------- 2. 每个模块的接口文档 ---------- */
for (const module of MODULES) {
  const text = read(module.doc);
  if (!text) {
    fail(`${module.id}（${module.name}）缺少接口文档：${module.doc}`);
    continue;
  }
  const missing = REQUIRED_SECTIONS.filter((section) => !text.includes(section));
  if (missing.length) {
    fail(`${module.doc} 缺少必需章节：${missing.join('、')}`);
  } else {
    lines.push(`✓ ${module.id} ${module.doc}（章节 4/4）`);
  }
}

/* ---------- 3. 源码里的模块边界声明头 ---------- */
for (const module of MODULES) {
  if (!module.source || !module.marker) continue;
  const text = read(module.source);
  if (text === null) {
    fail(`${module.id} 找不到源码文件：${module.source}`);
    continue;
  }
  if (!text.includes(module.marker)) {
    fail(`${module.source} 缺少模块边界声明头（应含「${module.marker}」），请勿删除文件头注释`);
  } else {
    lines.push(`✓ ${module.source} 有边界声明头`);
  }
}

/* ---------- 4. agent 薄壳文件 ---------- */
for (const shell of AGENT_SHELLS) {
  const text = read(shell);
  if (text === null) {
    fail(`缺少 agent 入口文件 ${shell}`);
    continue;
  }
  if (!text.includes('AGENTS.md')) {
    fail(`${shell} 未指向 AGENTS.md（薄壳文件应只做指针，避免规则多处漂移）`);
  }
}

/* ---------- 5. 总纲里的接口文档链接 ---------- */
if (overview) {
  const links = new Set();
  for (const match of overview.matchAll(/\]\(((?:docs\/)?interfaces\/[^)#\s]+\.md)/g)) {
    links.add(match[1]);
  }
  for (const link of links) {
    const target = link.startsWith('docs/') ? link : path.posix.join('docs', link);
    if (!fs.existsSync(path.join(ROOT, target))) fail(`docs/INTERFACES.md 引用了不存在的文件：${link}`);
  }
  lines.push(`✓ 总纲引用的接口文档链接全部有效（${links.size} 条）`);
}

/* ---------- 6. Markdown 链接：文件存在性 + 锚点有效性 ---------- */
const SKIP_DIRS = new Set(['node_modules', '.git', 'logs', 'data', 'target', '.workbuddy']);

/** 按 GitHub 的 slug 规则近似归一化标题（保留中英文字母数字与连字符，去掉标点与 emoji）。 */
function slugify(text) {
  return String(text)
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1') // 内联链接只取文字
    .replace(/[`*_~]/g, '')
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .trim()
    .replace(/\s+/g, '-');
}

/** 归一化链接里的锚点，便于与标题 slug 比较。 */
function normalizeAnchor(anchor) {
  let value = anchor;
  try {
    value = decodeURIComponent(anchor);
  } catch {
    /* 保持原样 */
  }
  return value
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s-]/gu, '')
    .trim()
    .replace(/\s+/g, '-');
}

function markdownFiles() {
  const found = [];
  const walk = (dir, depth) => {
    if (depth > 4) return;
    let entries = [];
    try {
      entries = fs.readdirSync(path.join(ROOT, dir || '.'), { withFileTypes: true });
    } catch {
      return;
    }
    for (const entry of entries) {
      if (entry.isDirectory() && SKIP_DIRS.has(entry.name)) continue;
      if (entry.name.startsWith('.') && !['.github', '.cursor'].includes(entry.name)) continue;
      const relative = dir ? `${dir}/${entry.name}` : entry.name;
      if (entry.isDirectory()) walk(relative, depth + 1);
      else if (entry.name.endsWith('.md')) found.push(relative);
    }
  };
  walk('', 0);
  return found;
}

const mdCache = new Map();
function markdownEntry(file) {
  if (!mdCache.has(file)) {
    const raw = read(file);
    if (raw === null) {
      mdCache.set(file, null);
    } else {
      const text = raw.replace(/```[\s\S]*?```/g, ''); // 忽略代码块里的示例
      const slugs = new Set();
      for (const match of text.matchAll(/^#{1,6}\s+(.+)$/gm)) slugs.add(slugify(match[1]));
      mdCache.set(file, { text, slugs });
    }
  }
  return mdCache.get(file);
}

const mdFiles = markdownFiles();
const deadLinks = [];
const badAnchors = [];

for (const file of mdFiles) {
  const entry = markdownEntry(file);
  if (!entry) continue;
  for (const match of entry.text.matchAll(/\]\(([^)\s]+)\)/g)) {
    const raw = match[1];
    if (/^(https?:|mailto:)/i.test(raw)) continue;

    const [rawPath, rawAnchor] = raw.split('#');
    let targetFile = file;
    if (rawPath) {
      const resolved = path.posix.normalize(path.posix.join(path.posix.dirname(file), rawPath));
      if (!fs.existsSync(path.join(ROOT, resolved))) {
        deadLinks.push(`${file} → ${rawPath}`);
        continue;
      }
      if (!resolved.endsWith('.md')) continue; // 目录或其它类型文件，不做锚点检查
      targetFile = resolved;
    }
    if (!rawAnchor) continue;

    const target = markdownEntry(targetFile);
    if (!target) continue;
    const wanted = normalizeAnchor(rawAnchor).replace(/-+$/, '');
    if (!wanted) continue;
    const hit = [...target.slugs].some((slug) => slug.replace(/-+$/, '') === wanted);
    if (!hit) badAnchors.push(`${file} → ${rawPath ? `${rawPath}` : ''}#${rawAnchor}`);
  }
}

if (deadLinks.length) {
  for (const link of deadLinks) fail(`文档链接指向不存在的文件：${link}`);
} else {
  lines.push(`✓ Markdown 链接的文件目标均存在（扫描 ${mdFiles.length} 份文档）`);
}
if (badAnchors.length) {
  for (const anchor of badAnchors) fail(`文档链接的锚点找不到对应标题：${anchor}`);
} else {
  lines.push('✓ Markdown 链接的锚点均能找到对应标题');
}

/* ---------- 输出 ---------- */
console.log('接口文档结构检查');
for (const line of lines) console.log(`  ${line}`);

if (problems.length) {
  console.error('\n发现问题：');
  for (const problem of problems) console.error(`  ✗ ${problem}`);
  console.error(`\n共 ${problems.length} 项。规则见 docs/INTERFACES.md（第 3 节铁律、第 8 节文档骨架）。`);
  process.exit(1);
}

console.log('\n全部通过。');
