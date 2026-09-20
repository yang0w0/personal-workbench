# 个人工作台

一个本地优先的个人工具集合。界面只有一个可直接双击打开的 HTML 文件；脚本、链接、目录扫描结果和未来的密码库数据都保存在本项目的 `data` 文件夹内，不上传到云端，也不依赖浏览器缓存。

> 隐私优先：`data/` 是本地私人资料区，已被 Git 永久忽略。请勿将密码、令牌、私人网址、备份或真实个人资料放入源码目录。

## 功能概览

- 集中管理本地脚本、常用网址、应用程序（快捷方式）和文档，支持扫描、补充信息、编辑、删除和拖拽排序。
- 支持自定义分类：除内置的「脚本 / 网址 / 应用 / 文档」外，可以自己建分类，并自动对应到 `data/` 下的文件夹。
- 支持从 Windows `.url` 快捷方式扫描网址、从开始菜单与桌面扫描 `.lnk` 应用，并提供“待整理”收件箱。
- 每个条目可设置自定义图标（自动压缩为 128px 级别，保存在索引里，不产生额外文件）。
- 提供浏览器版和 Tauri 桌面客户端；数据始终保留在本机文件夹中。
- 浏览器版本机服务仅监听 `127.0.0.1:8765`，不会对局域网或互联网开放。

## 模块与接口文档（重要）

本项目按**模块**组织，模块之间**只通过接口文档通信**——这是给人和 AI agent 共同的开发规则。

**开工前请先读 [AGENTS.md](AGENTS.md)（agent 第一入口）和 [docs/INTERFACES.md](docs/INTERFACES.md)（接口总纲）：**

- 只读、只改本次任务所属模块的文件；
- 需要用别的模块的能力时，**查接口文档，不要读它的实现代码**；
- 改接口必须同一次更新对应接口文档；
- 必须跨模块改动时，先停下来说明理由。

| 模块 | 代码位置 | 接口文档 |
| --- | --- | --- |
| `M0` 数据契约 | `data/catalog.json`（只能通过接口改） | [data-catalog.md](docs/interfaces/data-catalog.md) |
| `M1` 浏览器后端 | `server/server.js` | [http-api.md](docs/interfaces/http-api.md) |
| `M2` 快捷方式解析 | `server/shortcut.js` | [shortcut-lib.md](docs/interfaces/shortcut-lib.md) |
| `M3` 桌面客户端 | `desktop/src-tauri/` | [tauri-ipc.md](docs/interfaces/tauri-ipc.md) |
| `M4` 前端界面 | `app/index.html` | [app-ui.md](docs/interfaces/app-ui.md) |
| `M5` 构建与验证 | `package.json`、`*.cmd`、`.github/workflows/` | [build-and-verify.md](docs/interfaces/build-and-verify.md) |
| `M6` 图标素材 | `assets/icons/` | 见 [data-catalog.md](docs/interfaces/data-catalog.md) 的「图标」一节 |

接口总纲里还有一份[任务路由表](docs/INTERFACES.md#4-任务路由表)（「我想改 X → 该读哪份文档 → 该改哪些文件」）和[已知契约偏差](docs/INTERFACES.md#6-已知契约偏差必读)，动手前扫一眼能省很多来回。

## 当前项目状态

此仓库已包含可用的本地基础骨架：

- `app/index.html`：单文件桌面界面，HTML、CSS 和 JavaScript 全部内嵌，不加载任何相对路径的静态资源。
- `server/server.js`：零第三方依赖的本机服务，用于扫描文件夹和保存工作台索引数据。
- `server/shortcut.js`：零依赖的 `.lnk` 解析与图标提取。
- `desktop/src-tauri/`：便携式 Windows 客户端（Tauri 2）。
- `data`：实际用户数据目录，默认不提交到 GitHub。
- `start-server.cmd`：一键启动本机服务；`build-client.cmd`：一键构建客户端。

当前可管理脚本、网址、应用和文档。密码库目录与安全规范已预留，但**尚未实现密码库**；在完成客户端加密、审计和测试前，绝不应把真实密码存入此项目。

## 目录结构

```text
D:\个人工作台\
├─ AGENTS.md                   # ⭐ agent 第一入口：模块边界与红线规则
├─ CLAUDE.md                   # 其他 agent 工具的薄壳入口（指向 AGENTS.md）
├─ app/index.html              # ⭐ M4 唯一界面：可直接双击打开的单文件页面
├─ start-server.cmd            # 启动本地服务（浏览器版）
├─ build-client.cmd            # 构建便携式 Windows 客户端
├─ package.json                # Node.js 运行要求、脚本入口与项目元数据
├─ README.md / SECURITY.md / CONTRIBUTING.md / SUPPORT.md / CODE_OF_CONDUCT.md
├─ .gitignore                  # 避免将个人数据提交到 GitHub
├─ .github/                    # CI、Issue/PR 模板、Copilot 指令
├─ .cursor/rules/              # Cursor 规则（薄壳）
├─ data/                       # ⭐ M0 本地个人数据：不上传 GitHub
│  ├─ catalog.json             # 服务自动生成：分类注册表 + 条目
│  ├─ 脚本/                    # 从页面创建或手动放入的小脚本
│  ├─ 网址/                    # Windows .url 快捷方式
│  ├─ 应用/                    # 复制进来的 .lnk 应用快捷方式
│  ├─ 文档/                    # 用默认程序打开的文档
│  ├─ 待整理/                  # 待分类文件；扫描后会在页面提示补充信息
│  ├─ 密码库/                  # 预留，未来仅保存加密密码库文件
│  └─ 备份/                    # 预留，加密备份
├─ server/
│  ├─ server.js                # ⭐ M1 本机 HTTP 服务（127.0.0.1:8765）
│  └─ shortcut.js              # ⭐ M2 .lnk 解析与图标提取（零依赖）
├─ desktop/src-tauri/          # ⭐ M3 Tauri 客户端源码
├─ assets/icons/               # M6 图标制作素材（SVG 源 + 128px 成品）
├─ scripts/check-structure.js  # ⭐ M5 接口文档结构校验（npm run check 的一部分）
└─ docs/
   ├─ INTERFACES.md            # ⭐ 接口总纲：模块地图、铁律、任务路由表、已知偏差
   ├─ interfaces/              # ⭐ 各模块接口文档（6 份）
   │  ├─ data-catalog.md       #   数据契约
   │  ├─ http-api.md           #   浏览器后端 HTTP 接口
   │  ├─ shortcut-lib.md       #   快捷方式解析库
   │  ├─ tauri-ipc.md          #   桌面客户端 IPC 命令
   │  ├─ app-ui.md             #   前端 DOM 与适配层契约
   │  └─ build-and-verify.md   #   构建、验证、发布
   ├─ DATA_FORMAT.md           # 面向使用者的数据格式说明
   └─ CLIENT.md                # 面向使用者的客户端说明
```

> ⭐ 标记的是「改功能时你会碰到的文件」。改之前先确认它属于哪个模块，并打开对应接口文档。

## 如何运行

### 1. 安装一次 Node.js

需要 Node.js 20 或更高版本。它只用来在你的电脑上启动本地服务，不会把数据上传到互联网。

### 2. 启动数据服务

双击 `start-server.cmd`。命令窗口显示下列地址后保持打开：

```text
http://127.0.0.1:8765
```

服务只监听本机回环地址 `127.0.0.1`，其他电脑无法访问。

### 3. 直接打开 HTML

双击 `app/index.html`，任何浏览器都能打开页面。它使用固定完整地址 `http://127.0.0.1:8765/api/...` 连接本机服务，而不是相对路径。

如果服务尚未启动，页面仍会显示界面，但会提示“本地服务未连接”，无法保存或扫描数据。启动服务后点击“重新连接”。

## Windows 客户端（便携式）

客户端源码位于 `desktop/src-tauri`，界面仍使用 `app/index.html`。客户端运行时不读取 `D:\个人工作台` 等固定地址；它会以客户端可执行文件所在位置为起点寻找 `data`，所以复制或移动整个项目文件夹后仍可使用同一套本地数据。

双击 `build-client.cmd` 可在项目根目录生成 `个人工作台.exe`。它是本地构建产物，默认不会提交到 GitHub；完整规则见 [客户端说明](docs/CLIENT.md)。

两点要注意：

1. **界面是被编译进 exe 的**。改了 `app/index.html`（或 `desktop/`）之后，桌面版必须重新构建才生效——`app/index.html` 浏览器里刷新一下就能看到，桌面版不行。构建前先关掉正在运行的 `个人工作台.exe`。
2. **桌面版目前是浏览器版能力的子集**：它还不支持自定义分类、应用/文档分类、开始菜单快捷方式扫描，也暂时不支持在这端编辑网址目标。需要这些能力时请用浏览器版。差异清单见 [tauri-ipc.md](docs/interfaces/tauri-ipc.md)。

## 数据同步方式

```text
手动放入文件 ─┐
页面新增条目 ─┼─> data/分类目录 + data/catalog.json <─> 本地服务 <─> app/index.html
网页点击同步 ─┘                                    （或桌面客户端）
```

- **从网页添加脚本**：填写名称和脚本内容后保存，服务将文件写入 `data/脚本/`，并把名称、标签和说明写入 `data/catalog.json`。
- **从网页添加网址**：网址和描述写入 `data/catalog.json`；后续可扩展为同步生成 `.url` 文件。
- **手动加入文件**：把脚本放进 `data/脚本/`，或把文件先放进 `data/待整理/`；把 Windows 网址快捷方式（`.url`）放进 `data/网址/`。页面点击“扫描并同步”后，会把未登记文件列为“待补充”；点击“补充信息”即可填写名称、描述、标签及分类，待整理文件会被移入对应目录。
- **移动或删除文件**：再次扫描时工作台会更新其状态；不会自动删除索引，以免误删数据。
- **换电脑**：复制整个 `个人工作台` 文件夹；在新电脑安装 Node.js 后启动服务即可。数据跟随文件夹迁移。

> ⚠️ **已知数据风险（尚未修复）**：桌面客户端执行任何写入操作（新建、编辑、删除、排序、扫描）后，`data/catalog.json` 里的分类注册表（`categories` 字段）会被清掉，自定义分类会从界面上消失（磁盘上的文件夹仍在）。在修复前，**请优先使用浏览器版修改数据**；确实要用桌面版时，先复制一份 `data/` 备份。原因见 [tauri-ipc.md](docs/interfaces/tauri-ipc.md) 的「已知偏差」。

浏览器出于安全原因不能在用户双击 HTML 后任意读写硬盘，因此“同步”由仅在本机运行的服务完成。这比把数据放在 `localStorage` 或浏览器缓存中更可靠。

## GitHub 发布规则

可公开提交界面、服务源码、文档和空目录占位文件；默认**不会**提交 `data` 内的个人内容、索引、备份和日志。

发布前务必运行：

```powershell
git status
```

确认没有脚本私密内容、网址、数据库、备份或密码文件被列为待提交。密码库功能完成前，也不要向 `data/密码库` 写入任何真实密码。

## 开发与质量检查

```powershell
npm run check
```

这会检查本机 Node.js 服务语法。GitHub Actions 会在向 `main` 分支推送或创建拉取请求时运行相同检查。

## 参与和反馈

- 使用问题请阅读 [支持说明](SUPPORT.md)。
- 缺陷和功能建议请使用相应的 GitHub Issue 模板。
- 提交改动前请阅读 [贡献指南](CONTRIBUTING.md) 和 [行为准则](CODE_OF_CONDUCT.md)。
- 安全问题请按照 [安全政策](SECURITY.md) 私下报告，不要在公开 Issue 中披露敏感细节。

本项目当前未附带开源许可证；除非仓库所有者另行授予许可，保留所有权利。

## 后续实施顺序

1. 完善工作台的卡片、搜索、标签、编辑和删除交互。
2. 扩充分类扫描规则，并支持更多脚本语言和网址快捷方式。
3. 增加本地 SQLite 数据库替代简单索引 JSON；保留原始脚本文件作为可移植资产。
4. 单独实现经安全审计的密码库：主密码、客户端加密、自动锁定、加密备份和恢复。
5. 在本地 API 稳定后，为移动端增加安全的同步方案；不默认把本地密码数据上传至第三方服务。

## 安全边界

- 本机服务没有账户体系，因为它只绑定 `127.0.0.1`；不得把监听地址改为 `0.0.0.0`。
- 本项目目前不能作为密码管理器使用。
- 同步/备份前自行确认文件夹的磁盘加密、系统登录密码和备份位置安全。
- `data` 是真实个人数据，删除或覆盖前先复制一份到安全位置。

详见 [数据格式说明](docs/DATA_FORMAT.md) 和 [安全政策](SECURITY.md)。
