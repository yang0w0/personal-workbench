# 接口文档 · `M3` 桌面客户端（Tauri IPC）

## 职责与边界

`desktop/src-tauri/` 是便携式 Windows 客户端：一个极薄的 Tauri 外壳 + 一组 IPC 命令，直接读写 `data/`，**不需要 Node 服务**。界面仍然是 `app/index.html`（编译时被内嵌进 exe）。

**允许读写**

| 路径 | 权限 |
| --- | --- |
| `desktop/src-tauri/src/main.rs` | ✅ 后端逻辑全在这里 |
| `desktop/src-tauri/tauri.conf.json` | ✅ 窗口、CSP、内嵌目录配置 |
| `desktop/src-tauri/Cargo.toml` | ✅ Rust 依赖（当前无第三方依赖，只有 tauri / serde） |
| `data/` | ✅ 通过 `workspace_root()` 向上寻找的 `data` 目录 |
| `app/index.html` | ❌ 属于 `M4`。**改了前端必须重新编译 exe 才在客户端生效** |
| `server/` | ❌ 属于 `M1`/`M2`。桌面端不复用它们 |

**不允许**：使用固定盘符/用户名/安装目录（必须保持便携）；开启 `bundle.active`（当前是便携模式，只产出 exe）；在 `main.rs` 里实现网络请求。

---

## 对外接口

前端通过 `window.__TAURI__.core.invoke('<命令>', { input })` 调用。`nativeInvoke` 存在即走本通道，因此**浏览器版的 HTTP 契约与这里的命令名必须语义一致**。

### 1. IPC 命令清单（10 个）

| 命令 | 入参 | 返回 | 对应 HTTP 接口 |
| --- | --- | --- | --- |
| `get_status` | — | `{ ok: true, dataPath }` | `GET /api/status` |
| `get_items` | — | `{ items: Item[] }`（**无 `categories`**） | `GET /api/items` |
| `scan_items` | — | `{ discovered, removed, items }` | `POST /api/scan` |
| `create_item` | `{ input: NewItem }` | `{ item }` | `POST /api/items` |
| `complete_metadata` | `{ input: { id, category, title, description, tags } }` | `{ item }` | `POST /api/items/metadata` |
| `hide_item` | `{ input: { id } }` | `{ item }` | `POST /api/items/hide` |
| `open_item` | `{ input: { id } }` | `{ ok: true }` | `POST /api/items/open` |
| `update_item` | `{ input: { id, title, description, tags, icon? } }` | `{ item }` | `POST /api/items/update` |
| `delete_item` | `{ input: { id } }` | `{ ok: true }` | `POST /api/items/delete` |
| `reorder_items` | `{ input: { ids: string[] } }` | `{ items }` | `POST /api/items/reorder` |

失败时命令返回 `Err(中文字符串)`，表现为 **reject**（不是 `{error}` 对象）。前端 `request()` 的 `catch` 分支负责把它变成界面提示——这是与 HTTP 通道返回 `{error}` 的差异，前端适配层已经统一处理。

### 2. `NewItem`（`create_item` 的入参）

```jsonc
{
  "category": "script",   // 仅支持 "script" | "link"，其它返回「不支持的类别。」
  "title": "示例脚本",     // 必填，空则报「类别和名称不能为空。」
  "description": "",
  "tags": ["示例"],        // 去空、最多 20 个
  "extension": "ps1",     // 仅 script 用；只保留 ASCII 字母数字，最长 10，空则 txt
  "content": "...",       // 仅 script 用；写入 data/脚本/<title>.<ext>
  "url": "https://example.com", // 仅 link 用；必须以 http:// 或 https:// 开头
  "icon": "data:image/..." // 可选，>2 MiB 时被丢弃（HTTP 端是截断）
}
```

### 3. 磁盘与路径解析

| 函数 | 行为 |
| --- | --- |
| `workspace_root()` | 优先取 exe 所在目录（若其下有 `data/`），其次向上逐级找含 `data/` 的目录，最后用当前工作目录 |
| `data_root()` | `workspace_root()/data` |
| `catalog_path()` | `data/catalog.json` |
| `source_join(source)` | 只接受**两层**相对路径（`分类/文件名`），且解析后必须仍在 `data/` 内，否则返回 `None` |
| `ensure_directories()` | 创建 `脚本` `网址` `待整理` `密码库` `备份` 五个目录 |
| `new_id()` | `<unix秒>-<pid>-<自增计数器>` |
| `now()` | **Unix 秒的字符串**（HTTP 端是 ISO 8601） |
| 写盘 | 先写 `catalog.json.tmp` 再 `rename` 覆盖 |

### 4. 前端适配层契约（`M4` 侧需要知道的）

`app/index.html` 的 `request()` 里维护了 HTTP 路径 → IPC 命令的映射表：

```js
{'/status':'get_status','/items':'get_items','/scan':'scan_items',
 '/items/metadata':'complete_metadata','/items/hide':'hide_item','/items/open':'open_item',
 '/items/update':'update_item','/items/delete':'delete_item','/items/reorder':'reorder_items'}
```

- 新增 HTTP 路径时，**必须同时在 `M4` 的这张表里加映射**，否则浏览器版能用、桌面版会 `invoke(undefined)` 失败。
- `GET /status`、`GET /items` 不传 `input`；其余命令统一包一层 `{ input: body }`。

### 5. 配置契约（`tauri.conf.json`）

| 键 | 值 | 为什么不能随便改 |
| --- | --- | --- |
| `build.frontendDist` | `../../app` | 决定把 `app/` 打进二进制。改完前端**必须重新编译**才生效 |
| `app.withGlobalTauri` | `true` | 前端靠 `window.__TAURI__` 判断走哪条通道 |
| `app.windows[].label` | `main` | 与 `capabilities/default.json` 的 `windows` 匹配，权限按窗口下发 |
| `app.windows[].decorations` | `false` | **无边框窗口**：系统标题栏由前端顶栏自绘（拖拽区 + `#winMin/#winMax/#winClose`，见 app-ui.md 5.1）。改回 `true` 会出现两条标题栏；改掉前端拖拽区则窗口无法拖动 |
| `app.windows[].dragDropEnabled` | `false` | **Tauri 2 在 Windows 上默认接管 OS 级拖放，会禁用 WebView2 内的 HTML5 DnD**。前端拖拽排序用的是 Pointer Events，但保留该配置以防被接管 |
| `app.security.csp` | 见下 | 少一项就会白屏或图标消失 |

**能力文件 `capabilities/default.json`**（2026-09-20 新增）：声明 `core:default` + `core:window:allow-start-dragging / allow-minimize / allow-toggle-maximize / allow-close`，是自绘标题栏的最小权限集；`windows` 用通配 `"*"` 匹配。删掉它们窗口按钮会静默失效（前端只有 console 报错）。

CSP 必须包含：

```text
default-src 'self';
img-src 'self' data: blob:;                          ← 少了 data: 自定义图标全部空白
connect-src 'self' ipc: http://ipc.localhost http://127.0.0.1:8765;  ← 少了 ipc: 会让 IPC 报 CSP 违规并退化
style-src 'self' 'unsafe-inline';
script-src 'self' 'unsafe-inline'
```

> 历史 bug：CSP 缺 `data:` 导致所有自定义图标渲染空白，连占位符号也被顶掉；缺 `ipc: http://ipc.localhost` 导致 Tauri IPC 报 CSP 违规。

---

## 禁止事项

- ❌ **不要引入固定路径**（`D:\个人工作台`、用户名、安装目录）。客户端必须能整个文件夹搬走就用。
- ❌ **不要随意改 `Catalog` 结构体**。它**缺少 `categories` 字段**，写回时会丢掉该字段——见下方「已知偏差」，修改前先读。
- ❌ **不要在前端没有同步的情况下改命令名**。命令名变更必须同时改 `M4` 的映射表，并更新本文档。
- ❌ **不要开启 `bundle.active`**，也不要在源码里写死窗口自动重载等开发专用逻辑。
- ❌ **不要在 `main.rs` 里做 HTTP 请求或调用 `M2` 的能力**（Rust 侧没有 `.lnk` 解析，图标只能拿到前端上传的 data URL）。
- ❌ **改完 `app/index.html` 不要以为桌面版立刻生效**。必须重新构建：双击 `build-client.cmd`（或 `npm run desktop:build`），且构建前确认 `个人工作台.exe` 没在运行，否则覆盖失败。详见 [build-and-verify.md](build-and-verify.md)。

越界时应怎么办：桌面端要支持新分类/新字段时，这属于 **`M0` + `M3`（+ `M4`）的联合改动**。先读 [data-catalog.md](data-catalog.md) 确认字段语义，再改 `main.rs`，并在回复里明确「桌面端与浏览器端的差异已缩小到 X 项」。

---

### 6. Rust 侧已有但未接线的解析器

`desktop/src-tauri/src/shortcut.rs`（约 1040 行）是 `M2` 的 **Rust 移植版**，导出：

```rust
pub struct ShortcutInfo { ... }
pub fn read_shortcut(lnk_path: &Path) -> ShortcutInfo
pub fn read_icon_data_url(icon_location: &str, icon_index: i32, target: &str) -> String
```

内部还包含 `parse_lnk_buffer` / `ico_from_pe` / `base64_encode` / `base64_decode` 与一个 `mod tests` 单元测试模块，能力与 Node 版 `M2` 对齐。

**但它当前没有接线**：`main.rs` 里既没有 `mod shortcut;` 也没有任何调用，所以在 Rust 中这个文件**不参与编译**，对现有 exe 没有任何影响。

后续要给桌面端补「应用/文档分类」「扫描快捷方式」这类能力时：

- ✅ **复用它**，先加 `mod shortcut;` 再调用 `read_shortcut()` / `read_icon_data_url()`；
- ❌ 不要重新写一份 `.lnk` 解析（那是 1000 行级别的二进制解析，Node 版踩过的坑会再踩一遍）；
- 接线时请同步本文档与 [INTERFACES.md](../INTERFACES.md) 第 6.4 节的能力对照表。

---

## 已知偏差

以下来自 [INTERFACES.md 第 6 节](../INTERFACES.md#6-已知契约偏差必读)。

**已修复（留档，不要再按旧描述去修）**：

| 曾经的偏差 | 现状 |
| --- | --- |
| `struct Catalog` 缺 `categories`，写回时整段删除 | ✅ 已加 `categories: Vec<Category>`，读时补齐默认分类、写时原样回写 |
| 桌面端用 `url`，浏览器端用 `target` | ✅ 统一写 `target`；`url` 降级为只读兼容字段 |
| `ensure_directories` 只建旧五件套 | ✅ `default_categories()` 与 `M1` 同一套（脚本/网址/应用/文档/待整理） |
| `scan()` 只扫三个固定目录、不理解 `kind` | ✅ 改为遍历 `catalog.categories` 的 `folder`，并按 `kind` 补信息 |
| `create_item`/`open_item`/`complete_metadata` 只支持 script/link | ✅ 四种 `kind` 全支持；`open_item` 按 `kind` 分派 |
| `update_item` 不支持 `target` / `clearIcon` | ✅ 已支持 |
| `src/shortcut.rs` 已写好但未接线 | ✅ 已 `mod shortcut;` 参与编译，`list_shortcuts`/`shortcut_detail` 都走它 |

**仍存在**：

| 偏差 | 严重度 | 说明 |
| --- | --- | --- |
| 时间戳写 Unix 秒字符串 | 🟡 中 | `M1` 写 ISO 8601，本模块写 Unix 秒。前端 `timestamp()` 两种都兼容，但同一条目被两端交替写过会混用格式。统一属于 `M0` 变更，见 [INTERFACES.md 6.1](../INTERFACES.md#61-时间戳格式两端不一致中风险仍存在) |

**这些偏差是记录，不是待办。** 修它们属于跨模块改动，需要使用者确认后再动手。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-20 | 首次编写。冻结 10 个 IPC 命令、`NewItem` 入参、路径解析规则、CSP/窗口配置契约；记录 6 处与浏览器端的已知偏差（含 `categories` 丢失这一高风险项）；标注 `src/shortcut.rs` 已存在但未接线。 |
| 2026-09-20 | 窗口改为无边框（`decorations:false`），系统标题栏由前端自绘；新增 `windows[].label=main` 与能力文件 `capabilities/default.json`（窗口拖拽/最小化/最大化/关闭四个权限）。前端侧契约见 app-ui.md 5.1。 |
| 2026-09-20 | **与浏览器端契约统一**：`Catalog` 增加 `categories`（修掉写回丢分类的高风险 bug）；`Item` 增加 `target`/`arguments`/`working_directory`/`icon_location`（`url` 保留为只读兼容）；`default_categories()` 与 `M1` 对齐为同一套五个分类；`scan()` 改为按 `catalog.categories` 的 `folder` 遍历；IPC 命令补齐到 16 个并与前端 `IPC_MAP` 一一对应；`shortcut.rs` 接线参与编译。已修复项移入「已修复」留档，仅剩时间戳格式未收敛。 |
