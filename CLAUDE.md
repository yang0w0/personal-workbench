# CLAUDE.md

本仓库的 agent 规则统一写在 [AGENTS.md](AGENTS.md)，接口契约在 [docs/INTERFACES.md](docs/INTERFACES.md)。

@AGENTS.md

## 最小硬规则（细节见 AGENTS.md）

1. **只读、只改本次任务所属模块的文件**，不要通读全仓库。
2. **跨模块能力只查接口文档**（`docs/interfaces/`），不读对方源码。
3. 改接口必须**同一次**更新对应接口文档。
4. 需要改别的模块时**先停下来问**，不要默默跨模块改。
5. 不动 `data/` 里的真实数据；不引 npm 依赖；不改 `127.0.0.1` 监听。
6. 改了 `app/index.html`，桌面版要重新编译（`build-client.cmd`）才生效。
