# 接口文档 · `M1` 浏览器后端（HTTP API）

## 职责与边界

`server/server.js` 是一个**零第三方依赖**的本机 HTTP 服务：扫描 `data/` 目录、读写索引、打开脚本/网址/应用/文档、解析 Windows 快捷方式。它只为浏览器版提供能力，桌面版不经过它。

**允许读写**

| 路径 | 权限 |
| --- | --- |
| `server/server.js` | ✅ 本模块唯一入口，改后端只改这里 |
| `data/catalog.json` | ✅ 通过 `readCatalog()` / `writeCatalog()` |
| `data/` 下的分类目录 | ✅ 通过 `itemFilePath()` 限定的两层路径 |
| `server/shortcut.js` | ⚠️ **只读调用**。它是 `M2` 模块，要改去 [shortcut-lib.md](shortcut-lib.md) |
| `app/index.html` | ❌ 属于 `M4`。接口要变就更新本文档，由前端侧自己跟进 |
| `desktop/` | ❌ 属于 `M3` |

**不允许**：引入任何 npm 依赖（`package.json` 里没有 `dependencies`，且要保持这样）；把监听地址从 `127.0.0.1` 改成别处；读写 `data/` 之外的路径。

---

## 对外接口

### 基本约定

- **基地址**：`http://127.0.0.1:8765`，端口写死在 `server.js` 的 `PORT` 常量。
- **前缀**：所有业务接口以 `/api` 开头。
- **请求体**：JSON。**上限 8 MiB**（`readBody()`），超过返回 500「请求过大」。
- **响应体**：裸 JSON 对象（没有 `{code,data}` 包装）。失败时统一为 `{ "error": "中文原因" }`。
- **响应头**：`Access-Control-Allow-Origin: *`（因为只监听回环地址，且要允许 `file://` 打开的页面跨域）、`Cache-Control: no-store`。
- **错误状态码**：`400` 参数不合法 / `404` 找不到 / `409` 文件名冲突 / `500` 服务端异常。路径不存在时 `404 {"error":"接口不存在。"}`。
- **`OPTIONS *`** → `204`，用于 CORS 预检。

### 1. 状态与数据

| 方法 | 路径 | 请求 | 响应 | 错误 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/status` | — | `{ ok: true, root, dataPath }` | — |
| `GET` | `/api/items` | — | `{ items: Item[], categories: Category[], kinds: string[] }` | — |
| `GET` | `/api/browsers` | — | `{ browsers: Browser[] }` | — |
| `POST` | `/api/scan` | `{}` | `{ discovered: Item[], removed: Item[], items: Item[], categories: Category[] }` | — |

`Item` / `Category` 的结构以 [data-catalog.md](data-catalog.md) 为准。`kinds` 恒为 `["script","link","app","file"]`。

`Browser` = `{ key, label, path }`：**本机已安装的浏览器**，`key` 是内置标识（`chrome` / `edge` / `firefox` / `brave` / `vivaldi` / `opera` / `chromium`），`path` 是 exe 绝对路径。表来自 `server.js` 顶部的 `BROWSERS` 常量（按 `ProgramFiles` / `ProgramFiles(x86)` / `LOCALAPPDATA` 拼路径逐个探存在性）。**它不包含「系统默认浏览器」**——条目的 `browser` 为空即代表跟随系统默认，所以调用方只需要在列表前面自己加一个「跟随系统默认」选项。非 Windows 平台返回空数组。

### 1.1 网址图标预览

| 方法 | 路径 | 请求 | 响应 | 错误 |
| --- | --- | --- | --- | --- |
| `POST` | `/api/links/icon` | `{ url }` | `{ icon, source }` | `400` 网址无效、超时或没有可用图标 |

只读取用户刚输入的 `http(s)` 网址：先解析网页的 `<link rel="icon">` / `apple-touch-icon`，按 `sizes` / SVG / Apple Touch 声明优先选择高清版本，再回退到站点根目录的 `/favicon.ico`。请求超时 6 秒、最多跟随 3 次跳转，网页正文最多 256 KiB、图标最多 512 KiB；返回的 `icon` 是临时 data URL，前端会压缩后才写进条目的既有 `icon` 字段。

### 2. 分类管理

| 方法 | 路径 | 请求 | 响应 | 错误 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/categories` | — | `{ categories: Category[] }` | — |
| `POST` | `/api/categories` | `{ label, kind?, symbol?, note? }` | `201 { category, categories }` | `400` 名称为空 / 分类名重复 / 文件夹名重复或不可用 |
| `POST` | `/api/categories/update` | `{ key, label, symbol?, note?, kind?, order? }` | `{ category, categories }` | `404` 分类不存在；`400` 名称为空或与其它分类重名 |
| `POST` | `/api/categories/delete` | `{ key, force?, removeFolder? }` | `{ ok: true, categories }` | `404` 不存在；`400` 内置分类不可删 / 分类内还有条目 |

行为细节（调用方需要知道的副作用）：

- `POST /api/categories` 会在 `data/` 下**真的建出文件夹**并写入 `.gitkeep`；`key` 自动生成为 `cat-<sha1(label)前8位>`。
- `POST /api/categories/update`：非内置分类改 `label` 时，会尝试把旧文件夹整体 `rename` 成新的 `folder`，并同步改写该分类下所有条目的 `sourcePath` 前缀。若目标文件夹已存在或改名失败，**只改显示名，文件夹保持不动**。
- `POST /api/categories/delete`：默认拒绝删除非空分类（返回 `count` 提示剩余数量）；带 `force: true` 时**会连同条目的磁盘文件一起删除**；带 `removeFolder: true` 时连文件夹一起递归删除。

### 3. 快捷方式与应用清单（依赖 `M2`）

| 方法 | 路径 | 请求 | 响应 | 错误 |
| --- | --- | --- | --- | --- |
| `GET` | `/api/shortcuts` | — | `{ shortcuts: [{ name, path, group }] }` | — |
| `GET` | `/api/shortcuts/icon?path=<绝对路径>` | query `path` | `{ icon, detail? }` | `404` 文件不存在；`400` 不支持的类型 |

- 扫描范围：开始菜单（当前用户 + 所有用户）、桌面（当前用户 + 公共），深度上限 5 层，按文件名去重，按名称中文排序。`group` 取值 `桌面` / `所有用户` / `当前用户` / `商店应用`。
- 除 `.lnk` 快捷方式外，还会通过 PowerShell `Get-StartApps` 枚举 Windows 商店应用（UWP/MSIX），它们的 `path` 使用 `shell:AppsFolder\<AUMID>` 格式，`group` 为 `商店应用`。
- `icon` 只接受 `.lnk` `.exe` `.ico` `.dll` `.png` `.jpg` `.jpeg`。`.lnk` 额外返回 `detail`（结构见 [shortcut-lib.md](shortcut-lib.md) 的 `publicShortcutDetail`）。`shell:AppsFolder\` 路径返回空图标与基本 detail。
- 传进来的 `path` 是**任意绝对路径**（不限于 `data/`），这是扫描用户开始菜单的必要代价；调用方必须来自本机界面。

### 4. 条目增删改

| 方法 | 路径 | 请求 | 响应 | 错误 |
| --- | --- | --- | --- | --- |
| `POST` | `/api/items` | 见下 | `201 { item, categories }` | `400` 分类无效/名称为空/网址或路径无效；`409` 同名脚本已存在且未传 `overwrite` |
| `POST` | `/api/items/update` | `{ id, title, description?, tags?, icon?, clearIcon?, target?, browser? }` | `{ item }` | `400` 找不到条目 / 名称为空 / 网址无效 |
| `POST` | `/api/items/delete` | `{ id }` | `{ ok: true }` | `404` 找不到条目 |
| `POST` | `/api/items/reorder` | `{ ids: string[] }` | `{ items }` | `400` `ids` 不是数组 |
| `POST` | `/api/items/metadata` | `{ id, title, category, description?, tags?, icon?, target?, browser?, newCategory? }` | `{ item, categories }` | `400` 条目不存在/名称为空/分类无效/网址分类收到非 `.url` |
| `POST` | `/api/items/hide` | `{ id }` | `{ item }` | `404` 条目不存在或不是 `needs_metadata` |
| `POST` | `/api/items/open` | `{ id }` | `{ ok: true }` | `404` 条目不存在或不可打开；`400` 目标文件/网址无效 |

**`POST /api/items` 请求体按 `kind` 分流：**

```jsonc
{
  "category": "script",        // 必填，必须是已存在的分类 key（不能用收件箱）
  "title": "示例脚本",          // 必填
  "description": "",           // 可选
  "tags": ["示例"],             // 可选，最多 20 个
  "icon": "data:image/png;base64,...", // 可选，data URL，>2MiB 会被截断
  "newCategory": { "label": "示例分类", "kind": "file" }, // 可选：分类不存在时顺手新建

  // kind = script
  "extension": "ps1",          // 可选，默认 txt，只保留字母数字，最长 10
  "content": "Write-Host 'hi'", // 脚本正文，写入 data/脚本/<title>.<extension>
  "overwrite": false,          // 同名文件已存在时必须为 true

  // kind = link
  "target": "https://example.com", // 或以 url 传入；会自动补 https:// 并校验 http/https
  "browser": "chrome",            // 可选：只对 link 生效。留空/不传 = 跟随系统默认浏览器；
                                  // 也可以是本机浏览器 exe 的绝对路径（用于未收录的浏览器）

  // kind = app
  "path": "C:\\...\\示例.lnk",   // 或 target；必须是存在的文件
                                  // 会把 .lnk 复制进 data/应用/，保留参数/工作目录/图标

  // kind = file
  "target": "C:\\...\\示例.docx" // 文件或文件夹路径
}
```

**`/api/items/open` 的打开方式**（`kind` 决定 `target` 怎么解释）：

| kind | 行为 |
| --- | --- |
| `script` | 用 `cmd /c start` 打开 `data/脚本|文档/...` 里的真实文件 |
| `link` | 必须是 `http(s)://`，交给系统默认浏览器 |
| `app` | 优先用复制进 `data/应用/` 的 `.lnk`，否则回落到 `target` |
| `file` | `target` 或 `sourcePath` 解析出的绝对路径。文件和目录都适用同一个内置「文件」分类（`key: file`）；目录交给资源管理器打开 |

成功后 `openCount += 1`、`lastOpenedAt = now`。「常用」网格按 `openCount * 12 + 30 天内衰减分` 排序。

**`/api/items/reorder` 语义**：`ids` 的下标即新的 `order` 值，**只更新出现在 `ids` 里的条目**（不会整段覆盖）。⚠️ 但 `order` 仍是全体条目共用的一维数字空间，不同分类的条目可能撞号；同视图内撞号由前端的 `score()` 兜底。详见 [INTERFACES.md 6.2](../INTERFACES.md#62-排序序号-order-是全体共用的一维空间低风险部分存在)。

**迁移语义**（`/api/items/metadata`，由前端的「待补充 → 归类」流程调用）：把 `inbox` 里的条目移到目标分类，会**真的移动磁盘文件**并改写 `sourcePath`；目标为 `link` 时只接受 `.url` 文件。请求体里的 `newCategory` 可以顺手新建分类。

---

## 禁止事项

- ❌ **不要新增 npm 依赖**。这个服务的卖点之一是「装了 Node 就能跑」。
- ❌ **不要把监听地址改成 `0.0.0.0`** 或加认证之外的暴露方式。它没有账号体系，安全完全依赖「只绑回环」。
- ❌ **不要在 `server.js` 里直接解析 `.lnk` / 提取图标**。那是 `M2` 的活，通过 `readShortcut()` / `readIconDataUrl()` 调用。
- ❌ **不要绕过 `itemFilePath()` 拼接 `data/` 下的路径**，否则会破坏「两层路径」这个安全假设。
- ❌ **不要在前端侧改动之前单方面改字段名**（例如把 `target` 改名）。接口变更必须走 R3：同一次提交更新本文档，且提示前端侧。
- ❌ **不要把 `/api/items/metadata`、`/api/items/hide` 当死代码删掉**。两者都有真实调用方：「待补充 → 归类」走 `metadata`，右键菜单的「忽略」走 `hide`。

越界时应怎么办：接口不够用时，**先改本文档把契约写清楚**，再改实现，并明确告诉你「这会影响 `M4`，前端需要同步」或「桌面包需要同步」，由你决定是否一并做。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-20 | 首次编写。冻结 17 条路由（含 `OPTIONS`）、8 MiB 请求上限、错误码约定、按 `kind` 分流的请求体。标注 `metadata`/`hide` 两个接口当前无调用方。 |
| 2026-09-20 | 同步前端接入后的状态：`/api/items/metadata`（待补充归类，含 `newCategory` 内联新建）与 `/api/items/hide`（右键「忽略」）都已有调用方；改正 `reorder` 描述——它只更新传入 `ids` 里的条目，不再整段覆盖 `order`。 |
| 2026-09-21 | 新增内置分类「文件夹」（`key: folder`，`kind` 仍是 `file`，`KINDS` 未变、`/api/items` 的 `kinds` 仍为四个值）；`readCatalog()` 增加内置分类补齐，于是 `categories` 会对已有 `catalog.json` 多出这一项。`/api/items/open` 的 `file` 分支明确支持目录（交给资源管理器打开）。 |
| 2026-09-21 | `/api/items/metadata` 多了一个调用方：**`M4` 的「合并包」**用它把条目移进包里（请求体 `{ id, title, description, tags, icon, target, browser, category }`）。语义没变——对**非收件箱**条目只改 `category`、不搬磁盘文件（所以包里的 `.lnk` 仍留在 `data/应用/`，`/api/items/open` 按分类 `kind` 打开，行为不变）。⚠️ 调用方注意：该接口会把 `description` / `tags` **整段覆盖**成请求体里的值，移进包时必须原样回传条目现有内容，否则会被清空。 |
| 2026-09-21 | 新增 `POST /api/links/icon`：按用户输入的网址临时读取网页声明的 favicon（失败回退 `/favicon.ico`），受超时、跳转和体积限制；返回 data URL 给 M4 压缩后写入既有 `Item.icon`，不新增数据字段。 |
| 2026-09-22 | 优化 `POST /api/links/icon` 的候选顺序：网页同时声明多种尺寸时，优先 `sizes` 更大、SVG 或 Apple Touch 图标，避免先取到 16px favicon 后被界面放大。接口字段与限制不变。 |
