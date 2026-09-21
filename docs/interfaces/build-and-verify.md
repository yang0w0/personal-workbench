# 接口文档 · `M5` 构建与验证

## 职责与边界

本模块定义**怎么运行、怎么编译、怎么验证、怎么提交**。它不包含业务逻辑，但所有模块都受它约束——因为「改完怎么让它生效」在不同模块里是不同的。

**允许读写**

| 路径 | 权限 |
| --- | --- |
| `package.json` | ✅ 脚本入口 |
| `start-server.cmd` `build-client.cmd` | ✅ 一键启动/构建 |
| `.github/workflows/check.yml` | ✅ CI 检查 |
| `scripts/check-structure.js` | ✅ 接口文档结构校验 |
| `README.md` `CONTRIBUTING.md` `SECURITY.md` `SUPPORT.md` `docs/*` | ✅ 面向人和 agent 的说明 |
| `个人工作台.exe` | ⚠️ **构建产物**，由 `build-client.cmd` 生成，不要手工替换 |
| 各模块源码 | ❌ 本模块只观察，不改别人 |

---

## 对外接口

### 1. npm 脚本（`package.json`）

| 命令 | 作用 |
| --- | --- |
| `npm start` | 启动本机服务（等价于 `node server/server.js`） |
| `npm run check` | 语法 + 结构校验。**提交前必跑** |
| `npm run desktop:dev` | Tauri 开发模式（改前端即时生效，不产出 exe） |
| `npm run desktop:build` | 编译桌面客户端（产物在 `desktop/src-tauri/target/release/`） |

### 2. 一键脚本

| 文件 | 用途 | 前置条件 |
| --- | --- | --- |
| `start-server.cmd` | 双击启动浏览器版后端，窗口需保持打开 | Node.js 20+ |
| `build-client.cmd` | 双击编译便携客户端：`npm install` → `desktop:build` → 把产物复制成根目录的 `个人工作台.exe` | Node.js 20+、Rust（MSVC 工具链）、C++ Build Tools、WebView2 Runtime |

### 3. 改完怎么生效对照表

这是本模块最重要的内容。**改错生效方式的代价是「明明改了却没反应」。**

| 改了什么 | 浏览器版 | 桌面版 |
| --- | --- | --- |
| `data/` 下的脚本、网址等真实文件 | 点「扫描并同步」 | 点「扫描并同步」 |
| `data/catalog.json` | 刷新页面 | 重开客户端 |
| `server/server.js` | 重启 `start-server.cmd` | 无影响（不用它） |
| `server/shortcut.js` | 重启 `start-server.cmd` | 无影响 |
| `app/index.html` | **刷新浏览器即可** | ⚠️ **必须重新编译**（`build-client.cmd`），因为前端被内嵌进 exe |
| `desktop/src-tauri/**` | 无影响 | ⚠️ 必须重新编译 |
| `docs/**` `*.md` | 无影响 | 无影响 |
| `assets/icons/**` | 无影响（图标是制作素材，成品写在条目 `icon` 字段里） | 无影响 |

**桌面端交付标准：**凡是修改了 `app/index.html`，如果用户要在桌面版看到效果，agent 不能只停在 HTML；必须运行 `build-client.cmd`，把前端重新编译进根目录的 `个人工作台.exe`，并确认该 exe 的更新时间已经变化后才算完成。

增量编译约 1.5 分钟。**构建前必须确认 `个人工作台.exe` 没有在运行**，否则最后一步 `copy` 会失败（`build-client.cmd` 用的是 `copy /Y`）。

### 4. `npm run check` 检查内容

1. `node --check server/server.js` —— 服务端语法。
2. `node scripts/check-structure.js` —— 接口文档结构校验：
   - `docs/INTERFACES.md` 存在；
   - `docs/interfaces/` 下每份文档都包含四个固定二级标题（`## 职责与边界` / `## 对外接口` / `## 禁止事项` / `## 变更记录`）；
   - `docs/INTERFACES.md` 的模块地图里列出的模块，都有对应的接口文档链接且文件存在。

这个检查的意义：把「每个模块必须有接口文档」从口头约定变成**可执行的约束**。

### 5. CI（`.github/workflows/check.yml`）

向 `main` 推送到位或开 PR 时，在 Ubuntu 上跑：校验 `package.json` → `npm run check`。**CI 里不跑任何需要 Windows 的步骤**。

### 6. 验证方式（零安装）

| 场景 | 做法 |
| --- | --- |
| 浏览器侧交互 | 无头 Chrome + CDP（Node 原生 WebSocket）直接跑 `app/index.html`，需先启动服务 |
| 桌面侧交互 | 用 `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223"` 启动 exe，再用 CDP 接管真实窗口做端到端验证 |

沿用「零安装」原则：优先用系统自带 Chrome/Edge，不要为了测试引入 npm 依赖到本仓库。相关经验见用户级 skill `cdp-headless-interaction-verify`。

**验证纪律：**

- 验证交互时**不要真实派发 `click`**——会触达业务处理器产生真实副作用（会真的启动脚本、打开网址、改 `catalog.json`）。
- 若测试改动了 `data/catalog.json`，**结束后必须还原**。

### 7. 提交与发布

- 提交前：`npm run check` + `git status`。
- `data/`、`logs/`、`.workbuddy/`、`node_modules/`、`desktop/src-tauri/target/`、`*.exe` 均已被 `.gitignore` 忽略，**不要用 `-f` 强推**。
- 公开仓库之前确认没有私人网址、脚本内容、备份、密码进入版本库。
- 使用 [.github/pull_request_template.md](../../.github/pull_request_template.md)（含模块边界自查项）。

---

## 禁止事项

- ❌ **不要把 `data/` 或 `logs/` 提交到 Git**，也不要为了让 CI 通过而放宽 `.gitignore`。
- ❌ **不要在 CI 里加需要 Windows 才能跑的步骤**（CI 是 Ubuntu）。
- ❌ **不要删除 `scripts/check-structure.js` 的校验项来「让检查变绿」**。文档缺失就补文档。
- ❌ **不要手工替换或反编译 `个人工作台.exe`**，它必须由 `build-client.cmd` 产出。
- ❌ **不要在没确认 exe 未运行时构建**，会覆盖失败且看起来像「编译没反应」。
- ❌ **不要把测试用的探针脚本、日志、截图留在仓库里**（放 `logs/`，它被忽略）。
- ❌ **不要在验证交互时派发真实 `click`**（见第 6 节）。

越界时应怎么办：如果改动需要新的构建步骤或新依赖，**先在这里写下新步骤与前置条件**，再改 `package.json`，并告知使用者「现在构建需要多做一步 X」。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-21 | 明确桌面端交付标准：改 `app/index.html` 后若要桌面版生效，必须运行 `build-client.cmd` 并确认根目录 `个人工作台.exe` 已更新。 |
| 2026-09-20 | 首次编写。冻结 npm 脚本清单、一键脚本前置条件、「改完怎么生效」对照表、`npm run check` 检查项、验证纪律。 |
