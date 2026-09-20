# 接口文档 · 总纲

> **这份文件是项目中唯一的跨模块通信依据。**
> 只做某一个功能时，你只读、只改那个功能的模块；需要用别的模块的能力时，读这里和对应的接口文档，**不要读别人的实现代码**。

本文件面向人和 AI agent。任何 agent（WorkBuddy / Codex / Claude Code / Copilot / Cursor 等）在本仓库动手前都应先读 [AGENTS.md](../AGENTS.md) 和本文件。

---

## 1. 为什么要这样做

这个项目有三端（浏览器版后端、桌面客户端、前端界面）共用同一份数据。三端各自实现了一遍「读写条目」的逻辑，字段名、支持的分类、时间格式都已经开始出现差异。如果每次改功能都从头把整个仓库读一遍：

- 上下文会被无关代码淹没，容易改错层；
- 会「顺手」把别的模块也改了，引发布局/契约漂移；
- 同一个逻辑被重复实现三遍，谁都不知道哪份才是准的。

所以本项目采用**模块 + 接口文档**的组织方式：模块之间只通过**文档化的接口**通信，接口文档是契约的唯一真相。

---

## 2. 模块地图

| 模块 ID | 模块名 | 代码位置 | 接口文档 | 说明 |
| --- | --- | --- | --- | --- |
| `M0` | 数据契约 | `data/catalog.json`、`data/*` 分类目录 | [interfaces/data-catalog.md](interfaces/data-catalog.md) | 最上游契约。三端共用，改动影响面最大 |
| `M1` | 浏览器后端 | `server/server.js` | [interfaces/http-api.md](interfaces/http-api.md) | 本机 HTTP 服务，`127.0.0.1:8765` |
| `M2` | 快捷方式/图标解析 | `server/shortcut.js` | [interfaces/shortcut-lib.md](interfaces/shortcut-lib.md) | 被 `M1` 调用的纯函数库，不联网、不写盘 |
| `M3` | 桌面客户端 | `desktop/src-tauri/`（`src/main.rs`、`tauri.conf.json`） | [interfaces/tauri-ipc.md](interfaces/tauri-ipc.md) | Tauri 外壳 + IPC 命令 |
| `M4` | 前端界面 | `app/index.html` | [interfaces/app-ui.md](interfaces/app-ui.md) | 单文件界面；内部含 `M4-CSS` / `M4-MAIN` / `M4-SKIN` / `M4-DRAG` 四个子模块 |
| `M5` | 构建与验证 | `package.json`、`build-client.cmd`、`start-server.cmd`、`.github/workflows/` | [interfaces/build-and-verify.md](interfaces/build-and-verify.md) | 编译、校验、发布 |
| `M6` | 图标素材 | `assets/icons/` | 见 [interfaces/data-catalog.md](interfaces/data-catalog.md) 的「图标」一节 | 生成 128×128 PNG 的素材与流程 |

依赖方向（箭头 = 「调用/依赖」）：

```text
                M6 图标素材
                     │（制作后写入 M0 的 icon 字段）
                     ▼
   M4 前端界面 ──► M1 浏览器后端 ──► M2 快捷方式/图标解析
        │                │
        │                └──► M0 数据契约 ◄──┐
        │                                    │
        └──► M3 桌面客户端 ─────────────────┘

   M5 构建与验证：观察以上全部，但不属于任何业务模块
```

**关键点：`M4 → M1` 与 `M4 → M3` 是两条并行通道，能力并不等价**，差异清单见 [第 6 节](#6-已知契约偏差必读)。

---

## 3. 铁律

**R1 · 只在自己的模块里动手。**
每次开工先确定「本次任务属于哪个模块」，然后只读、只改该模块的文件。改完不要顺手格式化、重构其他模块。

**R2 · 需要别的模块的能力时，读接口文档，不读它的代码。**
跨模块调用的正确姿势是：查本文件的[任务路由表](#4-任务路由表) → 打开对方的接口文档 → 按契约调用。**禁止**为了搞清楚「它怎么实现的」而去读对方源码。

**R3 · 改接口 = 改文档，且在同一次提交里完成。**
新增/修改/删除任何接口、字段、状态值、文件路径，必须在同一次提交里更新对应接口文档。文档与实现不一致时，先怀疑文档过期，但**先核对接口文档再决定**，不要靠读源码去猜。

**R4 · 接口文档里不写真实数据。**
接口文档只写结构和规则，不写真实网址、脚本内容、个人路径、密码。示例一律用 `https://example.com`、`示例脚本.ps1`。

**R5 · 同一时刻只用一个界面写数据。**
浏览器版和桌面版共用 `data/catalog.json`。不要同时开着两个界面改数据。

**R6 · 越界即停。**
如果发现完成任务必须改到别的模块，**停下来**告诉使用者「这需要同时改 Mx，理由是……」，得到确认再动手。不要默默跨模块改。如果只是对方的实现有 bug，写进[第 6 节](#6-已知契约偏差必读)并在回复里说明，不要直接改。

---

## 4. 任务路由表

「我想做 X」→「该读哪份文档」→「该改哪些文件」。**未列出的任务，按 R6 先确认模块归属。**

| 我想做的事 | 模块 | 先读 | 只改 |
| --- | --- | --- | --- |
| 加一个字段到条目上（如「评分」） | `M0` 起，需同步 | data-catalog.md → 三端各自接口文档 | `data/catalog.json` 的既有数据由 `M1` 迁移；三端读写逻辑分别改 |
| 加一个新的内置分类（如「音乐」） | `M1` + `M4` | http-api.md、app-ui.md | `server/server.js` 的 `DEFAULT_CATEGORIES`；前端若需展示再动 `app/index.html` |
| 改扫描规则（扫哪些文件、怎么命名） | `M1` | http-api.md | `server/server.js` 的 `scan()` |
| 改新建/编辑条目的表单与行为 | `M4` + `M1` | app-ui.md、http-api.md | `app/index.html` 主脚本；后端不改，除非契约要变 |
| 改拖拽排序手感（阈值、动画） | `M4` 的拖拽子模块 | app-ui.md | `app/index.html` 第三个 `<script>` 块（`M4-DRAG`） |
| 改图标上传压缩参数 | `M4` | app-ui.md | `app/index.html` 的 `shrinkImage()` |
| 改图标制作流程（SVG 截图/缩放） | `M6` | data-catalog.md 的「图标」一节 | `assets/icons/` 与流程说明；**不要**改前端压缩逻辑 |
| 改编辑器字段 / 加一个新类型的表单 | `M4`（+`M0` 若加 kind） | app-ui.md 第 3、4 节 | `app/index.html` 的 `.kfields[data-kind]` 与 `setEditorKind()`；加 `kind` 则需同步 `M0`/`M1`/`M3` |
| 改窗口尺寸 / CSP | `M3` | tauri-ipc.md、app-ui.md 第 5.2 节 | `desktop/src-tauri/tauri.conf.json`；注意内联 `style` 属性会被 CSP 拦掉 |
| 改端口、请求体上限 | `M1` | http-api.md | `server/server.js` |
| 解析 `.lnk` / 提取图标 | `M2` | shortcut-lib.md | `server/shortcut.js`；调用方 `M1` 不改 |
| 改构建/发布流程 | `M5` | build-and-verify.md | `package.json`、`*.cmd`、`.github/workflows/` |
| 改隐私规则、忽略清单 | 仓库级 | AGENTS.md、build-and-verify.md | `.gitignore`、`SECURITY.md` |

---

## 5. 数据流

```text
                  ┌──────────────── M4 前端界面（app/index.html）────────────────┐
                  │  request('/items')  ──┬── nativeInvoke 存在？ ──┐            │
                  └───────────────────────┼──────────────────────┼────────────┘
                                          │ 否                   │ 是
                                          ▼                      ▼
                              M1 HTTP API (127.0.0.1:8765)   M3 Tauri IPC
                                          │                      │
                                          ├──► M2 解析 .lnk/图标  │
                                          ▼                      ▼
                                  ┌─────────── M0 data/catalog.json ───────────┐
                                  │  + data/分类目录 下的真实文件              │
                                  └────────────────────────────────────────────┘
```

- 浏览器版：双击 `start-server.cmd`，前端走 `fetch('http://127.0.0.1:8765/api/...')`。
- 桌面版：双击 `个人工作台.exe`，前端检测到 `window.__TAURI__.core.invoke` 就走 IPC，无需 Node 服务。
- **前端是唯一的界面**，`M3` 与 `M1` 都只是在为它提供后端能力。

---

## 6. 已知契约偏差（必读）

本节记录**核对了代码后确认的偏差**。首次编写时列了 5 条，随后一次三端统一的改动修掉了其中大部分；下面先列残留项，再留档已修复项（避免以后有人照着旧描述去「修」已经不存在的问题）。

### 6.1 时间戳格式两端不一致（中风险，**仍存在**）

| 写入方 | `createdAt` / `updatedAt` / `lastOpenedAt` 格式 |
| --- | --- |
| `M1` 浏览器后端 | ISO 8601 字符串（`2026-09-20T06:12:00.000Z`） |
| `M3` 桌面端 | Unix 秒的**字符串**（`1758348720`） |

前端 `timestamp()` 两种都兼容，所以界面上看不出问题；但同一条目被两端交替写过，会混用两种格式。

**这是目前唯一未收敛的字段级偏差。** 若要做统一，属于 `M0` 契约变更（需同步 `M1`/`M3`/`M4`），先确认以哪种格式为准再动手。**不要**在某一端单方面改。

### 6.2 排序序号 `order` 是全体共用的一维空间（低风险，**部分存在**）

`/api/items/reorder`（与 `reorder_items`）的语义是「`ids` 的下标即新的 `order` 值，**只更新出现在 `ids` 里的条目**」——所以不再有「后提交的网格覆盖整个 `order` 空间」的问题。

残留点：`order` 仍是**全体条目共用的一个数字空间**，两个不同分类的条目可能取到相同的 `order`。因为每个视图按 `category` 过滤后再排序，跨视图撞号通常无影响；同一视图内撞号时由 `score()` 兜底。

### 6.3 已修复的偏差（留档，**不要再按旧描述去修**）

| 原偏差 | 现状 |
| --- | --- |
| 桌面端写回会删掉 `categories` 整段（高风险） | ✅ **已修复**。`main.rs` 的 `Catalog` 增加了 `categories: Vec<Category>`，读取时补齐默认分类、写入时原样回写 |
| 桌面端用 `url`、浏览器端用 `target` | ✅ **已统一**。两端都写 `target`；`url` 降级为只读兼容字段，读取时迁移成 `target` |
| 桌面端丢失 `arguments` / `workingDirectory` / `iconLocation` | ✅ **已修复**。`Item` 结构体已含这三个字段，快捷方式属性两端都能保留 |
| 桌面端不认识「应用」「文档」分类（`create_item` 报「不支持的类别」） | ✅ **已修复**。两端 `DEFAULT_CATEGORIES` 是同一套：脚本 / 网址 / 应用 / 文档 / 待整理 |
| 桌面端没有分类管理、没有快捷方式扫描 | ✅ **已修复**。IPC 命令已补齐 16 个，与 `IPC_MAP` 一一对应（见 [tauri-ipc.md](interfaces/tauri-ipc.md) 第 4 节） |
| `shortcut.rs` 已写好但未接线（`main.rs` 里没有 `mod shortcut;`） | ✅ **已接线**。`shortcut.rs` 已参与编译，`list_shortcuts` / `shortcut_detail` 都走它 |
| 前端只渲染 `script` / `link`，`metadata` / `hide` 两个接口无调用方 | ✅ **已修复**。界面改为完全数据驱动（分类来自 `catalog.json`）；`/api/items/metadata` 由「待补充 → 归类」流程调用，`/api/items/hide` 由右键菜单调用 |

### 6.4 两端能力仍然不完全等价的地方

修复后两端的**能力**已对齐，但实现细节仍有差异，改代码时注意：

| 项 | `M1` 浏览器端 | `M3` 桌面端 |
| --- | --- | --- |
| 时间戳格式 | ISO 8601 | Unix 秒字符串（见 6.1） |
| 打开程序/网址 | 调系统 `cmd /c start` / 默认浏览器 | Tauri `shell` / `opener` 能力 |
| 前端能否直连磁盘 | ❌（只能走 HTTP） | ❌（只能走 IPC） |

**任何一端新增接口，都要同时补另一端的实现与两份接口文档**，否则前端在另一端会静默失败（`request()` 会抛「桌面端还没有实现 …」）。

---

## 7. 接口文档索引

| 文档 | 覆盖模块 | 什么时候必须读它 |
| --- | --- | --- |
| [interfaces/data-catalog.md](interfaces/data-catalog.md) | `M0`、`M6` | 任何涉及字段、分类、状态、图标、磁盘布局的改动 |
| [interfaces/http-api.md](interfaces/http-api.md) | `M1` | 改后端、或前端要调新接口 |
| [interfaces/shortcut-lib.md](interfaces/shortcut-lib.md) | `M2` | 改 `.lnk` 解析、图标提取 |
| [interfaces/tauri-ipc.md](interfaces/tauri-ipc.md) | `M3` | 改桌面客户端、或前端要走 IPC |
| [interfaces/app-ui.md](interfaces/app-ui.md) | `M4` | 改界面、DOM 结构、交互、样式类 |
| [interfaces/build-and-verify.md](interfaces/build-and-verify.md) | `M5` | 编译、验证、提交、发布前 |

其他文档：[DATA_FORMAT.md](DATA_FORMAT.md)（面向使用者的数据说明）、[CLIENT.md](CLIENT.md)（面向使用者的客户端说明）、[../README.md](../README.md)、[../CONTRIBUTING.md](../CONTRIBUTING.md)。

---

## 8. 每份接口文档的固定骨架

为了让 agent 能稳定检索，`docs/interfaces/` 下的每份文档必须包含这四个二级标题（`scripts/check-structure.js` 会校验）：

1. `## 职责与边界` —— 这个模块负责什么、**允许读写**哪些文件
2. `## 对外接口` —— 契约本体（表格）
3. `## 禁止事项` —— 明确写出不能做什么，以及越界时应怎么办
4. `## 变更记录` —— 接口变动留痕（日期 + 变更点）
