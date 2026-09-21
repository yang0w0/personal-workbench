# 接口文档 · `M0` 数据契约（catalog.json）

## 职责与边界

本模块定义**落盘数据的形状**：索引文件 `data/catalog.json` 的字段、分类目录的布局、条目的状态机与图标格式。

它不是一个代码文件，而是 `M1`（浏览器后端）、`M3`（桌面客户端）、`M4`（前端界面）共同遵守的契约。任何一端改变了数据形状，都必须在同一次提交里更新本文件并检查另外两端。

**允许读写**

| 路径 | 说明 |
| --- | --- |
| `data/catalog.json` | 索引文件。只允许由 `M1` 或 `M3` 通过接口写入，**不允许手工编辑** |
| `data/脚本/` `data/网址/` `data/应用/` `data/文档/` `data/待整理/` | 分类目录，存真实文件 |
| `data/密码库/` `data/备份/` | 预留目录，**当前不扫描、不读取、不写入** |
| `assets/icons/` | 图标制作素材（模块 `M6`），成品写入条目的 `icon` 字段 |

**不允许**：把 `data/` 下的任何内容提交到 Git（`.gitignore` 已忽略整个 `data/`）；让代码读 `data/` 以外的用户文件（`M3` 读取用户选择的 `.lnk` 除外，那是用户显式行为）。

---

## 对外接口

### 1. 索引文件整体结构

```json
{
  "version": 2,
  "categories": [ /* Category[] */ ],
  "items": [ /* Item[] */ ]
}
```

- `version`：当前为 `2`。`M1` 读到缺少 `categories` 的旧文件时会自动补齐默认分类并把条目里的 `url` 迁移成 `target`（就地迁移，不递增版本号）。
- 写入方式：`M1` 与 `M3` 都先写 `catalog.json.tmp` 再 `rename` 覆盖，避免写坏原文件。

### 2. `Category`（分类）

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `key` | string | ✅ | 分类标识。内置分类是固定小写单词；自建分类为 `cat-<sha1(label) 前 8 位>` |
| `label` | string | ✅ | 界面显示名，最长 24 字符 |
| `folder` | string | ✅ | 对应的 `data/` 下文件夹名，最长 24 字符，已过滤 `<>:"/\|?*` 与控制字符 |
| `kind` | `script` \| `link` \| `app` \| `file` \| `inbox` | ✅ | 决定编辑器长什么样、怎么打开 |
| `symbol` | string | ✅ | 侧边栏图标。**后端一律截到 2 字符**（`M1`/`M3` 的 `normalizeCategory` 都写了 `.slice(0, 2)`）。历史上它是字符型占位符号（`▤` `◆` `↗`）；前端现在的渲染顺序是：能认出图标名就用图标 → 否则按名称关键词猜 → 再否则用该分类 `kind` 的默认图标（见 [app-ui.md](app-ui.md#55-内置图标集)）。所以**「给分类挑一个图标」目前只在名称关键词命中时才生效**，要让任意图标名存下来，得先放宽后端这两处长度限制（属 `M1`/`M3` 变更，见 [INTERFACES.md](../INTERFACES.md) 第 6 节） |
| `note` | string | ✅ | 区块副标题，最长 40 字符 |
| `order` | number | ✅ | 展示顺序 |
| `builtin` | boolean | ✅ | 内置分类，**不可删除**，不可改 `kind` |
| `inbox` | boolean | ✅ | 收件箱标记。**全表最多一个** |

内置分类固定为：

| key | label | folder | kind |
| --- | --- | --- | --- |
| `script` | 脚本 | `脚本` | `script` |
| `link` | 网址 | `网址` | `link` |
| `app` | 应用 | `应用` | `app` |
| `file` | 文件 | `文档` | `file` |
| `inbox` | 待整理 | `待整理` | `inbox` |

> **内置分类读取时自动补齐与迁移**：`M1`/`M3` 读索引时会把默认表里缺失的内置分类按 `key`（或同名 `folder`）补进去，所以新增一个内置分类不需要迁移脚本。反过来，已存在同名 `key` 或同名 `folder` 时不会重复添加；已废弃的内置分类会在读取时按迁移规则收敛。
>
> **文件与文件夹共用一个分类**：两者都用 `key: file` 与 `kind: file`。`kind` 决定编辑器显示哪组字段、点击时怎么打开；文件与目录都只需一个本地路径，所以不需要新增 `kind` 枚举。
>
> 「文件」分类既可收录 `data/文档/` 里扫描到的文件，也可收录任意本地文件或目录的绝对路径（放在 `target`，不写 `sourcePath`）。目录会由资源管理器打开；这类绝对路径条目不会被扫描的失效清理误删。旧版 `folder` 分类及其条目在读取索引时会迁移到 `file`；遗留的 `data/文件夹/` 目录不会再被自动收编为分类。
>
> 两端的默认分类现在是同一套（含 `应用`/`文件`）。除此之外，`M1` 与 `M3` 在读取时都会**扫描 `data/` 下已存在的文件夹并补登为分类**，所以用户自建的 `密码库`/`备份` 等目录也会出现在索引里（它们只是空分类，不带计数徽标）。
>
> **「合并包」不是一个新字段，而是分类的一种用法**（前端 `isPackage()` 认它）：把几条内容收进一个包 = 建一个普通分类（`builtin: false`、`kind` 与成员一致），再把那些条目的 `category` 改挂过去。为了不改 `M0` 契约，包的标记借用两个既有字段——建包时写 `symbol: 'pk'`（**恰好 2 个字符，正好穿过两端 `normalizeCategory` 的 `.slice(0,2)` / `.chars().take(2)`**）并以 `note: '合并包 · 点开查看内容'` 开头，前端渲染成一张文件夹卡（见 [app-ui.md](app-ui.md#3-渲染契约数据-界面)）。也正因为 `symbol` 只有 2 个字符，**合并包目前不支持自选图标**（统一用 `folder` 图标），要支持得先放宽那两处长度限制（见 [INTERFACES.md](../INTERFACES.md) 第 6 节）。
>
> **解散包 = 把成员退回同类内置分类 + 删除这个分类**（`removeFolder: true`）。⚠️ 必须带 `removeFolder`：只有 `.gitkeep` 的空目录如果留在 `data/` 下，会被读取时的「扫描 `data/` 补登分类」重新收编成一个分类。

### 3. `Item`（条目）

| 字段 | 类型 | 必填 | 说明 |
| --- | --- | --- | --- |
| `id` | string | ✅ | 唯一标识。`M1` 用 `crypto.randomUUID()`；`M3` 用 `<unix秒>-<pid>-<自增>` |
| `category` | string | ✅ | 指向 `Category.key` |
| `title` | string | ✅ | 显示名 |
| `description` | string | ✅ | 说明，可为空串 |
| `tags` | string[] | ✅ | 最多 20 个，去空、去首尾空格 |
| `sourcePath` | string | ⬜ | 相对 `data/` 的**两层**路径，形如 `脚本/示例脚本.ps1`。是「文件存在的证据」 |
| `target` | string | ⬜ | 打开目标：网址 URL、文件绝对路径、程序目标路径 |
| `browser` | string | ⬜ | **仅 `link` 类型使用**：用哪个浏览器打开。空串/缺省 = 跟随系统默认浏览器；否则是浏览器 key（`chrome` / `edge` / `firefox` / `brave` / `vivaldi` / `opera` / `chromium`）或浏览器 exe 的绝对路径。其它类型忽略该字段 |
| `url` | string | ⬜ | **历史字段，只读兼容**。老数据里的旧字段名；`M1`/`M3` 读取时都会迁移成 `target`，新写入不再产生它 |
| `icon` | string | ⬜ | 图片 data URL（`data:image/png;base64,...`），长度上限 2 MiB |
| `arguments` / `workingDirectory` / `iconLocation` | string | ⬜ | 仅快捷方式类型使用，由 `M2` 解析得出，**仅 `M1` 会写** |
| `status` | `complete` \| `needs_metadata` \| `hidden` | ✅ | 见下方状态机 |
| `createdAt` / `updatedAt` | string | ✅ | `M1` 写 ISO 8601；`M3` 写 Unix 秒字符串 |
| `openCount` | number | ✅ | 打开次数，用于「常用」排序 |
| `lastOpenedAt` | string | ✅ | 最近打开时间，空串表示从未打开 |
| `order` | number | ⬜ | 手动拖拽排序序号。**注意它是全体条目共用的一维空间**，见 [6.2](../INTERFACES.md#62-排序序号-order-是全体共用的一维空间低风险部分存在) |
| `uwpAppId` | string | ⬜ | **仅 `app` 类型中的 Windows 商店应用**：存储 AUMID（如 `OpenAI.Codex_xxx!App`）。有此字段时 `target` 为 `shell:AppsFolder\<AUMID>`，启动走 `explorer.exe shell:AppsFolder\<AUMID>` 而非文件系统 |

### 4. `status` 状态机

```text
扫描发现文件（M1/M3）
        │
        ▼
 needs_metadata ──「补充信息」──► complete ──「打开」──► complete（openCount+1）
        │
        └──「忽略」──► hidden（不再出现在待补充列表，文件仍在磁盘上）
```

| 值 | 含义 | 由谁产生 |
| --- | --- | --- |
| `complete` | 信息齐全，可作为图标显示、可打开 | `M1` / `M3` |
| `needs_metadata` | 扫描发现但信息未补全，显示在「待补充」 | `M1` / `M3` |
| `hidden` | 用户主动忽略 | `M1` 的 `/api/items/hide`、`M3` 的 `hide_item` |

### 5. 扫描规则（`M1` 的 `scan()` 语义，`M3` 是其子集）

1. **失效清理**：索引里 `sourcePath` 指向的文件不存在 → 从索引移除（磁盘上删文件不会自动删索引，要再扫描一次）。
2. **发现新文件**：遍历每个 `category.folder`，跳过目录与非 `.gitkeep` 之外的点文件；未登记的 `分类/文件名` 生成 `needs_metadata` 条目。
3. **按 `kind` 补信息**：
   - `link` → 从 `.url` 文件读 `URL=`，读到则 `complete`，读不到则 `needs_metadata`
   - `app` → 用 `M2` 解析 `.lnk`，一律 `complete`
   - `file` → `target` 设为真实路径，`complete`
   - `inbox` → 一律 `needs_metadata`

`M3` 与 `M1` 的扫描语义一致：遍历 `catalog.categories` 里每个分类的 `folder`（而不是写死目录名），并按 `kind` 补信息。

### 6. 图标（模块 `M6`）

- 存储位置：条目 `icon` 字段，**data URL**，不是文件。
- 写入尺寸：**128×128 PNG**。前端只在 42px 的格子里显示，不要放大到 512（历史四条图标 512px 合计 1MB+）。
- 前端上传压缩：`M4` 的 `shrinkImage(file, max=256)` —— 最长边缩到 256px，PNG 超 400KB 时转 JPEG(0.9)。因为 data URL 比原图大约 1.37 倍，这是历史上「图片加不进去」的根因。
- 应用图标（`kind: app`）：从开始菜单选中 `.lnk` 后，前端把 `M2` 解析出的 `.ico` 交给 canvas 归一化成 **128px PNG**（`shrinkImage(icon, 128)`）再存；后端在把 `.lnk` 复制进 `data/应用/` 时，另存一份 **64px** 的图标（`readShortcut(path, { iconMaxSize: 64 })`）作为兜底。所以同一条目可能先后被写过两种尺寸，以最后一次写入为准。
- 后端上限：`M1` 的 `normalizeIcon()` 截断到 2 MiB；`M3` 的 `update_item` 超限直接报「图片文件过大」。
- 制作流程（`assets/icons/`）：手写 SVG（512 视口）→ 无头 Chrome 截图 → LANCZOS 缩到 128。
  **不要在 Chrome 的 `--window-size` 上做缩放**，行为不可靠（曾渲染出只截一角的图）。
- 占位：条目没有 `icon` 时，前端画**字母头像**（名称首字母 + 由 `id` 哈希得到的固定色相），而不是塞一个容易认错的系统图标。色相表 `AVATAR_HUES` 不依赖皮肤变量，任何皮肤下都保持对比度。

---

## 禁止事项

- ❌ **不要手工编辑 `data/catalog.json`**。改动一律通过 `M1` 或 `M3` 的接口。
- ❌ **不要新增/重命名 `Item` 或 `Category` 的字段而不更新本文件**，也不要在同一改动里只改一端（必须同时检查 `M1`、`M3`、`M4`）。
- ❌ **不要让 `data/` 之外的路径进入索引**。`sourcePath` 必须严格是 `分类/文件名` 两层，且解析后仍在 `data/` 内（`M1` 的 `itemFilePath()`、`M3` 的 `source_join()` 都在做这个校验）。
- ❌ **不要扫描或写入 `密码库`、`备份`**。密码库功能需要先完成加密设计与安全审计（见 [SECURITY.md](../../SECURITY.md)）。
- ❌ **不要把真实数据写进本文档**。示例统一用 `示例脚本.ps1`、`https://example.com`。
- ❌ **不要为了让某一端省事而放宽字段类型**（例如把时间统一改成数字），那会破坏另外两端。

越界时应怎么办：不要直接改。在回复里说明「这属于 `M0` 契约变更，需要同步 `M1`/`M3`/`M4`」并列出具体影响面，等确认。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-20 | 首次编写。记录 `version: 2` 结构、5 个内置分类、`Item` 全字段、状态机、扫描规则、图标规范；标注 `url`/`target` 双写、`order` 共享空间、时间格式不一致三处已知偏差。 |
| 2026-09-20 | 同步三端统一后的语义：两端默认分类改为同一套（含 `应用`/`文档`）；扫描改为按 `catalog.categories` 的 `folder` 动态遍历并按 `kind` 补信息；`url` 降级为只读兼容字段（两端都迁移成 `target`）；无图标时的占位由 `▣`/`↗` 符号改为字母头像；补充应用图标的两处写入尺寸（前端 128px PNG / 后端 64px 兜底）。 |
| 2026-09-21 | `Item` 新增可选字段 `browser`（**仅 `link` 类型使用**）：空串/缺省 = 跟随系统默认浏览器，否则存浏览器 key 或 exe 绝对路径。三端同步：`M1` 的 `/api/items`、`/api/items/update`、`/api/items/metadata` 都接收该字段（`update` 传空即清掉），`/api/items/open` 按它启动并在浏览器缺失时回落；`M3` 对应 IPC 命令同步（空值同样清掉）；`M4` 编辑器在「网址」类型下新增「用哪个浏览器打开」下拉。两端各自新增本机浏览器探测接口（`GET /api/browsers` / `list_browsers`），候选表以 `M1` 的 `BROWSERS` 为准，`M3` 的 `browser_specs()` 与之逐条对应。 |
| 2026-09-21 | 新增内置分类「文件夹」（`key: folder` / `folder: 文件夹` / `kind: file` / `order: 4`），用于「一键跳到本地某个文件夹」。同时给 `M1`/`M3` 的读取逻辑补上**内置分类补齐**——原先只有 `categories` 整段缺失时才回落到默认表，于是**后加的内置分类对已有 `catalog.json` 完全不可见**；现在按 `key`（或同名 `folder`）补齐，已存在则跳过。条目语义：只存 `target` 绝对路径、不写 `sourcePath`，点击由系统资源管理器打开。 |
| 2026-09-21 | **新增「合并包」的用法约定（`M0` 契约本身没有变化）**：一个包 = 一个普通分类（`builtin: false`、`kind` 与成员一致），成员条目的 `category` 指向它；标记借 `symbol: 'pk'` + `note` 前缀「合并包」，`symbol` 恰好 2 个字符、不受两端 `normalizeCategory` 截断影响。包里的**磁盘文件不搬**：`M1`/`M3` 的 `/items/metadata` 对非收件箱条目只改 `category`，所以「应用」包里的 `.lnk` 仍留在 `data/应用/`、`sourcePath` 继续有效、扫描不会重复收录。解散＝成员退回同类内置分类后 `removeFolder: true` 删掉那个空分类目录（见第 2 节的说明）。 |
| 2026-09-21 | `Item` 新增可选字段 `uwpAppId`（仅 `app` 类型中的 Windows 商店应用）：存 AUMID（如 `OpenAI.Codex_xxx!App`），`target` 对应 `shell:AppsFolder\<AUMID>`。`M1`/`M3` 的应用清单扫描现在同时通过 PowerShell `Get-StartApps` 枚举商店应用（`group` 为 `商店应用`），创建时不复制 `.lnk`，启动走 `explorer.exe shell:AppsFolder\<AUMID>`。 |
