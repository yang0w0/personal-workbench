# AGENTS.md · 给 AI agent 的项目说明

> 任何 agent（WorkBuddy / Codex / Claude Code / Copilot / Cursor 等）在本仓库动手前**先读这一页**。
> 人也可以读，规则是同一套。

---

## 一句话

**个人工作台**：一个本地优先的个人工具集合。三端共用一份落在 `data/catalog.json` 的索引——浏览器版后端（`server/`）、桌面客户端（`desktop/`）、前端界面（`app/index.html`）。

---

## 第一条规则：只做你被指派的那个模块

**这个项目按模块组织，模块之间只通过接口文档通信。**

1. **只读、只改本次任务所属模块的文件。** 不要为了「了解上下文」把整个仓库读一遍。
2. **需要用别的模块的能力时，查接口文档，不要读它的源码。** 入口是 [docs/INTERFACES.md](docs/INTERFACES.md)（含任务路由表）和 [docs/interfaces/](docs/interfaces/)。
3. **要改接口 = 同一次改动里更新对应接口文档。** 文档与代码不一致时以「先核对接口文档」为准。
4. **越界即停。** 如果必须改到别的模块，**先停下来说明**「这需要同时改 Mx，理由是……」，等确认再动手。发现别人的 bug 只记录、不顺手修。
5. **接口文档里不写真实数据**（网址、脚本内容、个人路径、密码），示例统一用 `example.com`。

模块速查（详情见 [docs/INTERFACES.md](docs/INTERFACES.md) 第 2 节）：

| 模块 | 该改的文件 | 接口文档 |
| --- | --- | --- |
| `M0` 数据契约 | `data/catalog.json`（**只能通过接口改**） | [data-catalog.md](docs/interfaces/data-catalog.md) |
| `M1` 浏览器后端 | `server/server.js` | [http-api.md](docs/interfaces/http-api.md) |
| `M2` 快捷方式解析 | `server/shortcut.js` | [shortcut-lib.md](docs/interfaces/shortcut-lib.md) |
| `M3` 桌面客户端 | `desktop/src-tauri/**` | [tauri-ipc.md](docs/interfaces/tauri-ipc.md) |
| `M4` 前端界面 | `app/index.html` | [app-ui.md](docs/interfaces/app-ui.md) |
| `M5` 构建与验证 | `package.json`、`*.cmd`、`.github/workflows/` | [build-and-verify.md](docs/interfaces/build-and-verify.md) |
| `M6` 图标素材 | `assets/icons/` | [data-catalog.md](docs/interfaces/data-catalog.md) 的「图标」一节 |

---

## 开工流程

1. **定位模块**：查 [docs/INTERFACES.md](docs/INTERFACES.md) 的[任务路由表](docs/INTERFACES.md#4-任务路由表)，确定「本次任务属于哪个模块 / 该读哪份文档 / 该改哪些文件」。
2. **读那一份接口文档**（不是模块的代码）。
3. **改本模块文件**；需要跨模块时回到规则 4。
4. **跑验证**：`npm run check`，然后按[改完怎么生效对照表](docs/interfaces/build-and-verify.md#3-改完怎么生效对照表)确认在哪一端生效。
5. **同步文档**：接口/字段/路径有变动就更新接口文档（含「变更记录」）。

---

## 高危红线（不遵守会出事）

| 红线 | 说明 |
| --- | --- |
| ❌ 不要动 `data/` 里的真实个人数据 | 里面有私人脚本与网址。测试若改动了 `data/catalog.json`，**结束后必须还原** |
| ❌ 不要把 `data/`、`logs/` 提交到 Git | 已被 `.gitignore` 忽略，不要 `-f` 强推 |
| ❌ 不要改动 `data/密码库`、`data/备份` | 密码库尚未实现加密，相关目录必须保持为空 |
| ❌ 不要把本机服务监听从 `127.0.0.1` 改到别处 | 它没有账号体系，安全完全依赖只绑回环 |
| ❌ 不要引入 npm 依赖到 `server/` | 后端卖点是零依赖；前端也不许引 CDN/外部资源 |
| ❌ 不要用 HTML5 drag-and-drop 重写排序 | Tauri 2 在 Windows 会接管 OS 拖放使其失效，现用 Pointer Events |
| ❌ 不要真实派发 `click` 去验证交互 | 会真的启动脚本、打开网址、改 `catalog.json` |
| ❌ 不要手工编辑 `data/catalog.json` | 只能通过 `M1`/`M3` 的接口写 |

---

## 本机环境注意事项（Windows）

- **改了 `app/index.html`，桌面版不会自动生效**——前端被内嵌进 `个人工作台.exe`，必须重新编译（双击 `build-client.cmd`，增量约 1.5 分钟）。编译前确认 exe 没在运行。
- **改了 `data/脚本/` 下的自写脚本不需要重新编译**（与上一条别搞混）。
- Bash 工具如果报 `dirname: command not found`，在命令前加 `PATH="/usr/bin:/bin:$PATH"; export PATH;` 即可恢复。
- 命令行里出现中文路径容易整体失败；把逻辑写进临时脚本、用相对路径调用、结果 `Out-File` 到日志再读，更稳。
- 谨慎使用 `个人工作台.exe`、`logs/` 之外的二进制产物：`个人工作台.exe` 是构建产物，不要手工替换。

---

## 常用命令

```bash
npm start                 # 启动浏览器版后端（127.0.0.1:8765）
npm run check             # 语法 + 接口文档结构校验（提交前必跑）
npm run desktop:dev       # Tauri 开发模式（改前端即时生效）
npm run desktop:build     # 编译桌面客户端
node server/shortcut.js "C:\path\to\app.lnk"   # M2 自测
```

---

## 文档索引

| 文档 | 用途 |
| --- | --- |
| [docs/INTERFACES.md](docs/INTERFACES.md) | **接口总纲**：模块地图、铁律、任务路由表、已知契约偏差 |
| [docs/interfaces/](docs/interfaces/) | 各模块接口文档（每份含「职责与边界 / 对外接口 / 禁止事项 / 变更记录」） |
| [README.md](README.md) | 面向使用者的项目说明 |
| [docs/DATA_FORMAT.md](docs/DATA_FORMAT.md) | 面向使用者的数据格式说明 |
| [docs/CLIENT.md](docs/CLIENT.md) | 面向使用者的桌面客户端说明 |
| [CONTRIBUTING.md](CONTRIBUTING.md) | 贡献流程 |
| [SECURITY.md](SECURITY.md) | 安全边界与密码库上线要求 |

---

## 已知契约偏差（动手前扫一眼）

以下偏差**已被记录**，遇到相关任务时以接口文档为准，不要顺手改：

1. 🟡 **时间戳格式两端不一致**：`M1` 写 ISO 8601，`M3` 写 Unix 秒字符串。前端两种都兼容，但这是目前唯一未收敛的字段级偏差。要做统一属于 `M0` 变更，先确认格式再动手。
2. 🟡 **`order` 是全体条目共用的一维空间**：`reorder` 只更新传入 `ids` 里的条目（不再整段覆盖），但不同分类可能撞号；同视图内撞号由**原始下标**兜底（不再用 `score()`——那会让点击改变位置）。

以下偏差**已经修复**，留档以免有人照着旧描述去「修」不存在的问题：桌面端丢 `categories`、两端 `url`/`target` 字段名不一致、桌面端丢快捷方式属性、桌面端不认识「应用/文档」、桌面端无分类管理与快捷方式扫描、`shortcut.rs` 未接线、前端只渲染 `script`/`link`。

完整说明见 [docs/INTERFACES.md 第 6 节](docs/INTERFACES.md#6-已知契约偏差必读)。

---

## 维护本文件

本文件是 agent 的第一入口，属于「仓库级契约」。修改它时同时检查：
[CLAUDE.md](CLAUDE.md)、[.github/copilot-instructions.md](.github/copilot-instructions.md)、[.cursor/rules/](.cursor/rules/) —— 它们是薄壳，只应指向本文件与 [docs/INTERFACES.md](docs/INTERFACES.md)，**不要在薄壳里复制大段规则**（避免多处漂移）。
