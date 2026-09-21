# 接口文档 · `M4` 前端界面（app/index.html）

## 职责与边界

`app/index.html` 是**唯一的界面**，也是浏览器版与桌面版共用的同一份文件。它自包含：HTML、CSS、JS 全部内嵌，**不加载任何相对路径的静态资源、不引入 CDN**。

文件内部又分成五个**子模块**，本模块的接口文档主要就是固定它们之间的契约：

| 子模块 | 位置 | 职责 |
| --- | --- | --- |
| `M4-CSS` | `<head>` 内的 `<style>` | 全部样式，含拖拽视觉（`.drag-ghost` 等）、各皮肤专属背景规则、多版图标风格与两版顶栏的规则 |
| `M4-MAIN` | 第一个 `<script>` 块 | 页面骨架、渲染、表单、上下文菜单、**内置图标集 `ICONS`**、**双通道适配层 `request()`** |
| `M4-SKIN` | 第二个 `<script>` 块（IIFE） | 皮肤注册表、CSS 变量注入、皮肤切换菜单、localStorage 记忆 |
| `M4-DRAG` | 第三个 `<script>` 块（IIFE） | 图标拖拽排序（Pointer Events + FLIP 动画） |
| `M4-PREF` | 第四个 `<script>` 块（IIFE） | 界面偏好（侧栏/顶栏是否收文字、顶栏两版、图标两版）、设置面板、悬停文字提示 |

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
> `IPC_MAP` 目前登记 17 条：`GET /status` `/items` `/categories` `/shortcuts` `/shortcuts/icon` `/browsers`，`POST /scan` `/categories` `/categories/update` `/categories/delete` `/items` `/items/metadata` `/items/update` `/items/delete` `/items/reorder` `/items/open` `/items/hide`。两端都已实现，没有只在一端存在的路径。

### 2. DOM 契约（`M4-MAIN` 与 `M4-DRAG` 之间的接口）

`M4-DRAG` 不认识任何业务数据，它**只依赖下面这组 DOM 约定**。改动这些约定必须同步两个脚本块。

**必须存在的元素 ID**（主脚本直接 `$('...')` 取用，删掉即 `TypeError`）：

| 区域 | ID |
| --- | --- |
| 顶栏 | `barActions`（按钮组容器）、`connect` `scan` `openAdd` `openSettings` `skinToggle` `skinMenu`（后两个归 `M4-SKIN`） `winMin` `winMax` `winClose`（后三个归自绘标题栏，仅桌面端显示）。按钮一律是 `.bar-btn`，图标由 `data-icon` 声明 |
| 设置面板（`M4-PREF`） | `settingsModal` `settingsTitle` `settingsClose` `settingsDone` `settingsNotice`、皮肤网格 `skinGrid` + `settingsCustomSkin`、顶栏图标 `settingsBrandPick` `settingsBrandReset` `settingsBrandNotice`、侧边栏版式/开关 `navStyleSeg` `navModeSeg` `navModeTip`、顶栏开关 `barLabelSeg` `barStyleSeg`、图标风格开关 `iconStyleSeg` + 预览 `iconDemo`、数据区 `settingsStatus` `settingsScan` `settingsConnect`。另有 `[data-action="settings"]`（顶栏按钮与侧栏底部入口）与 `[data-action="toggle-nav"]`（侧栏底部折叠开关）按选择器绑定，不是 ID |
| 自定义外观面板（`M4-SKIN`） | `skinCustom` `skinCustomTitle` `skinCustomClose` `skinPreview` `skinPreviewTip` `skinPickImage` `skinClearImage` `skinImage` `skinBase` `skinBaseTip` `skinImageAlpha` `skinImageAlphaOut` `skinVeil` `skinVeilOut` `skinImageBlur` `skinImageBlurOut` `skinPanelAlpha` `skinPanelAlphaOut` `skinPanelBlur` `skinPanelBlurOut` `skinAccent` `skinAccentOut` `skinNotice` `skinReset` `skinDone`；另有 `skinCustomOpen`（菜单里「✎ 自定义外观…」入口）由 `M4-SKIN` 运行时生成，不在静态标记里 |
| 侧栏 | `nav`（导航按钮由 `renderNav()` 动态生成，**没有写死的分类按钮**） |
| 网格容器 | `frequent` `pending` |
| 内容区 | 各分类的 `.content-section` 由 `render()` 动态生成 |
| 右键菜单 | `contextMenu` |
| 条目编辑器 | `editor` `dialogTitle` `dialogHint` `closeAdd` `cancelAdd` `notice` `save` `kindSwitch`（分段切换类型） |
| 编辑器表单 | `title` `description` `tags` `url` `scriptBody` `extension` `importScript` `appPicked` `pickApp` `manualApp` `appManual` `appPath` `readApp` `filePath` `iconSlot` `iconSlotGlyph` `iconSlotImg` `iconTip` `iconFile` `scriptFile`、品牌头像 `brandFile` |
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
| `.nav-link.category-link[data-category-key]` | 由数据驱动分类生成的侧栏导航；`M4-DRAG` 只允许它们拖动，`data-category-key` 是 `Category.key`。总览、常用、待补充没有此类，位置固定 |
| `.kfields[data-kind]` | 编辑器里**按类型分流**的字段组，取值 `script` / `link` / `app` / `file`。`setEditorKind()` 只显示与当前类型匹配的那一组 |
| `.pick-row` / `.pick-detail` | 开始菜单选择器的列表行与右侧详情 |
| `.kind-card` | 分类新建/归类弹窗里的类型选择卡 |
| `.icon-pick` | 图标选择器（新建分类、归类时给分类挑图标）。选中的图标名放在容器的 `dataset.value` 上，内部按钮带 `data-icon-name` |
| `.nav-icon[data-icon-name]` | 侧栏图标容器；属性值 = 实际用的图标名，便于调试与自动化验证 |
| `[data-icon]` | 声明图标的宿主元素（`.bar-btn` 内部的 `.bar-ic`、侧栏底部按钮内部的 `.nav-icon`、窗口按钮自身）。启动时由 `hydrateIcons()` 把 `iconSvg()` 的结果填进去 |
| `[data-tip]` | 悬停文字提示的文本来源；只有「文字已被收起」时才由 `M4-PREF` 浮出来 |
| `[data-action="settings"]` / `[data-action="toggle-nav"]` | 打开设置面板 / 收起展开侧栏文字，`M4-PREF` 按选择器绑定（顶栏与侧栏底部各有一处） |
| `body[data-icon-style]` | 图标风格：`plain`（极简线稿，默认）/ `tile`（圆角方块垫底）/ `abstract`（抽象高级符号）/ `readable`（直观重绘图标）。后两者会切换 SVG 路径本体，不只是改容器外观 |
| `body[data-nav-style]` | 侧边栏版式：`card`（信息卡片，默认）/ `pill`（柔和胶囊）/ `rail`（悬浮导轨）；仅改变外观，可与紧凑态组合 |
| `body[data-bar-style]` | 顶栏样式：`circle`（各自独立圆形，默认）/ `capsule`（收进一枚胶囊） |
| `body.nav-compact` | 侧栏只显示图标（宽度 `--nav-w` 收到 68px） |
| `body.bar-labels` | 顶栏按钮显示文字（默认不带，只显示图标） |

**跨块函数契约**：

| 函数 | 归属 | 被谁调用 | 说明 |
| --- | --- | --- | --- |
| `persistOrder(grid)` | `M4-MAIN` | `M4-DRAG` 的 `endDrag()` | 拖拽结束时把网格内 `data-id` 顺序提交给后端。**`M4-MAIN` 改名/删掉它，拖拽就会在结束时抛错。** 它内部已有 `if (ids.length < 2) return` 保护 |
| `persistCategoryOrder(nav)` | `M4-MAIN` | `M4-DRAG` 的 `endNavDrag()` | 读取 `.category-link` 顺序并按序更新 `Category.order`；请求必须串行，避免并发写 `catalog.json` 相互覆盖 |
| `iconSvg(name)` / `iconInto(box,name)` | `M4-MAIN` | `M4-PREF`（图标风格预览） | 生成 / 填充一枚 SVG 图标。**名字取不到时回落 `spark`，不会抛错** |
| `window.PWB_ICONS.refresh()` | `M4-MAIN` | `M4-PREF` | 图标风格从 `plain/tile` 切到 `abstract/readable` 时，重新填充顶栏、侧栏、图标选择器和预览里的 SVG 路径 |
| `window.PWB_APP` | `M4-MAIN` 末尾暴露 | `M4-PREF` | `{ connect, scanFolders }`：设置面板里的「重新连接 / 同步文件夹」调它 |
| `window.PWB_SKIN` | `M4-SKIN` 末尾暴露 | `M4-PREF` | `{ list(), apply(id), openCustom() }`：设置面板列皮肤、换皮肤、开自定义外观面板 |
| `window.PWB_BRAND` | `M4-MAIN` 末尾暴露（品牌标初始化之后） | `M4-PREF` | `{ pick(), reset() }`：设置面板里的「上传头像 / Logo」与「恢复默认图标」调它 |
| `pwb:skin` 事件 | `M4-SKIN` 在 `apply()` 里广播（`CustomEvent`，`detail.id`） | `M4-PREF` 监听 | 皮肤在别处（顶栏菜单 / 自定义面板）被换掉时，同步设置面板的选中态，两块互不直接依赖 |

`M4-DRAG` 不向任何人暴露东西（只调用 `persistOrder`，纯单向依赖）。`M4-PREF` 也是只出不进：它读 DOM 上的 `data-*` 与上面两个全局入口，其它子模块不需要认识它。

**拖拽期间产生的临时样式类**（由 `M4-CSS` 定义外观）：

| 类名 | 挂在哪 | 作用 |
| --- | --- | --- |
| `.drag-ghost` | 克隆出的幽灵卡，追加到 `document.body` | 跟手浮层 |
| `.drag-source` | 被拖动的原卡 | 原位置半透明 |
| `.dragging-active` | `document.body` | 全局拖拽态（如禁选） |
| `.dragging-category` | `document.body` | 侧栏分类拖拽态；同样使用 `.drag-ghost` / `.drag-source`，但只作用于 `.category-link` |

### 3. 渲染契约（数据 → 界面）

**分类完全数据驱动**：`state.categories` 来自 `/status` 或 `/items` 返回的 `categories`，侧栏（`renderNav()`）与内容区块（`render()`）都据此生成，**没有任何写死的分类**。加一个分类只需后端数据变化，前端不动。

`render()` 的分流规则：

| 目标容器 | 过滤条件 |
| --- | --- |
| `#frequent` | `status === 'complete'`，按「常用度」排序取前 8 |
| 每个分类区块 | `status === 'complete'` 且 `category === <该分类 key>` |
| `#pending` | `status === 'needs_metadata'`，渲染成列表行 |

> `#frequent` **不参与手动排序**（前端明确排除它，见上表 `.icon-grid` 约定）；只有分类网格可以拖。
> 分类网格的顺序**只由拖动改写**：点击/打开条目只会更新 `openCount` 与 `lastOpenedAt`，不会改变它的位置。
> 侧栏里的分类同样可以拖动调整相对位置。前端按拖后的 `.category-link` 顺序依次更新每个 `Category.order`，因此刷新、浏览器版与桌面版都会保持一致；「全部内容」「常用」「待补充」不属于分类，位置固定。

- 排序：`sortItems()` —— **位置只由 `order` 决定**：有 `order` 的按 `order` 升序，没有 `order` 的（从没拖过序的分类）按后端返回的原始顺序跟在后面；缺 `order` 或 `order` 撞号时用原始下标兜底，保证结果稳定可复现。**排序键里不允许出现任何会随点击/时间变化的量**（历史上缺 `order` 时用 `score()` 兜底，导致点一下图标、它的分数涨上去就自己跳到第一位）。
- 常用度：`score(item) = openCount * 12 + max(0, 30 - 距上次打开的天数)`。**只有 `#frequent` 用它排序，分类网格不用**——分类网格的位置只由拖动决定。
- 时间戳：`timestamp()` 同时兼容 ISO 8601 字符串与 Unix 秒数字——**不要「统一」成一种**，因为桌面端与浏览器端写出的格式不同（见 [data-catalog.md](data-catalog.md)）。
- 图标：`paintIcon()` —— 有 `icon` 就用 `<img>` 并挂 `onerror` 回落到 `paintLetter()`；没有图标就画**字母头像**（首字母 + 由 `id` 哈希得到的固定色相），不再按分类显示 `▣`/`↗` 占位符号。
- 分类图标：`categoryIcon(category)` 是四段回落 —— ① `symbol` 本身就是已知图标名（如 `spark`）→ 用它；② `category.icon` 是图标名 → 用它；③ 按 `label` 关键词查 `LABEL_ICONS`；④ 回落到 `KIND_ICONS[category.kind]`。
  **内置分类走的是第 ③ 段**（它们的 `symbol` 是 `▣`/`↗` 这类旧字符，`hasIcon()` 不认，`categoryIcon()` 上方那句注释「旧数据里 ▤ / ◆ 这类字符符号不再直接显示」说的就是这件事）。
  所以内置的「文件夹」分类靠 `LABEL_ICONS` 里**最前面**那条 `/文件夹|目录/ → folder` 取图标——**这条必须排在该表的 `/文档|资料|笔记|文件/` 之前**，否则「文件夹」会被「文件」二字抢先匹配成 `file` 图标，在侧栏里跟「文档」分类长得一模一样。
  给某个分类单独配图标 = 往 `ICONS` 加一条 24×24 的描边图形（`fill:none` + `stroke:currentColor`）+ 往 `LABEL_ICONS` 加一条关键词匹配，**渲染代码不用动**；`LABEL_ICONS` 是按顺序取第一个命中的。

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

样式全部内联在 `<head>` 的 `<style>` 里，**所有颜色/圆角/模糊/阴影都走 CSS 变量**，由 `M4-SKIN` 在运行时注入。基础变量：`--ink` `--muted` `--faint` `--line` `--line-strong` `--paper` `--paper-solid` `--paper-hover` `--wash` `--brand` `--brandwash` `--brand-ink` `--brand-line` `--on-brand` `--soft` `--soft-ink` `--ok` `--ok-line` `--ok-bg` `--danger` `--scrim` `--blur` `--panel-border` `--panel-shadow` `--bar-bg` `--bar-border` `--bar-shadow` `--glass-sheen` `--glass-edge` `--glass-depth` `--glass-glow` `--glass-inner` `--scroll-thumb` `--scroll-thumb-hover` `--scroll-track` `--radius*` `--sh-raised` `--sh-float`。`:root` 里是「经典云白」的兜底值；`--bar-*` 三个变量专门控制顶栏（玻璃皮肤下是带色调的磨砂条），`--glass-*` 五个变量控制液态玻璃的表面高光、边缘折射、内部雾感与深度阴影，`--scroll-*` 控制滚动条（`::-webkit-scrollbar` 细胶囊滑块 + `scrollbar-color` 兜底，Windows 默认那条浅灰滚动槽和玻璃皮肤不搭）。

另有三组变量是后补的，皮肤按需覆盖、不写就吃 `:root` 兜底：

| 变量 | 管什么 |
| --- | --- |
| `--bar-btn-bg` `--bar-btn-ink` `--bar-btn-border` `--bar-btn-radius` `--bar-btn-size` | 顶栏图标按钮。**胶囊版**把外壳挪到 `.actions` 上（外壳颜色仍走 `--paper-solid` / `--panel-border`），内部按钮转透明 |
| `--nav-icon` 相关：`--nav-tile-bg` `--nav-tile-ink` `--nav-tile-radius` | 「圆角方块 / 直观重绘」等图标风格下垫的那块底色；`--nav-w` 是侧栏宽度（`body.nav-compact` 收到 68px） |
| `--glass-sheen` `--glass-edge` `--glass-depth` `--glass-glow` `--glass-inner` | 液态玻璃质感：表面高光、边缘亮线、底部深度、内侧折射与雾感。皮肤可以覆盖它们，组件规则不硬编码玻璃颜色 |
| `--tip-bg` `--tip-ink` `--tip-border` | 悬停文字提示的浮层配色 |

**主题不再是固定浅色**——皮肤可以是深色（暗夜玻璃）；组件规则里禁止硬编码色值，拖拽相关的三个类名同样走变量，任意皮肤下都要保持对比度。

**原生控件的弹层不吃变量继承**：`<select>` 的 `option` 默认 `background` 是透明的，弹出的选项列表会回退到浏览器/系统默认底色（浅色主题下是白的），而文字色继承皮肤的 `--ink`。深色皮肤下 `--ink` 是近白色 → **白底白字，下拉看起来是空的、什么都选不了**（2026-09-21 实际踩到：暗夜玻璃 / 石墨暮色下「用哪个浏览器打开」是空的，浅色皮肤一直正常所以没暴露）。因此 `option` 必须自己带不透明底色与文字色，`M4-CSS` 里的 `select option{background:var(--paper-solid);color:var(--ink)}` 是硬要求。新增原生控件（`<select>`、`<progress>`、`<input type="date">` 等）时同理：别指望它继承皮肤变量，**要么显式写色，要么改用自绘控件**（皮肤菜单 `.skin-menu`、图标选择器就是这么做的）。

### 5.1 自绘标题栏（桌面端专用）

桌面端窗口**没有系统边框**（`tauri.conf.json` 的 `decorations:false`，见 [tauri-ipc.md](tauri-ipc.md) 第 5 节），窗口外壳由前端承担：

- 拖动窗口：顶栏的 `.brand`、`.drag-space` 带 `data-tauri-drag-region`（这两个元素**不能包住任何按钮**，否则点击会触发拖动）。`.brand-mark` 虽然位于 `.brand` 内部，但它自身带 `role="button" tabindex="0"`，按 Tauri 2 的规则会被视为可点击元素而**挡住拖动**，所以点击它能正常触发上传而不会变成拖窗口。
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

### 5.3 自定义外观（「自定义 · 我的图片」动态皮肤）

`M4-SKIN` 除五套内置皮肤外还有一个**动态皮肤** `custom`：参数存在 `localStorage`（键 `pwb.skin.custom`），CSS 变量由 `customVars()` 运行时现算——`SKINS` 注册表里它的 `vars` 故意留空，不要往里写死值。

| 参数 | cfg 字段 | 控件 | 落点 |
| --- | --- | --- | --- |
| 背景图片 | `image` | `#skinImage`（file input）+ `#skinPickImage` | 自动压到最长边 1920px、转 JPEG（透明区按基调补底色）；塞不进存储就逐级降 1440/1024 |
| 底色基调 | `base` | `#skinBase`（浅色底·深字 / 深色底·浅字） | 文字色、面板基色、蒙层颜色 |
| 图片不透明度 | `imageAlpha` | `#skinImageAlpha` | `--custom-image-alpha` |
| 蒙层浓度 | `veil` | `#skinVeil` | `--custom-veil`（压花背景，保文字对比度） |
| 图片模糊 | `imageBlur` | `#skinImageBlur` | `--custom-image-blur` |
| 面板不透明度 | `panelAlpha` | `#skinPanelAlpha` | 生成 `--paper` / `--paper-hover` / `--bar-bg` 的 rgba |
| 面板毛玻璃 | `panelBlur` | `#skinPanelBlur` | `--blur` |
| 主色调 | `accent` | `#skinAccent`（color input） | 派生 `--brandwash` / `--brand-ink` / `--brand-line` / `--on-brand`（按主色亮度自动黑/白字） |

契约要点：

- **改动即生效**：面板里任何控件的 `input` 都会 `writeCfg()` → 立即 `apply('custom')` 重算全部变量，没有「保存」按钮；滑块落盘有 300ms 防抖（避免拖动时反复序列化整张图）。`#skinReset` 清空配置并回到默认参数。
- **背景画在伪元素上**：`body[data-skin="custom"]::before`（图）+ `::after`（蒙层），走变量 `--custom-image` / `--custom-image-alpha` / `--custom-image-blur` / `--custom-veil`（`:root` 有兜底值）；切回内置皮肤时这些变量会被 `apply()` 清掉，不残留。
- **CSP 合规**：预览图、蒙层、菜单色板都用 CSSOM（`el.style.xxx`）写，全程无内联 `style` 属性、无运行时 `<style>`（见 5.2 节）；壁纸是 data URL，桌面端 CSP 的 `img-src data:` 已放行。
- 存储：`localStorage` 配额约 5MB，写入失败时提示「外观仍生效但下次打开会回到上一张」。
- 入口与关闭：皮肤菜单底部「✎ 自定义外观…」（`#skinCustomOpen`，运行时生成）；面板 `#skinCustom` 的 Escape 关闭由 `M4-SKIN` 自己接管（`M4-MAIN` 的 Escape 只管它自己的弹窗）。

### 5.4 内置图标集

界面上的图标（侧栏、分类、类型切换、顶栏按钮、窗口按钮）全部来自 `M4-MAIN` 的 `ICONS` 注册表，**手写在 24×24 视口里、只描边不填充**（`fill:none` + `stroke:currentColor`），于是任何皮肤、任何底色下都自动跟着文字颜色走，也不需要外部图标库。

| 事项 | 约定 |
| --- | --- |
| 加一枚图标 | 往 `ICONS` 加一条 `名字: '<几何图形>'`；想让它出现在分类图标选择器里，再把它加进 `PICKER_ICONS` |
| 渲染 | `iconSvg(name)` 返回字符串，`iconInto(box,name)` 直接写进容器，`hydrateIcons()` 启动时处理标记里所有 `[data-icon]` |
| 取不到的名字 | 回落 `spark`，不抛错（`hasIcon()` 用的是 `hasOwnProperty`，不会误命中原型上的属性） |
| **尺寸必须由 CSS 给** | SVG 不加约束时默认 `300×150`。已有规则：`.nav-icon svg` / `.bar-btn svg` / `.win-btn svg` / `.seg-icon svg` / `.kind-icon svg` / `.icon-demo .nav-icon svg` / `.hero-slot .glyph svg`、`.icon-pick button svg`。新增图标容器时别忘了这一条 |
| 分类图标怎么选 | `categoryIcon()` 按优先级解析：① `symbol` 本身就是图标名 → ② `category.icon` 是图标名 → ③ 按 `label` 关键词（`LABEL_ICONS`，如「密码库 → lock」「备份 → archive」「临时 → clock」）→ ④ 该分类 `kind` 的默认图标（`KIND_ICONS`）。**旧的字符型 symbol（`▤` `◆` `↗` 之类）不再直接上屏** |
| 分类图标选择器 | `buildIconPicker(box,current)` 把 `PICKER_ICONS` 铺成一格一格，选中值写在 `box.dataset.value`；保存时直接取它当 `symbol` 发给后端。⚠️ **但后端目前把 `symbol` 截到 2 字符**（见 [INTERFACES.md 6.5](../INTERFACES.md#65-分类-symbol-只存得下-2-个字符低风险待办)），所以自定义图标名暂时存不下来，实际生效的是上一条的关键词/`kind` 兜底——要根治得先放宽 `M1`/`M3` 的长度限制 |

### 5.5 界面偏好与设置面板（`M4-PREF`）

| 偏好 | 取值 | 落到哪 | 效果 |
| --- | --- | --- | --- |
| 侧栏 | `expanded`（默认）/ `compact` | `body.nav-compact` | 只留图标、宽度收到 68px；鼠标停上去浮出名称 |
| 顶栏按钮文字 | `off`（默认）/ `on` | `body.bar-labels` | 默认只有图标，悬停浮出文字 |
| 顶栏样式 | `circle`（默认）/ `capsule` | `body[data-bar-style]` | 各自独立圆形按钮 / 整组收进一枚胶囊 |
| 侧边栏版式 | `card`（默认）/ `pill` / `rail` | `body[data-nav-style]` | 信息卡片 / 柔和胶囊 / 悬浮导轨；不影响数据驱动导航 |
| 图标风格 | `plain`（默认）/ `tile` / `abstract` / `readable` | `body[data-icon-style]` | 极简线稿 / 圆角方块 / 抽象高级符号 / 直观重绘图标；后两者会刷新 SVG 本体 |

契约要点：

- 存在 `localStorage` 键 `pwb.prefs`。**组件规则里不写任何 JS 判断**——只认上面五个钩子，外观全在 `M4-CSS`。
- 悬停提示只在「文字已经被收起」的地方出现（紧凑侧栏、只显示图标的顶栏），展开状态下不弹重复信息；浮层是全局单例 `.hover-tip`，坐标走 CSSOM。
- 设置面板 `#settingsModal`：皮肤网格取自 `window.PWB_SKIN.list()`，改任何一项立即生效并落盘，没有「保存」按钮；面板里的「同步文件夹 / 重新连接」走 `window.PWB_APP`。
- 入口有三处：顶栏 ⚙ 按钮、侧栏底部 ⚙ 按钮（`[data-action="settings"]`）、皮肤菜单仍保留（快捷换皮肤）。侧栏底部的折叠按钮（`[data-action="toggle-nav"]`）直接切 `expanded` / `compact`。

### 5.6 品牌标与自定义头像

顶栏与侧栏左上角共用 `.brand-mark`：

- 默认是 M4 设计的「个」字几何标（SVG，描边 `currentColor`，背景跟 `--brand` 渐变，随皮肤变化）。
- 点击任意一处 `.brand-mark`、或在设置面板点「上传头像 / Logo」，可把本地图片设成头像；图片经 `shrinkImage()` 压到最长边 256px 后存 `localStorage`（键 `pwb.brand`），不落 `data/`、不走后端。
- 上传后两处 `.brand-mark` 会同时切换；刷新页面自动恢复。
- 在设置面板点「恢复默认图标」可回到「个」字标。
- 自定义头像用 `<img class="brand-photo">` 显示，CSS 用 `object-fit:cover` 铺满并跟随 `border-radius` 裁剪；CSP 已放行 `data:` 图片，桌面端正常显示。
- 顶栏 `.brand` 带 `data-tauri-drag-region`，但 `.brand-mark` 带 `role="button" tabindex="0"`，Tauri 2 会把它识别为可点击元素而挡住窗口拖动，因此点击它不会误触发拖拽（已验证 Tauri 2.11.5）。

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
- ❌ **不要为了「图标风格 / 侧栏版式 / 顶栏样式 / 侧栏收起文字」在 JS 里逐个改样式**。只往 `<body>` 写 `data-icon-style` / `data-nav-style` / `data-bar-style` / `nav-compact` / `bar-labels` 这五个钩子，外观交给 `M4-CSS`；图标本体需要切换时调用 `window.PWB_ICONS.refresh()` 统一刷新。
- ❌ **不要给 `<svg>` 漏掉 CSS 尺寸**。SVG 不加约束时按 `300×150` 布局，会把侧栏/顶栏整个撑坏；新增图标容器时照 5.4 节那份清单补一条规则。
- ❌ **不要在带 `data-tip` 的元素上再写 `title`**，否则原生提示和浮层会一起冒出来。
- ❌ **不要把分类 `symbol` 当纯字符用**。新写数据要传图标名（见 5.4），前端虽然兼容旧的字符符号，但那是过渡兼容而不是目标形态。
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
| 2026-09-21 | **新增「自定义外观」动态皮肤（M4-SKIN）**：`SKINS` 注册表加 `custom` 项（`vars` 留空，由 `customVars()` 按用户设置现算）；新增 `#skinCustom` 面板——导入壁纸 + 图片/蒙层透明度、图片模糊、面板不透明度与毛玻璃、底色基调（浅/深）、主色调，改动即时生效并落盘 `localStorage`（键 `pwb.skin.custom`，滑块 300ms 防抖）；壁纸经 canvas 压到 ≤1920px JPEG 后存 data URL，超存储自动降级 1440/1024。CSS 新增 `--custom-image` / `--custom-image-alpha` / `--custom-image-blur` / `--custom-veil` 四个变量（`:root` 有兜底）与 `body[data-skin="custom"]::before/::after` 背景层、range/color/预览/面板样式、通用 `button:disabled`。M4-MAIN / M4-DRAG 未动。新增 5.3 节与 DOM 契约面板 ID 行。 |
| 2026-09-21 | **分类网格的位置只由 `order` 决定**（点击不再换位）：`sortItems()` 去掉 `score()` 兜底——旧实现是「两两比较时双方都有 `order` 才比 `order`，否则比 `score()`」，而 `score()` 含打开次数与距上次打开天数，于是点一下图标它就会跳到分类里的第一位。新版为「有 `order` 的按 `order` 升序 → 缺 `order` 的按后端返回的原始顺序跟在后面 → 缺 `order`/`order` 撞号用原始下标兜底」，排序键不含任何随点击或时间变化的量。`score()` 保留，仅供 `#frequent` 常用区排序。同步修正本节 `#frequent` 取前 8（原写 6）。 |
| 2026-09-21 | **界面改版：内置图标集 + 侧栏可折叠 + 设置面板 + 两套新皮肤**。① `M4-MAIN` 新增 `ICONS` 注册表（40 枚手写 24 视口线性图标）与 `iconSvg()` / `iconInto()` / `hydrateIcons()` / `categoryIcon()` / `buildIconPicker()`；侧栏与类型切换全换成 SVG 图标，分类图标按「图标名 → label 关键词 → kind 默认」逐级解析（旧的字符型 symbol 不再上屏），分类编辑器里的「符号」输入框改成图标选择器（选中值仍发到 `symbol`）。② 顶栏按钮改成图标按钮、文字默认收起（悬停浮出），侧栏支持「只显示图标」（宽 68px）。③ 新增 `M4-PREF` 子模块：四组偏好存 `pwb.prefs`、设置面板 `#settingsModal`、全局悬停提示 `.hover-tip`；`M4-MAIN` 暴露 `window.PWB_APP`，`M4-SKIN` 暴露 `window.PWB_SKIN` 并广播 `pwb:skin` 事件。④ 新增两套皮肤 `graphite-dusk`（深色简约 B 端）与 `sakura-mist`（樱粉柔光），二者另定义 `--bar-btn-*` / `--nav-tile-*`；`classic` 的 `vars` 补上几个键，好让设置面板能画出色卡。⑤ 新增变量 `--bar-btn-*` `--nav-tile-*` `--nav-w` `--tip-*`；组件样式零硬编码色值的前提不变。 |
| 2026-09-21 | 新增 `folder` 图标（24×24 描边，`fill:none` + `stroke:currentColor`），并在 `LABEL_ICONS` **最前面**插入 `/文件夹|目录/ → folder`，让内置的「文件夹」分类不再跟「文档」共用 `file` 图标——该表按顺序取第一个命中，这条规则必须排在 `/文档|资料|笔记|文件/` 之前，否则会被「文件」二字抢先匹配。`categoryIcon()` 与渲染代码未改动。 |
| 2026-09-21 | **品牌标改版 + 支持自定义头像**：把顶栏/侧栏的「工」字换成「个」字几何标（SVG），新增 `brandFile` 与 `.brand-mark` 交互。默认点击品牌标可上传自己的照片 / Logo，图片经 `shrinkImage()` 压到 256px 后存 `localStorage`（键 `pwb.brand`），两处标记同步切换、刷新后自动恢复；设置面板新增「上传头像 / Logo」与「恢复默认图标」按钮。新增 `window.PWB_BRAND` 跨块契约与 `--brand-glyph` 变量，CSS 适配 46px / 38px / 36px 三处尺寸；文档更新 5.1 / 5.6 节与 DOM 契约。 |
| 2026-09-21 | **删除顶部状态条**（`<main>` 里的 `<div id="status" class="status">`）：连接结果、同步结果、品牌图报错原先都写在这条上，删除后统一改由新增的 `setStatus(text,bad)` 写进设置面板的 `#settingsStatus`（类名保持 `tip` / `tip bad`，不覆盖成原来那套 `status ok` / `status bad`）。注意：`connect()`、`scanFolders()`、`brandNotice()` 里原有 6 处 `$('status')` 未做空值保护，**只删元素必然出问题**——`connect()` 会在赋值处抛 `TypeError`，被自己的 `catch` 接住后又在 `catch` 里抛第二次，其后的 `loadItems()` 再也不执行，首屏直接空白；所以元素与这三处写入必须同一次改完。DOM 契约表里的「状态条 `status`」一行随之删除，「数据区 `settingsStatus`」成为唯一的状态文案出口。`.status`/`.status.ok`/`.status.bad` 三条 CSS 规则与 `paintStatus()` 已成死代码，本次保留未动。 |
| 2026-09-21 | **修复深色皮肤下原生下拉「空白、选不中」**：`select` 的 `option` 未设底色时其 `background` 是 `rgba(0,0,0,0)`，弹出列表回退到系统浅色底（白），而文字色继承皮肤的 `--ink`——暗夜玻璃（`#e8ebf8`）/ 石墨暮色（`#eef3f9`）下即为白底白字，「用哪个浏览器打开」等下拉看着是空的、任何一项都点不着；浅色皮肤文字是深色所以一直没暴露。`M4-CSS` 新增 `select option{background:var(--paper-solid);color:var(--ink)}`，一条规则覆盖全部原生下拉（网址的浏览器选择、归类弹窗的分类选择、开始菜单的分组筛选）。通用约定补进第 5 节。 |
| 2026-09-21 | **侧栏分类支持拖动排序**：`renderNav()` 给数据驱动分类加 `.category-link[data-category-key]`，`M4-DRAG` 用 Pointer Events 提供跟手幽灵与插入动画；总览、常用、待补充不带该类，保持固定。结束后 `persistCategoryOrder()` 串行调用既有的 `/categories/update` 更新 `Category.order`，避免并发写索引；补齐 IPC 映射清单中的分类更新/删除与浏览器探测条目。 |
| 2026-09-21 | **侧边栏与图标视觉扩展**：`M4-PREF` 新增 `navStyle` 偏好与 `#navStyleSeg`，在信息卡片（默认）、柔和胶囊、悬浮导轨三种侧边栏构图间切换，可与只显示图标的紧凑态叠加；`iconStyle` 从两种扩为四种，新增柔光胶囊与立体圆形。两个偏好均只通过 `body[data-nav-style]` / `body[data-icon-style]` 驱动 CSS，继续不影响数据、拖拽、DOM 导航契约。 |
| 2026-09-21 | **液态玻璃与图标本体升级**：玻璃卡片从单层半透明改为变量驱动的表面高光、边缘折射、内部雾感和深度阴影（新增 `--glass-*` 变量）；图标风格中的 `soft/solid` 迁移为 `abstract/readable`，分别提供抽象高级符号与直观重绘图标两套 SVG 路径，切换时由 `window.PWB_ICONS.refresh()` 重新填充已有 DOM。 |
