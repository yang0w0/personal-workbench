# Copilot 项目指令

本仓库按模块组织，模块之间只通过接口文档通信。**完整规则见 [AGENTS.md](../AGENTS.md)，接口契约见 [docs/INTERFACES.md](../docs/INTERFACES.md)。**

## 修改代码前

1. 先查 [docs/INTERFACES.md](../docs/INTERFACES.md) 的「任务路由表」，确认本次改动属于哪个模块。
2. **只改该模块的文件**，不要顺手重构其他模块；需要跨模块时先说明理由。
3. 需要用其他模块的能力时，读它的接口文档（`docs/interfaces/`），不要读它的实现。

## 模块与文件

| 模块 | 文件 | 接口文档 |
| --- | --- | --- |
| 浏览器后端 | `server/server.js` | `docs/interfaces/http-api.md` |
| 快捷方式解析 | `server/shortcut.js` | `docs/interfaces/shortcut-lib.md` |
| 桌面客户端 | `desktop/src-tauri/**` | `docs/interfaces/tauri-ipc.md` |
| 前端界面 | `app/index.html` | `docs/interfaces/app-ui.md` |
| 数据契约 | `data/catalog.json` | `docs/interfaces/data-catalog.md` |
| 构建验证 | `package.json`、`*.cmd` | `docs/interfaces/build-and-verify.md` |

## 硬约束

- 不改 `127.0.0.1` 监听地址；不给 `server/` 引入 npm 依赖；前端不引 CDN。
- 不使用 HTML5 drag-and-drop（Tauri 2 下会失效），排序用 Pointer Events。
- 不提交 `data/`、`logs/`；不改动 `data/密码库`、`data/备份`。
- 改了 `app/index.html` 后，桌面版需重新编译才生效。
- 新增/修改接口时同步更新对应接口文档。
