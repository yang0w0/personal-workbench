# 便携式 Windows 客户端

## 运行方式

Tauri 将 `app/index.html` 内嵌进 `个人工作台.exe`。发布后的客户端不使用固定盘符、用户名或安装目录：每次启动都会从客户端所在文件夹向上寻找同级 `data` 文件夹。

最终可携带目录如下：

```text
个人工作台/
├─ 个人工作台.exe
├─ data/
├─ app/                 # 保留浏览器版 HTML，便于直接打开
├─ start-server.cmd      # 浏览器版需要，本客户端不需要
└─ 其他项目文件
```

因此移动整个 `个人工作台` 文件夹到其他盘符或其他电脑后，客户端仍会操作该文件夹中的 `data`。不要只移动 `.exe` 而遗漏 `data`。

## 构建前置条件

构建 `.exe` 的电脑需要：

1. Node.js 20+。
2. Rust（`rustup` 安装的稳定版 MSVC 工具链）。
3. Microsoft C++ Build Tools，包含 Desktop development with C++ 工作负载。

然后双击根目录的 `build-client.cmd`。它会下载 Tauri 的构建依赖、编译客户端，并将生成的 `个人工作台.exe` 放到项目根目录。

## 使用与兼容性

- 日常使用双击 `个人工作台.exe`；无需启动本机 Node 服务。
- 浏览器版双击 `app/index.html`，仍需启动 `start-server.cmd`。
- Tauri 依赖 Microsoft Edge WebView2。较新的 Windows 通常已具备；若目标电脑缺少它，需要安装微软的 WebView2 Runtime。
- 客户端和浏览器版共用 `data/catalog.json` 与中文分类目录，因此同一时刻只应使用其中一个界面修改数据，避免同时写入索引文件。
