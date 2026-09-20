# 接口文档 · `M4` 前端界面（app/index.html）

## 职责与边界

`app/index.html` 是**唯一的界面**，也是浏览器版与桌面版共用的同一份文件。它自包含：HTML、CSS、JS 全部内嵌，**不加载任何相对路径的静态资源、不引入 CDN**。

文件内部又分成四个**子模块**，本模块的接口文档主要就是固定它们之间的契约：

| 子模块 | 位置 | 职责 |
| --- | --- | --- |
| `M4-CSS` | `<head>` 内的 `<style>` | 全部样式，含拖拽视觉（`.drag-ghost` 等）与各皮肤的专属背景规则 |
| `M4-MAIN` | 第一个 `<script>` 块 | 页面骨架、渲染、表单、上下文菜单、**双通道适配层 `request()`** |
| `M4-SKIN` | 第二个 `<script>` 块（IIFE） | 皮肤注册表、CSS 变量注入、皮肤切换菜单、localStorage 记忆 |
| `M4-DRAG` | 第三个 `<script>` 块（IIFE） | 图标拖拽排序（Pointer Events + FLIP 动画） |

**允许读写**

| 路径 | 权限 |
| --- | --- |
| `app/index.html` | ✅ 本模块唯一文件 |
| `data/catalog.json` | ❌ 只能通过 `request()` 走 `M1`/`M3` 的接口，前端不直接读写磁盘 |
| `server/` `desktop/` | ❌ 属于 `M1`/`M3`。前端要新能力时，先看对方的接口文档，再在 `request()` 里加调用 |

---

## 对外接口

### 1. 双通道适配层（`M4-MAIN` 提供给整个应用的核心契约）

```js
const API = 'http://127.0.0.1:8765/api';
const nativeInvoke = window.__TAURI__?.core?.invoke;   // 存在 ⇒ 桌面通道
async function request(path, options)                   // 唯一的后端调用入口
```

**参数约定**（`options.query` 与 `options.body` 的区别很重要）：

| 方法 | 传参字段 | 举例 |
| --- | --- | --- |
| `GET` | `options.query`（对象，会拼成 query string） | `request('/shortcuts/icon', { query: { path } })` |
| `POST` | `options.body`（**JSON 字符串**，不是对象） | `post('/items', payload)` 内部已封装好 |

`request()` 把两者归一成 `params`，于是：

- **浏览器通道**（`nativeInvoke` 为假）：POST 走 `fetch(url, { method, headers, body })`；GET 把 `params` 逐项 `URLSearchParams.set` 后拼成 `?k=v`（空串/`null`/`undefined` 会被跳过）。
- **桌面通道**（`nativeInvoke` 为真）：查 `IPC_MAP` 把 `"<METHOD> <path>"` 转成 IPC 命令名，统一包 `{ input: params }` 调用；`IPC_NO_INPUT` 里的只读命令直接无参调用。
- **返回**：直接是后端的响应对象（如 `{ items, categories, kinds }`），没有二次包装。
- **错误**：非 2xx 时抛 `Error(d.error || '请求失败')`。

> ⚠️ **历史 bug 提醒**：GET 用 `options.body` 传参会静默丢掉参数（浏览器端不拼 query、桌面端 `input` 为空）。曾经的 `/shortcuts/icon` 404 就是这个原因——`?path=` 根本没发出去。新增 GET 调用一律用 `options.query`。
>
> 任何新增后端调用**必须**经过 `request()`，并且**必须**在 `IPC_MAP` 里补一条记录，否则桌面版会静默失败。契约清单见 [tauri-ipc.md](tauri-ipc.md) 第 4 节。
>
> `IPC_MAP` 目前登记 14 条：`GET /status` `/items` `/categories` `/shortcuts` `/shortcuts/icon`，`POST /scan` `/categories` `/items` `/items/metadata` `/items/update` `/items/delete` `/items/reorder` `/items/open` `/items/hide`。两端都已实现，没有只在一端存在的路径。

### 2. DOM 契约（`M4-MAIN` 与 `M4-DRAG` 之间的接口）

`M4-DRAG` 不认识任何业务数据，它**只依赖下面这组 DOM 约定**。改动这些约定必须同步两个脚本块。

**必须存在的元素 ID**（主脚本直接 `$('...')` 取用，删掉即 `TypeError`）：

| 区域 | ID |
| --- | --- |
| 顶栏 | `connect` `scan` `openAdd` `openNewCategory` `skinToggle` `skinMenu`（后两个归 `M4-SKIN`） `winMin` `winMax` `winClose`（后三个归自绘标题栏，仅桌面端显示） |
| 状态条 | `status` |
| 侧栏 | `nav`（导航按钮由 `renderNav()` 动态生成，**没有写死的分类按钮**） |
| 网格容器 | `frequent` `pending` |
| 内容区 | 各分类的 `.content-section` 由 `render()` 动态生成 |
| 右键菜单 | `contextMenu` |
| 条目编辑器 | `editor` `dialogTitle` `dialogHint` `closeAdd` `cancelAdd` `notice` `save` `kindSwitch`（分段切换类型） |
| 编辑器表单 | `title` `description` `tags` `url` `scriptBody` `extension` `importScript` `appPicked` `pickApp` `manualApp` `appManual` `appPath` `readApp` `filePath` `iconSlot` `iconSlotGlyph` `iconSlotImg` `iconTip` `iconFile` `scriptFile` |
| 待补充归类 | `assignBlock` `assignCategory` `assignNew` `assignNewName` `assignNewSymbol` `assignNewKind` |
| 开始菜单选择器 | `picker` `pickClose` `pickCancel` `pickSearch` `pickGroup` `pickList` `pickCount` `pickDetail` `pickNotice` `pickUse` |
| 新建分类弹窗 | `categoryModal` `categoryClose` `categoryCancel` `categoryName` `categorySymbol` `categoryKinds` `categoryNotice` `categorySave` |

**必须存在的属性约定**：

| 选择器 | 约定 |
| --- | --- |
| `.icon-card` | 一个可打开/可拖拽的图标卡。**必须带 `data-id`**（值 = 条目 `id`），否则排序会提交空 id |
| `.icon-grid` | 图标容器。**`id === 'frequent'` 的网格不参与拖拽**，这是唯一被硬编码排除的容器 |
| `.content-section[data-section]` | 区块，取值 `frequent` / `<分类 key>` / `inbox`，与 `.nav-link[data-view]` 的取值一一对应，由 `applyView()` 控制显隐 |
| `.nav-link[data-view]` | 侧边栏导航，取值 `all` / `frequent` / `<分类 key>` / `inbox` |
| `.kfields[data-kind]` | 编辑器里**按类型分流**的字段组，取值 `script` / `link` / `app` / `file`。`setEditorKind()` 只显示与当前类型匹配的那一组 |
| `.pick-row` / `.pick-detail` | 开始菜单选择器的列表行与右侧详情 |
| `.kind-card` | 分类新建/归类弹窗里的类型选择卡 |

**跨块函数契约**：

| 函数 | 归属 | 被谁调用 | 说明 |
| --- | --- | --- | --- |
| `persistOrder(grid)` | `M4-MAIN` | `M4-DRAG` 的 `endDrag()` | 拖拽结束时把网格内 `data-id` 顺序提交给后端。**`M4-MAIN` 改名/删掉它，拖拽就会在结束时抛错。** 它内部已有 `if (ids.length < 2) return` 保护 |

`M4-DRAG` 不向 `M4-MAIN` 暴露任何东西（纯单向依赖）。

**拖拽期间产生的临时样式类**（由 `M4-CSS` 定义外观）：

| 类名 | 挂在哪 | 作用 |
| --- | --- | --- |
| `.drag-ghost` | 克隆出的幽灵卡，追加到 `document.body` | 跟手浮层 |
| `.drag-source` | 被拖动的原卡 | 原位置半透明 |
| `.dragging-active` | `document.body` | 全局拖拽态（如禁选） |

### 3. 渲染契约（数据 → 界面）

**分类完全数据驱动**：`state.categories` 来自 `/status` 或 `/items` 返回的 `categories`，侧栏（`renderNav()`）与内容区块（`render()`）都据此生成，**没有任何写死的分类**。加一个分类只需后端数据变化，前端不动。

`render()` 的分流规则：

| 目标容器 | 过滤条件 |
| --- | --- |
| `#frequent` | `status === 'complete'`，按「常用度」排序取前 6 |
| 每个分类区块 | `status === 'complete'` 且 `category === <该分类 key>` |
| `#pending` | `status === 'needs_metadata'`，渲染成列表行 |

> `#frequent` **不参与手动排序**（前端明确排除它，见上表 `.icon-grid` 约定）；只有分类网格可以拖。

- 排序：`sortItems()` —— 两端都有 `order` 时按 `order` 升序，否则按 `score()` 降序。
- 常用度：`score(item) = openCount * 12 + max(0, 30 - 距上次打开的天数)`。
- 时间戳：`timestamp()` 同时兼容 ISO 8601 字符串与 Unix 秒数字——**不要「统一」成一种**，因为桌面端与浏览器端写出的格式不同（见 [data-catalog.md](data-catalog.md)）。
- 图标：`paintIcon()` —— 有 `icon` 就用 `<img>` 并挂 `onerror` 回落到 `paintLetter()`；没有图标就画**字母头像**（首字母 + 由 `id` 哈希得到的固定色相），不再按分类显示 `▣`/`↗` 占位符号。

**四个 `kind` 各有各的编辑器**（`setEditorKind()` + `.kfields[data-kind]`），首屏只显示该类型的必填字段：

| kind | 首屏字段 | 补充手段 |
| --- | --- | --- |
| `script` | 脚本内容、文件后缀 | 「导入文件」读取已有脚本 |
| `link` | 网址 | 只填域名会自动补 `https://` |
| `app` | （无输入框）从开始菜单选择 / 手动填路径 | 选择器会带出目标、参数、工作目录与真实图标 |
| `file` | 文件或文件夹路径 | 也可以把文件丢进分类文件夹后点「同步文件夹」 |

「说明」与「标签」统一折叠进 `<details class="more">`，默认收起。

### 4. 交互契约

| 交互 | 实现要点 |
| --- | --- |
| 拖拽排序 | **必须用 Pointer Events，不要换成 HTML5 drag-and-drop**。Tauri 2 在 Windows 下会接管 OS 级拖放使 `draggable="true"` 完全失效（这是历史上「拖不动」的根因） |
| 点击 vs 拖动 | 5px 阈值区分；拖动结束后紧随的 `click` 会被捕获阶段吞掉，避免误触「运行/打开」 |
| 图标上传 | `shrinkImage(file, max=256)`：最长边缩到 256px，PNG 超 400KB 转 JPEG(0.9)。输入文件上限 8 MiB |
| 应用图标 | 选中快捷方式后 `shrinkImage(icon, 128)` 归一化成 128px PNG。**这一步是异步的，且 `usePickedShortcut()` 会先把弹窗关掉再 await**——所以外部（含自动化测试）不能靠「弹窗关了」判断完成，要等 `state.editor.icon` 有值 |
| 右键菜单 | `contextmenu` 事件 + 固定定位，坐标用 `innerWidth/innerHeight` 夹取；菜单项由 `buildContextMenu()` 按条目类型动态生成 |
| 编辑器模式 | `state.editor.mode` 三值：`create`（新建，可切类型）/ `edit`（编辑，类型由分类锁定，隐藏 `#kindSwitch`）/ `assign`（待补充归类，多一个 `#assignBlock` 选择目标分类，可选内联新建分类） |
| 新建分类 | `openCategoryModal()` → `POST /categories`。后端会**真的在 `data/` 下建出文件夹**并写 `.gitkeep`，之后往里丢文件点「同步文件夹」就能收录 |
| 脚本文件导入 | `#scriptFile` 选中后读文本填入 `#scriptBody`，并自动带出文件名与后缀 |

### 5. 与样式的关系（`M4-CSS` + `M4-SKIN`）

样式全部内联在 `<head>` 的 `<style>` 里，**所有颜色/圆角/模糊/阴影都走 CSS 变量**，由 `M4-SKIN` 在运行时注入。基础变量：`--ink` `--muted` `--faint` `--line` `--line-strong` `--paper` `--paper-solid` `--paper-hover` `--wash` `--brand` `--brandwash` `--brand-ink` `--brand-line` `--on-brand` `--soft` `--soft-ink` `--ok` `--ok-line` `--ok-bg` `--danger` `--scrim` `--blur` `--panel-border` `--panel-shadow` `--bar-bg` `--bar-border` `--bar-shadow` `--scroll-thumb` `--scroll-thumb-hover` `--scroll-track` `--radius*` `--sh-raised` `--sh-float`。`:root` 里是「经典云白」的兜底值；`--bar-*` 三个变量专门控制顶栏（玻璃皮肤下是带色调的磨砂条），`--scroll-*` 控制滚动条（`::-webkit-scrollbar` 细胶囊滑块 + `scrollbar-color` 兜底，Windows 默认那条浅灰滚动槽和玻璃皮肤不搭）。

**主题不再是固定浅色**——皮肤可以是深色（暗夜玻璃）；组件规则里禁止硬编码色值，拖拽相关的三个类名同样走变量，任意皮肤下都要保持对比度。

### 5.1 自绘标题栏（桌面端专用）

桌面端窗口**没有系统边框**（`tauri.conf.json` 的 `decorations:false`，见 [tauri-ipc.md](tauri-ipc.md) 第 5 节），窗口外壳由前端承担：

- 拖动窗口：顶栏的 `.brand`、`.drag-space` 带 `data-tauri-drag-region`（这两个元素**不能包住任何按钮**，否则点击会触发拖动）。
- 窗口控制：`#winMin` / `#winMax` / `#winClose` 走 `window.__TAURI__.window`（`minimize` / `toggleMaximize` / `close`），权限在 `desktop/src-tauri/capabilities/default.json` 里声明。
- 显示条件：`M4-MAIN` 检测到 `nativeInvoke` 时给 `body` 加 `is-desktop` 类，`.win-buttons` 才显示；浏览器版永远隐藏。
- 顶栏配色走 `--bar-bg` / `--bar-border` / `--bar-shadow`，三种皮肤各自定义。

### 5.2 内联样式与 CSP（桌面端硬约束）

桌面端由 Tauri 注入 CSP，且**会往 `style-src` 里加 nonce**。按 CSP 规范，一旦来源列表里出现 nonce/哈希，`'unsafe-inline'` 就被忽略，于是：

| 写法 | 桌面端结果 |
| --- | --- |
| `el.style.background = '...'`（CSSOM 写入） | ✅ 允许 |
| 静态 `<style>` 块里的规则 | ✅ 允许 |
| HTML 里的 `style="..."` 属性（含 `insertAdjacentHTML` 注入的） | ❌ **被拦截**，样式不生效并在控制台报 `Applying inline style violates ... style-src` |
| 运行时 `document.createElement('style')` 注入规则 | ❌ **被拦截** |

因此：

- **禁止在标记或注入的 HTML 里写 `style="..."`**，需要变色/间距就加 CSS 类（约定用 `.bad` 这类修饰符，见 `M4-CSS` 里的 `.tip.bad` / `.notice.bad`）。历史上 `#assignBlock` 与开始菜单的「读取失败」提示都用过内联 `style`，在桌面端静默失效，已改成类名。
- **需要动态样式就写 `el.style.xxx`（CSSOM）或用 CSS 变量**，例如 `paintLetter()` / `colorSwatch()` 都是直接赋值 `style.background`。
- 皮肤要加专属规则，写进静态 `<style>` 里（如 `body[data-skin="xxx"]::before`），不要运行时注入 `<style>`。

---

## 禁止事项

- ❌ **不要把单文件拆成多文件**，也不要引入 CDN、外部字体或外部图标库。`app/index.html` 必须能双击直接打开。
- ❌ **不要在组件样式里硬编码色值**，配色一律走 CSS 变量（见第 5 节）；新皮肤优先只写 `vars` 覆盖，不要用 `!important` 到处覆盖组件规则。
- ❌ **不要在 HTML 标记或 `insertAdjacentHTML` 注入的字符串里写 `style="..."`**，桌面端 CSP 会拦掉（见 5.2 节）；也不要运行时 `createElement('style')`。动态样式走 `el.style.xxx` 或 CSS 变量。
- ❌ **不要把 GET 参数塞进 `options.body`**，要用 `options.query`；否则浏览器端不拼 query、桌面端 `input` 为空，会静默失效或 404（见第 1 节）。
- ❌ **不要用 HTML5 drag-and-drop 重写排序**（见上表，会失效）。
- ❌ **不要绕过 `request()` 直接用 `fetch` 或 `invoke`**，那会破坏双通道兼容。
- ❌ **不要在 `M4-DRAG` 里读取业务数据或调用后端接口**。它只处理 DOM 与调用 `persistOrder()`。
- ❌ **不要改 `.icon-card` / `data-id` / `.icon-grid#frequent` 这三个约定**而不改另一个脚本块。
- ❌ **不要在前端直接读写 `data/`**（浏览器环境本来就做不到，桌面端也别绕过 IPC）。
- ❌ **改完记得区分生效方式**：浏览器版刷新即可；**桌面版必须重新编译 exe**（`build-client.cmd`）。

越界时应怎么办：前端需要新数据时，不要自己去 `data/` 找，而是先看 [http-api.md](http-api.md) / [tauri-ipc.md](tauri-ipc.md) 找到现成接口；如果确实没有，就当作**接口新增**处理（先写文档、再改后端、最后改前端），并说明影响到了哪个模块。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-20 | 首次编写。冻结 `request()` 双通道适配层、DOM 契约（28 个 ID + 4 组属性约定 + `persistOrder` 跨块函数）、渲染分流规则、拖拽实现方式与三条样式类约定。 |
| 2026-09-20 | 新增 `M4-SKIN` 皮肤子模块（第三个 script 块之前为 `M4-DRAG`，现为：MAIN → SKIN → DRAG）：`SKINS` 注册表 + CSS 变量注入 + `localStorage`（键 `pwb.skin`）+ 顶栏切换菜单（`skinToggle`/`skinMenu`）。CSS 全面变量化（新增 `--line-strong` `--paper-solid` `--paper-hover` `--brand-ink` `--brand-line` `--on-brand` `--soft` `--soft-ink` `--ok` `--ok-line` `--ok-bg` `--scrim` `--blur` `--panel-border` `--panel-shadow` `--radius*`），内置皮肤：经典云白（兜底）/ 液态玻璃·暖阳（默认）/ 暗夜玻璃。布局调整：顶栏改吸顶玻璃条、侧栏顶部加品牌标、状态条改胶囊、导航激活态改实心填充。`M4-MAIN`/`M4-DRAG` 脚本块未改动。 |
| 2026-09-20 | 顶栏跟随皮肤：新增 `--bar-bg` / `--bar-border` / `--bar-shadow` 三个变量（`:root` 白色兜底，玻璃皮肤为带色调的磨砂渐变条），顶栏 backdrop-filter 饱和度提到 1.6，玻璃皮肤背景光斑增加顶部一枚（让吸顶条后有颜色可透）。配合 M3 的无边框窗口（`decorations:false`）新增自绘标题栏：`.brand`/`.drag-space` 承担 `data-tauri-drag-region` 拖拽，`#winMin`/`#winMax`/`#winClose` 三个窗口按钮仅 `body.is-desktop` 显示，窗口控制由 `M4-MAIN` 尾部的 `window.__TAURI__.window` 接线。详见 5.1 节。 |
| 2026-09-20 | 滚动条跟随皮肤：新增 `--scroll-thumb` / `--scroll-thumb-hover` / `--scroll-track` 三个变量，`::-webkit-scrollbar` 改为 12px 细胶囊滑块（`background-clip:padding-box` 内缩），并留 `scrollbar-color`/`scrollbar-width` 兜底；弹窗、开始菜单选择器等内部滚动区自动继承。 |
| 2026-09-20 | **界面改为完全数据驱动 + 编辑器按类型分流**（只重写 `M4-CSS` 与 `M4-MAIN`，`M4-SKIN`/`M4-DRAG` 两块原样保留）：侧栏与内容区块由 `state.categories` 生成，去掉写死的 `#scripts`/`#links`；编辑器改用 `#kindSwitch` 分段切换 + `.kfields[data-kind]` 四组字段，首屏只留必填项，「说明/标签」折叠进 `<details>`；新增开始菜单选择器（`#picker`，走 `/shortcuts` + `/shortcuts/icon`）、新建分类弹窗（`#categoryModal`）、待补充归类（`#assignBlock`，可内联新建分类）；`request()` 明确 GET 用 `options.query`、POST 用 `options.body`，`IPC_MAP` 扩到 14 条；图标占位由 `▣`/`↗` 符号改为字母头像（`paintLetter()`）。修复两处桌面端 CSP 拦掉的内联 `style` 属性，新增 5.2 节。 |
