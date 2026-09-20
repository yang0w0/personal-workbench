# 接口文档 · `M2` 快捷方式与图标解析（shortcut.js）

## 职责与边界

`server/shortcut.js` 是一个**纯函数库**：读取 Windows `.lnk` 二进制、解析出目标路径与启动参数、从 `.exe`/`.dll`/`.ico` 里提取图标并转成 data URL。

它被 `M1` 调用（`require('./shortcut.js')`），**自己不发起任何网络请求、不写任何文件、不依赖任何 npm 包**。

**允许读写**

| 路径 | 权限 |
| --- | --- |
| `server/shortcut.js` | ✅ 本模块唯一文件，改 .lnk 解析与图标提取只改这里 |
| 传入的 `.lnk` / `.exe` / `.dll` / `.ico` / 图片路径 | ✅ **只读**（`fs.readFileSync`） |
| `data/` 与 `catalog.json` | ❌ 完全不碰。本模块不知道「条目」「分类」的存在 |
| `server/server.js` | ❌ 属于 `M1`。改动调用方式要回到 [http-api.md](http-api.md) |

**设计原则**：本模块对上层只承诺「给我路径，我尽力解析；解析不出来就返回空值」，**绝不抛异常给调用方**（内部全部 `try/catch` 兜底），因为一个坏掉的快捷方式不应该让整个扫描失败。

---

## 对外接口

模块导出 6 个函数（`module.exports`）：

| 函数 | 签名 | 用途 |
| --- | --- | --- |
| `readShortcut` | `(lnkPath, { iconMaxSize = 128 }) => ShortcutInfo` | **主要入口**。解析 `.lnk` 并挑选最可能存在的目标 |
| `readIconDataUrl` | `({ iconLocation, iconIndex, target, maxSize = 128 }) => string` | **主要入口**。提取图标，返回 data URL |
| `parseLnkBuffer` | `(buf) => RawLnkInfo` | 解析 `.lnk` 字节流。一般由 `readShortcut` 内部调用 |
| `icoFromPe` | `(buf, wantedIndex, maxSize = 128) => Buffer \| null` | 从 PE（`.exe`/`.dll`）资源里抽出一个 `.ico` 字节流 |
| `expandEnvironment` | `(value) => string` | 展开 `%SystemRoot%` 这类环境变量 |
| `splitIconLocation` | `(iconLocation, fallbackIndex) => { file, index }` | 解析 `"C:\x\y.exe,3"` / `%SystemRoot%\system32\shell32.dll,-16` 这类图标位置写法 |

### `readShortcut(lnkPath, { iconMaxSize = 128 }) => ShortcutInfo`

第二个参数可选，只影响 `iconDataUrl` 的尺寸：

| 选项 | 类型 | 默认 | 说明 |
| --- | --- | --- | --- |
| `iconMaxSize` | number | `128` | 提取图标时挑最接近这个边长的尺寸。`M1` 在 `/api/shortcuts/icon` 用默认 128（给选择器大图预览），在把 `.lnk` 复制进 `data/应用/` 时传 **64**（存进 `icon` 字段的兜底图，不需要太大） |

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `valid` | boolean | `.lnk` 头是否合法 |
| `resolvedTarget` | string | **最可能存在的目标路径**。优先取真实存在的候选，其次取猜测值，最后空串 |
| `targetExists` | boolean | `resolvedTarget` 是否真的存在于磁盘 |
| `resolvedCandidates` | string[] | 全部候选路径，存在的排在前面 |
| `iconDataUrl` | string | 图标 data URL，提取失败为空串 |
| `fallbackName` | string | 兜底显示名：`.lnk` 内的名字，否则文件名 |
| `name` | string | `.lnk` 的 NAME_STRING。⚠️ 经常是**描述**而不是名字，`M1` 只在其长度 ≤ 40 且不含 `http`/`.com`/`参见` 时才采用 |
| `arguments` | string | 启动参数 |
| `workingDirectory` | string | 工作目录 |
| `iconLocation` | string | 图标位置原文 |
| `iconIndex` | number | 图标索引 |
| `description` | string | 描述 |
| `isFolder` | boolean | 目标是否为文件夹 |
| `runAsAdmin` | boolean | 是否要求管理员运行 |
| `showCommand` | number | 1 普通 / 3 最大化 / 7 最小化 |
| `relativePath` | string | `.lnk` 里记录的相对路径 |
| `candidates` | string[] | 解析阶段的原始候选 |
| `target` | string | 解析阶段的原始目标（**不是**最终解析结果，用 `resolvedTarget`） |

**兜底行为**：文件读不到或解析抛错时，返回默认骨架 + `resolvedTarget: ''`、`iconDataUrl: ''`、`fallbackName: <文件名>`，**不抛异常**。

**路径解析顺序**（`resolveRelativeToLnk`）：LinkInfo 绝对路径 → 相对路径 → 环境变量块 → 文件夹/未解析目标的兜底。

### `readIconDataUrl({ iconLocation, iconIndex, target, maxSize }) => string`

提取顺序（任一成功即返回）：

1. `iconLocation` 指向的文件（`.ico` 需通过 `looksLikeIco()` 校验，PE 则走 `icoFromPe`）
2. `target` 本身（`.exe`/`.dll`/`.scr`/`.cpl`/`.msc`/`.ocx`，或首两字节为 `MZ`）
3. `target` 同目录同名的 `.ico`

返回值为 `data:image/*;base64,...` 或**空串**。空串是正常结果，调用方应回落到占位符号——「前端的占位符号比一张认错的系统图标更诚实」。

`maxSize` 默认 128，用于挑选 PE 里最接近该尺寸的图标（`M1` 在 `/api/shortcuts/icon` 里显式传 128）。

### 支持的图标来源格式

| 扩展名 | 处理方式 |
| --- | --- |
| `.ico` | 直接校验后转 data URL |
| `.png` `.jpg` `.jpeg` `.gif` `.bmp` `.webp` | 直接转 data URL，MIME 由 `mimeFor()` 决定 |
| `.exe` `.dll` `.scr` `.cpl` `.msc` `.ocx` | 走 `icoFromPe()` 抽取指定索引的图标资源 |

### 命令行自测

```bash
node server/shortcut.js "C:\path\to\app.lnk"
```

`require.main === module` 时进入自测模式，打印解析结果。**改本模块后应至少跑一次这个自测**，因为它是纯二进制解析，容易在边界上出错。

---

## 禁止事项

- ❌ **不要引入 npm 依赖**（例如 `lnk`、`icojs`）。这个文件的价值就是零依赖手写解析，并且已经能处理国产软件的奇形怪状 `.lnk`。
- ❌ **不要写文件**。本模块是纯读的，任何生成/落盘都应该由 `M1` 负责。
- ❌ **不要抛异常给调用方**。所有对外函数必须 `try/catch` 兜底后返回空值。
- ❌ **不要在这里读取或判断 `data/catalog.json`**，本模块不该知道条目结构。
- ❌ **不要修改 `M1` 的调用方式后忘了更新 [http-api.md](http-api.md)**：`M1` 的 `/api/shortcuts/icon` 返回结构里的 `detail` 字段就来自本模块，字段变了会影响接口。
- ❌ **不要在没有 `.lnk` 样本的情况下凭猜修改二进制偏移**。改了解析逻辑，请用一个真实 `.lnk` 跑自测。

越界时应怎么办：如果需要新的解析能力（例如解析 `.url` 的 `IconFile=` 字段），先在这里加函数并更新本文档的导出表，再由 `M1` 调用；不要把这套逻辑塞进 `server.js`。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-20 | 首次编写。冻结 6 个导出函数、`readShortcut` 全字段、`readIconDataUrl` 三级回落策略、命令行自测入口。 |
| 2026-09-20 | `readShortcut` 增加可选第二参数 `{ iconMaxSize }`（默认 128），供「复制 `.lnk` 进 `data/应用/`」时取 64px 图标。⚠️ 实现上曾漏掉形参声明导致 `ReferenceError: iconMaxSize is not defined`——**改这个函数时务必同时改签名与文档**。 |
