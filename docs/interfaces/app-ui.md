# 接口文档 · `M4` 前端界面（app/index.html）

## 职责与边界

`app/index.html` 是**唯一的界面**，也是浏览器版与桌面版共用的同一份文件。HTML、CSS、JS 全部内嵌；除 `app/nocturne-rose-landscape.png`（Nocturne · 玫瑰圣殿皮肤随应用打包的背景图）外，**不加载任何相对路径的静态资源、不引入 CDN**。

文件内部又分成五个**子模块**，本模块的接口文档主要就是固定它们之间的契约：

| 子模块 | 位置 | 职责 |
| --- | --- | --- |
| `M4-CSS` | `<head>` 内的 `<style>` | 全部样式，含拖拽视觉（`.drag-ghost` 等）、各皮肤专属背景规则、多版图标风格、两版顶栏按钮与三档顶栏形式的规则 |
| `M4-MAIN` | 第一个 `<script>` 块 | 页面骨架、渲染、表单、上下文菜单、**内置图标集 `ICONS`**、**双通道适配层 `request()`** |
| `M4-SKIN` | 第二个 `<script>` 块（IIFE） | 皮肤注册表、CSS 变量注入、皮肤切换菜单、localStorage 记忆 |
| `M4-DRAG` | 第三个 `<script>` 块（IIFE） | 图标拖拽排序（Pointer Events + FLIP 动画） |
| `M4-PREF` | 第四个 `<script>` 块（IIFE） | 界面偏好（侧栏/顶栏是否收文字、顶栏按钮两版、顶栏形式三档、图标四版）、设置面板、悬停文字提示 |

**允许读写**

| 路径 | 权限 |
| --- | --- |
| `app/index.html` | ✅ 界面入口 |
| `app/nocturne-rose-landscape.png` | ✅ Nocturne · 玫瑰圣殿皮肤的本地背景图；必须与 `index.html` 一起随 `app/` 打包 |
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

**启动时的调用顺序（性能契约）**：`connect()` **并行**发出 `GET /status` 与 `GET /items`（`Promise.all`），拿到后在同一步里写 `state` 并 `renderNav()` + `render()`。

- 这两条互不依赖（`/status` 只提供 `dataPath` 文案），串行会让首屏白等一个来回：冷启动 A/B 对拍里并行版内容就绪中位数 145ms、串行版 171ms（浏览器通道；桌面版两次 IPC 的延迟同样是叠加的）。
- **不要再改回「先 await /status 再 await /items」**，也不要为了「先显示状态」把 `render()` 往后排。首屏渲染只依赖 `/items`。
- `loadItems()` 保留原样，供增删改之后重新拉取（14 处调用方），**它不参与启动路径**。

> 📊 **首屏预算（2026-09-21 实测，供以后判断"该不该优化"）**：桌面版从点开 exe 到界面可交互 **830–1120ms**，其中 **WebView2/Tauri 运行时启动就占 550–800ms**，页面自身的 `responseEnd → 内容渲染完成` 只有 **120–200ms**；主线程 `ScriptDuration` 7ms、`LayoutDuration` 14ms。浏览器版整页 `load` 在 27–84ms、`#content` 渲染完在 186–237ms（并行化后 131–173ms）。
> 也就是说：**应用代码里已经没有"大件"可砍**，改动前请先用 `tmp/perf/` 的方法量一次，不要凭感觉做"优化"（曾经怀疑的 155KB 内联 CSS/JS 与 `backdrop-filter` 都不是主因：对照实验里去掉全部 `backdrop-filter` 只省 ~24ms）。

### 2. DOM 契约（`M4-MAIN` 与 `M4-DRAG` 之间的接口）

`M4-DRAG` 不认识任何业务数据，它**只依赖下面这组 DOM 约定**。改动这些约定必须同步两个脚本块。

**必须存在的元素 ID**（主脚本直接 `$('...')` 取用，删掉即 `TypeError`）：

| 区域 | ID |
| --- | --- |
| 顶栏 | `barActions`（按钮组容器）、`connect` `scan` `openAdd` `multiSelect` `openSettings` `skinToggle` `skinMenu`（后两个归 `M4-SKIN`） `winMin` `winMax` `winClose`（后三个归自绘标题栏，仅桌面端显示）。按钮一律是 `.bar-btn`，图标由 `data-icon` 声明 |
| 设置面板（`M4-PREF`） | `settingsModal` `settingsTitle` `settingsClose` `settingsDone` `settingsNotice`、皮肤网格 `skinGrid` + `settingsCustomSkin`、顶栏图标 `settingsBrandPick` `settingsBrandReset` `settingsBrandNotice`、侧边栏版式/开关 `navStyleSeg` `navModeSeg` `navModeTip`、圆形胶囊底色滑块 `orbOpacityControl` `orbOpacity` `orbOpacityOut`、顶栏开关 `barLabelSeg` `barStyleSeg` `barShapeSeg` + 说明文案 `barShapeTip`、图标风格开关 `iconStyleSeg` + 预览 `iconDemo`、**逐分类图标样式 `perCategoryStyles`（内容由 `renderPerCategory()` 在每次 `syncForm()` 时按 `sectionCategories()` 重建）**、数据区 `settingsStatus` `settingsScan` `settingsConnect`。另有 `[data-action="settings"]`（顶栏按钮与侧栏底部入口）按选择器绑定，不是 ID |
| 自定义外观面板（`M4-SKIN`） | `skinCustom` `skinCustomTitle` `skinCustomClose` `skinPreview` `skinPreviewTip` `skinPickImage` `skinClearImage` `skinImage` `skinBase` `skinBaseTip` `skinImageAlpha` `skinImageAlphaOut` `skinVeil` `skinVeilOut` `skinImageBlur` `skinImageBlurOut` `skinPanelAlpha` `skinPanelAlphaOut` `skinPanelBlur` `skinPanelBlurOut` `skinAccent` `skinAccentOut` `skinNotice` `skinReset` `skinDone`；另有 `skinCustomOpen`（菜单里「✎ 自定义外观…」入口）由 `M4-SKIN` 运行时生成，不在静态标记里 |
| 侧栏 | `nav`（导航按钮由 `renderNav()` 动态生成，**没有写死的分类按钮**） |
| 网格容器 | `frequent` `pending` |
| 内容区 | 各分类的 `.content-section` 由 `render()` 动态生成 |
| 右键菜单 | `contextMenu` |
| 条目编辑器 | `editor` `dialogTitle` `dialogHint` `closeAdd` `cancelAdd` `notice` `save` `createCategoryBlock` `createCategory`（新增时选择目标分类）；`kindSwitch` 仅保留为内部类型字段切换容器 |
| 编辑器表单 | `title` `description` `tags` `url` `browserPick` `browserPath` `browserTip` `scriptBody` `extension` `importScript` `appPicked` `pickApp` `manualApp` `appManual` `appPath` `readApp` `filePath` `iconSlot` `iconSlotGlyph` `iconSlotImg` `iconTip` `iconFile` `scriptFile`、品牌头像 `brandFile` |
| 待补充归类 | `assignBlock` `assignCategory` `assignNew` `assignNewName` `assignNewSymbol` `assignNewKind` |
| 开始菜单选择器 | `picker` `pickClose` `pickCancel` `pickSearch` `pickGroup` `pickList` `pickCount` `pickDetail` `pickNotice` `pickUse` |
| 新建分类弹窗 | `categoryModal` `categoryClose` `categoryCancel` `categoryName` `categorySymbol` `categoryKinds` `categoryNotice` `categorySave`、图标样式 `categoryStyleSeg`（由 `buildIconStyleSeg()` 生成，与设置面板的逐分类行同一个构造函数） |
| 合并包（`M4-MAIN`） | 起名弹窗 `pkgModal` `pkgTitle` `pkgHint` `pkgName` `pkgNotice` `pkgSave` `pkgCancel` `pkgClose`；展开面板 `pkgPanel` `pkgGrid` `pkgPanelNotice` `pkgPanelClose`；多选操作条 `bulkBar` `bulkCount` `bulkHint` `bulkMerge` `bulkClear` |

**必须存在的属性约定**：

| 选择器 | 约定 |
| --- | --- |
| `.icon-card` | 一个可打开/可拖拽的图标卡。**必须带 `data-id`**（值 = 条目 `id`），否则排序会提交空 id |
| `.icon-grid` | 图标容器。**`id === 'frequent'` 的网格不参与拖拽**，这是唯一被硬编码排除的容器 |
| `.content-section[data-section]` | 区块，取值 `frequent` / `<分类 key>`，与 `.nav-link[data-view]` 的取值一一对应，由 `applyView()` 控制显隐 |
| `.nav-link[data-view]` | 侧边栏导航，取值 `all` / `frequent` / `<分类 key>` |
| `.nav-link.category-link[data-category-key]` | 由数据驱动分类生成的侧栏导航；`M4-DRAG` 只允许它们拖动，`data-category-key` 是 `Category.key`。总览、常用没有此类，位置固定 |
| `.kfields[data-kind]` | 编辑器里**按类型分流**的字段组，取值 `script` / `link` / `app` / `file`。`setEditorKind()` 只显示与当前类型匹配的那一组 |
| `.pick-row` / `.pick-detail` | 开始菜单选择器的列表行与右侧详情 |
| `.kind-card` | 分类新建/归类弹窗里的类型选择卡 |
| `.icon-pick` | 图标选择器（新建分类、归类时给分类挑图标）。选中的图标名放在容器的 `dataset.value` 上，内部按钮带 `data-icon-name` |
| `.nav-icon[data-icon-name]` | 侧栏图标容器；属性值 = 实际用的图标名，便于调试与自动化验证 |
| `[data-icon]` | 声明图标的宿主元素（`.bar-btn` 内部的 `.bar-ic`、侧栏底部按钮内部的 `.nav-icon`、窗口按钮自身）。启动时由 `hydrateIcons()` 把 `iconSvg()` 的结果填进去 |
| `[data-tip]` | 悬停文字提示的文本来源；只有「文字已被收起」时才由 `M4-PREF` 浮出来 |
| `[data-action="settings"]` | 打开设置面板；顶栏与侧栏底部各有一处，由 `M4-PREF` 按选择器绑定 |
| `body[data-icon-style]` | 图标风格：`plain`（极简线稿，默认）/ `tile`（圆角方块垫底）/ `abstract`（抽象高级符号）/ `readable`（直观重绘图标）。后两者会切换 SVG 路径本体，不只是改容器外观 |
| `body[data-nav-style]` | 侧边栏版式：`arc`（弧形导航，默认）/ `card`（信息卡片）/ `rail`（悬浮导轨）/ `orb`（圆形胶囊）；仅改变外观，可与紧凑态组合，与皮肤正交。`arc` 与 `orb` 都没有侧栏磨砂底板，圆形图标使用实体按钮；前者沿内凹弧线排布，后者居中直列。旧的 `pill` 本地偏好会在读取时自动迁移为 `arc`。⚠️ `rail` 的**选中态只换颜色、不画底板**（`background:transparent` + `color:var(--brand-ink)`）：紧凑侧栏下 `.nav-link` 被 `.sidebar{align-items:center}` 收到**图标宽 32px、行高 48px**，任何底板（尤其 `--paper-solid`）配上 `border-radius:14px` 都会变成「套住图标的深色椭圆框」。皮肤样式里凡会影响弧形或圆形胶囊按钮圆角的选择器必须加对应守卫 |
| `body[data-bar-style]` | 顶栏样式：`circle`（各自独立圆形，默认）/ `capsule`（收进一枚胶囊） |
| `body[data-bar-shape]` | **顶栏形式，刻意与皮肤解耦**：`auto`（跟随皮肤，默认）/ `bar`（吸顶通栏）/ `float`（悬浮浮岛）。形式只管几何（宽度、顶部留白、圆角、边框、阴影，走 `--bar-shape-*` 变量），配色仍由皮肤变量决定，所以任意皮肤都能配任意形式 |
| `body.nav-compact` | 侧栏只显示图标（宽度 `--nav-w` 收到 68px） |
| `body.bar-labels` | 顶栏按钮显示文字（默认不带，只显示图标） |
| `body.multi-select` | 多选模式钩子：卡片点击只切换选中、不打开条目，`M4-DRAG` 在此期间不启动拖拽。组件规则里同样不写 JS 判断，外观全在 `M4-CSS` |
| `.icon-card.picked` | 多选模式下被选中的条目卡（右上角一枚实心圆点）。只在 `body.multi-select` 下出现 |
| `.icon-card.merge-target` | 拖拽中指针停在某张卡**中间那块**时的合并目标高亮；松手即合并成一个包 |
| `.icon-card.pkg-card[data-pkg]` | 合并包的文件夹卡。**没有 `data-id`**（它不是条目），包的 key 放在 `data-pkg`。它与普通条目卡**共处同一个 `.icon-grid`**（挂在同类分类下，见第 3 节），因此约定成对：`M4-DRAG` 的 `pointerdown` 跳过它（不能被拖动）、`placeAt()` 不把它算进排序参照、`orderOf()` / `persistOrder()` 只取 `.icon-card[data-id]`（不会把空 id 提交给 `reorder`）；反过来它是**合法的「放进这个包」落点**——把一张条目卡拖到它中间松手 = `window.PWB_MERGE.intoPackage(pkgKey, itemId)` |
| `.pkg-stack` / `.pkg-cell` | 文件夹卡里那 2×2 的迷你图标格（最多 4 枚成员图标，仍走 `paintIcon()`，所以字母头像也照常工作） |

**跨块函数契约**：

| 函数 | 归属 | 被谁调用 | 说明 |
| --- | --- | --- | --- |
| `persistOrder(grid)` | `M4-MAIN` | `M4-DRAG` 的 `endDrag()` | 拖拽结束时把网格内 `data-id` 顺序提交给后端。**`M4-MAIN` 改名/删掉它，拖拽就会在结束时抛错。** 它内部已有 `if (ids.length < 2) return` 保护 |
| `persistCategoryOrder(nav)` | `M4-MAIN` | `M4-DRAG` 的 `endNavDrag()` | 读取 `.category-link` 顺序并按序更新 `Category.order`；请求必须串行，避免并发写 `catalog.json` 相互覆盖 |
| `iconSvg(name)` / `iconInto(box,name)` | `M4-MAIN` | `M4-PREF`（图标风格预览） | 生成 / 填充一枚 SVG 图标。**名字取不到时回落 `spark`，不会抛错** |
| `window.PWB_ICONS.refresh()` | `M4-MAIN` | `M4-PREF` | 图标风格从 `plain/tile` 切到 `abstract/readable` 时，重新填充顶栏、侧栏、图标选择器和预览里的 SVG 路径 |
| `window.PWB_ICONS`（其余四项） | `M4-MAIN` | `M4-PREF` | `styleOf(category)` 取该分类实际生效的档位；`overrideOf(category)` 取它**自己设过**的档位（没设过返回 `''`，用来判断「再点一次＝取消」）；`setStyle(key,style)` 写覆盖（空值/非法值＝删除）；`refreshRows()` 只重绘侧栏各行图标，不重建导航；`labels` 是四档的中文名（唯一一份，`M4-PREF` 不再自己写一份） |
| `window.PWB_APP` | `M4-MAIN` 末尾暴露 | `M4-PREF` | `{ connect, scanFolders }`：设置面板里的「重新连接 / 同步文件夹」调它 |
| `window.PWB_SKIN` | `M4-SKIN` 末尾暴露 | `M4-PREF` | `{ list(), apply(id), openCustom() }`：设置面板列皮肤、换皮肤、开自定义外观面板 |
| `window.PWB_BRAND` | `M4-MAIN` 末尾暴露（品牌标初始化之后） | `M4-PREF` | `{ pick(), reset() }`：设置面板里的「上传头像 / Logo」与「恢复默认图标」调它 |
| `window.PWB_MERGE` | `M4-MAIN` 末尾暴露 | `M4-DRAG` 的 `endDrag()` | `{ fromDrag(sourceId, targetId), intoPackage(packageKey, sourceId) }`：`fromDrag` 把「A 叠到 B 上松手」变成**新建**一个包；`intoPackage` 把「A 叠到包卡片上松手」变成**放进那个已有的包**（条目与包类型不同会被拒绝并提示）。**删除或改名会让拖拽合并静默失效**（调用处写的是 `window.PWB_MERGE?.xxx(...)`，不会抛错） |

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
- 合并包**不占侧栏、也没有自己的区块**（2026-09-21 改）：`isPackage(category)` 为真的分类（`symbol === 'pk'`，或 `note` 以「合并包」开头；内置分类与收件箱永不参与）**不再作为侧栏里的一行分类**，而是渲染成一张 `.pkg-card` 文件夹卡，**排在它「宿主分类」网格的最前面**，点开是 `#pkgPanel`（成员在那里平铺，可编辑、删除、再合并）。
  - 宿主由 `packageHostKey(kind)` 决定：优先同 `kind` 的**内置分类**（`key` 与 `kind` 同名，即应用 / 网址 / 脚本 / 文件），其次同 `kind` 的任意分类，最后退回「文件」。于是「两个应用合并成一个包」就长在「应用」里，侧栏不会冒出「合并包 / 合并包 2」。
  - 侧栏与区块都改用 `sectionCategories()`（= `visibleCategories()` 去掉包）；**包的宿主分类计数要把成员算进去**（成员条目的 `category` 指向包，不指向宿主）。「待补充 → 归类」的下拉（`#assignCategory`）**故意仍用 `visibleCategories()` 全集（含包）**：那是把一条待整理内容直接补进已有包的入口，不要「顺手」把它也过滤掉。
  - `state.view` **永远不会是包的 key**：`buildPackage()` 建完包只刷新并写 `#settingsStatus`，不再切视图；`openPackagePanel()` 只认 `state.pkgKey`。
  - 删除一个包 = 解散：成员退回同类的内置分类（`builtinCategoryFor(kind)`），再 `POST /categories/delete {removeFolder:true}`——**不能**走「删除分类」那条路（它会连条目与磁盘文件一起删）。包卡片右键与 `#pkgPanel` 底部的按钮都叫「删除这个包」，确认文案里写明不删任何内容与文件。
  - ⚠️ **数据层没变**：包仍是一个真实分类（`catalog.json` 里多一条 `symbol='pk'` 的分类，成员只是 `category` 指过去），所以两端读写逻辑、`M1`/`M3` 接口、`data-catalog.md` 都不用改；这是纯展示层改动。

**四个 `kind` 各有各的编辑器**（`setEditorKind()` + `.kfields[data-kind]`），首屏只显示该类型的必填字段：

| kind | 首屏字段 | 补充手段 |
| --- | --- | --- |
| `script` | 脚本内容、文件后缀 | 「导入文件」读取已有脚本 |
| `link` | 网址、**用哪个浏览器打开**（`#browserPick`，空 = 跟随系统默认） | 只填域名会自动补 `https://`；浏览器选项由 `setEditorKind()` 准备，见下节说明 |
| `app` | （无输入框）从开始菜单选择 / 手动填路径 | 选择器会带出目标、参数、工作目录与真实图标 |
| `file` | 文件或文件夹路径 | 也可以把文件丢进分类文件夹后点「同步文件夹」 |

「说明」与「标签」统一折叠进 `<details class="more">`，默认收起。

**`#browserPick` 的准备时机**（只有 `link` 类型有）：选项来自 `GET /browsers`，`loadBrowsers()` 把探测结果缓存在 `state.browsers`（`browsersReady` 为真后不再请求）。铺选项由 `setEditorKind()` 负责——**每次进入 `link` 字段组**都调 `prepareBrowserFields()`：先用已有列表铺一次，探测结果回来再铺一次；切走前把当前选择收进 `state.editor.browser`，切回来不丢。

> ⚠️ **不要在 `openEditor()` 末尾按 `state.editor.kind==='link'` 判断后只铺一次**（2026-09-21 修掉的 bug）：编辑器里手动把类型从「脚本」切到「网址」走的是 `setEditorKind()`，那条路径不铺选项，`#browserPick` 就一直是个空 `<select>`——点开什么都没有，看起来像「前端没有这个功能」，而 `M1` 的 `GET /api/browsers` 与 `M3` 的 `list_browsers` 其实都是好的。另需注意 `state.editor.kind` 是上一次编辑的残留值，`openEditor()` 必须把它重置，否则「离开 link 时保存当前选择」那段逻辑会拿旧值覆盖新条目的 `browser` 回填。

### 4. 交互契约

| 交互 | 实现要点 |
| --- | --- |
| 拖拽排序 | **必须用 Pointer Events，不要换成 HTML5 drag-and-drop**。Tauri 2 在 Windows 下会接管 OS 级拖放使 `draggable="true"` 完全失效（这是历史上「拖不动」的根因） |
| 点击 vs 拖动 | 5px 阈值区分；拖动结束后紧随的 `click` 会被捕获阶段吞掉，避免误触「运行/打开」 |
| 图标上传 | `shrinkImage(file, max=256)`：最长边缩到 256px，PNG 超 400KB 转 JPEG(0.9)。输入文件上限 8 MiB |
| 应用图标 | 选中快捷方式后 `shrinkImage(icon, 128)` 归一化成 128px PNG。**这一步是异步的，且 `usePickedShortcut()` 会先把弹窗关掉再 await**——所以外部（含自动化测试）不能靠「弹窗关了」判断完成，要等 `state.editor.icon` 有值 |
| 右键菜单 | `contextmenu` 事件 + 固定定位，坐标用 `innerWidth/innerHeight` 夹取；菜单项由 `buildContextMenu()` 按条目类型动态生成 |
| 编辑器模式 | `state.editor.mode` 三值：`create`（新建时经 `#createCategory` 选目标分类，类型由分类锁定）/ `edit`（编辑，类型由分类锁定）/ `assign`（待补充归类，多一个 `#assignBlock` 选择目标分类，可选内联新建分类） |
| 新建分类 | `openCategoryModal()` → `POST /categories`。后端会**真的在 `data/` 下建出文件夹**并写 `.gitkeep`，之后往里丢文件点「同步文件夹」就能收录 |
| 脚本文件导入 | `#scriptFile` 选中后读文本填入 `#scriptBody`，并自动带出文件名与后缀 |

### 5. 与样式的关系（`M4-CSS` + `M4-SKIN`）

样式全部内联在 `<head>` 的 `<style>` 里，**所有颜色/圆角/模糊/阴影都走 CSS 变量**，由 `M4-SKIN` 在运行时注入。基础变量：`--ink` `--muted` `--faint` `--line` `--line-strong` `--paper` `--paper-solid` `--paper-hover` `--wash` `--brand` `--brandwash` `--brand-ink` `--brand-line` `--on-brand` `--soft` `--soft-ink` `--ok` `--ok-line` `--ok-bg` `--danger` `--scrim` `--blur` `--panel-border` `--panel-shadow` `--bar-bg` `--bar-border` `--bar-shadow` `--glass-sheen` `--glass-edge` `--glass-depth` `--glass-glow` `--glass-inner` `--scroll-thumb` `--scroll-thumb-hover` `--scroll-track` `--radius*` `--sh-raised` `--sh-float`。`:root` 里是「经典云白」的兜底值；`--bar-*` 三个变量专门控制顶栏（玻璃皮肤下是带色调的磨砂条），`--glass-*` 五个变量控制液态玻璃的表面高光、边缘折射、内部雾感与深度阴影，`--scroll-*` 控制滚动条（`::-webkit-scrollbar` 细胶囊滑块 + `scrollbar-color` 兜底，Windows 默认那条浅灰滚动槽和玻璃皮肤不搭）。

另有三组变量是后补的，皮肤按需覆盖、不写就吃 `:root` 兜底：

| 变量 | 管什么 |
| --- | --- |
| `--bar-btn-bg` `--bar-btn-ink` `--bar-btn-border` `--bar-btn-radius` `--bar-btn-size` | 顶栏图标按钮。**胶囊版**把外壳挪到 `.actions` 上（外壳颜色仍走 `--paper-solid` / `--panel-border`），内部按钮转透明 |
| `--bar-shape-*`（8 个） | 顶栏**形式**的几何：宽度、顶部留白、最小高度、圆角、边框、阴影、主体上边距、侧栏吸顶偏移。**只在 `body[data-bar-shape="float"]` 下生效**：`bar` 走基础通栏规则，`auto` 由皮肤自带的形状规则决定（见 5.1 节）。皮肤不要用它们写自己的形状 |
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

**顶栏形式与皮肤解耦（2026-09-21）**：顶栏的**形状**不再由皮肤决定，改由 `M4-PREF` 的「顶栏形式」开关控制（钩子 `body[data-bar-shape]`）：

| 取值 | 含义 |
| --- | --- |
| `auto`（默认） | 跟随皮肤自带的样子：`nocturne-rose` 是浮岛（月牙顶栏），其余皮肤是吸顶通栏 |
| `bar` | 强制吸顶通栏：贴顶、占满整宽、只有底边一条线 |
| `float` | 强制悬浮浮岛：居中留白、四角圆润、带边框与投影 |

- 几何全部走 `--bar-shape-width` / `--bar-shape-gap` / `--bar-shape-min-h` / `--bar-shape-radius` / `--bar-shape-border` / `--bar-shape-shadow` / `--bar-shape-main-top` / `--bar-shape-sidebar-top` 八个变量（`:root` 给中性兜底，皮肤可按需微调）。
- **皮肤自带的特殊形状只能写在 `auto` 这一档里**：`nocturne-rose` 的月牙顶栏就是 `body[data-bar-shape="auto"][data-skin="nocturne-rose"] header`（外加 `main` / `.sidebar` 两条偏移）。用户一旦选 `bar` / `float`，这些规则因 `auto` 不匹配而自动失效，形状完全交给上面的通用规则 —— **这就是解耦的关键**。
  ⚠️ **别把皮肤的形状写进 `--bar-shape-*` 变量、也别无条件写 `body[data-skin="x"] header`**：变量是挂在 `<html>` 上的全局值，一旦被皮肤覆盖，`float` 档就会把通用浮岛又按回皮肤的形状（`nocturne-rose` 第一版正是这么写的，实测 `auto` 与 `float` 截图完全一样，等于没解耦）。
- 形式只影响外观，不碰 `data-tauri-drag-region` / `#winMin` / `#winMax` / `#winClose`，桌面端拖拽与窗口控制不受影响。
- 窄屏（`max-width:760px`）下 `float` 退成 `width:calc(100% - 28px)` + 四角 `18px`，`auto` 下的 `nocturne-rose` 月牙仍保持原样（`calc(100% - 28px)` + 下圆角 `24px`），因为顶栏在窄屏本身就是多行布局。

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

### 5.4 弹窗内容区的滚动契约（`M4-CSS` 的 `.dialog` 骨架）

所有弹窗共用同一套骨架 `.modal > .dialog > （.dialog-head / .dialog-body / .dialog-foot）`：`.dialog` 是列向 flex、`max-height:calc(100vh - 40px)`、`overflow:hidden`，`.dialog-body` 是**唯一的滚动区**，`.dialog-head` 与 `.dialog-foot` 永远留在可视区。**内容高于窗口时靠 `.dialog-body` 滚动，不允许把内容压扁。**

`.dialog-body` 的规则里 `min-height:0` 与 `grid-auto-rows:min-content` **两者都是必需的，缺一个就会复现「内容被压扁、且滚不动」**（2026-09-21 实测）：

| 现象（缺修复时） | 原因 |
| --- | --- |
| 矮窗口下每个 `.set-block` 只剩标题那么高（实测 900×560 时 clientHeight 53px、scrollHeight 579px），里面内容溢出到下一个区块上、又被 `.dialog` 的 `overflow:hidden` 裁掉 | `.dialog-body` 既是 `.dialog` 的 flex 子项、又是自己的滚动容器，此时它的 `min-height:auto` 按 **0** 算，会被压到内容高度以下；它又是 `display:grid`，默认的 `auto` **网格行允许收缩到内容高度以下**，于是行跟着塌陷 |
| 滚动条永远不出现（`scrollHeight === clientHeight`，滚轮完全无效） | 网格行已经被排进压缩后的高度里，容器认为自己「刚好装得下」，所以不产生可滚动的溢出 |

- `min-height:0` 是 flex 子项的收缩许可（防止外层再压它），**单独加它没用**——实测加上之后几何数值一字不变。
- `grid-auto-rows:min-content` 才是关键：把每一行锁在内容高度，区块恢复自然高度，`scrollHeight` 于是大于 `clientHeight`，滚动条正常出现、滚轮到得了底。窄窗口下换成 `display:block` 也能达到同样效果，但会丢掉 `gap`，所以采用改行高这一条（改动面最小）。
- `overscroll-behavior:contain` 让滚到头的滚动不再传递给背后的页面。
- 这条在**通用 `.dialog-body` 规则**上，所以设置面板、自定义外观、条目编辑器、新建分类、合并包命名等所有弹窗一起受益；新增弹窗时直接沿用这三个类即可，**不要自己写死高度或再加一层内部滚动**。
- ⚠️ 修的是「行被压扁」，不是「内容太长」：窗口够高时不该出现滚动条，窗口变矮时才出现——这正是用户要的「加载不完就滚动、不要挤在一屏」。
- 编辑器里有**两个** `.dialog-body`：`#assignBlock`（待补充归类，默认 `hidden`）与主表单。自动化验证按选择器取 `.dialog-body` 时要注意会取到第一个，请显式指定 `#editor .dialog-body:not(#assignBlock)` 之类。

### 5.5 内置图标集

界面上的图标（侧栏、分类、类型切换、顶栏按钮、窗口按钮）全部来自 `M4-MAIN` 的 `ICONS` 注册表，**手写在 24×24 视口里、只描边不填充**（`fill:none` + `stroke:currentColor`），于是任何皮肤、任何底色下都自动跟着文字颜色走，也不需要外部图标库。

| 事项 | 约定 |
| --- | --- |
| 加一枚图标 | 往 `ICONS` 加一条 `名字: '<几何图形>'`；**必须同时加进 `PICKER_ICONS`**（见下一行），否则图标在界面上挑不到 |
| `ICON_GROUPS` / `ICON_FORMS` / `PICKER_ICONS` | `ICON_GROUPS` 是**分组的**候选集：9 组 = 8 个分类（概览 / 常用 / 脚本 / 网址 / 应用 / 文档 / 待补充 / 密码库；各 4 个形态方案，顺序与 `assets/icons/category-icon-forms-preview.html` 一致）+ 1 个「通用」组（28 枚语义图标）。`ICON_FORMS` = 前 8 组展平后的 32 个形态方案；`PICKER_ICONS` = 全部 60 枚展平（供「库里有没有这枚」这类判断）。**选择器是按 `ICON_GROUPS` 分组渲染的，不是一维平铺**。新增图标漏加进这里不会报错，只是选择器里永远看不到它 |
| 渲染 | `iconSvg(name)` 返回字符串，`iconInto(box,name)` 直接写进容器，`hydrateIcons()` 启动时处理标记里所有 `[data-icon]` |
| 取不到的名字 | 回落 `spark`，不抛错（`hasIcon()` 用的是 `hasOwnProperty`，不会误命中原型上的属性） |
| **尺寸必须由 CSS 给** | SVG 不加约束时默认 `300×150`。已有规则：`.nav-icon svg` / `.bar-btn svg` / `.win-btn svg` / `.seg-icon svg` / `.kind-icon svg` / `.icon-demo .nav-icon svg` / `.hero-slot .glyph svg`、`.icon-pick button svg`。新增图标容器时别忘了这一条 |
| 分类图标怎么选 | `categoryIcon()` 按优先级解析：**⓪ 用户在设置面板里给这个分类单独挑的图标（`pwb.prefs.iconNames[分类key]`）** → ① `symbol` 本身就是图标名 → ② `category.icon` 是图标名 → ③ 按 `label` 关键词（`LABEL_ICONS`，如「密码 → lock」「备份 → archive」「临时 → clock」）→ ④ 该分类 `kind` 的默认图标（`KIND_ICONS`）。**旧的字符型 symbol（`▤` `◆` `↗` 之类）不再直接上屏** |
| 内置分类的形态图标 | 内置五类（脚本 / 网址 / 应用 / 文件 / 待整理）的 `symbol` 都是旧字符，所以**实际生效的是第 ③ 段关键词匹配**——落在 `LABEL_ICONS` 最前面那七条「形态映射」上（`terminal` / `pin` / `puzzle` / `book-open` / `folder-question`，另有给自建分类用的 `stack` `flame` `keyhole`）。这七条必须排在其后的通用关键词之前；其中「文件」那条带 `(?!夹)` 负向断言，否则「文件夹」会被「文件」二字抢先匹配成 `book-open`。改这张表前先想清楚顺序，它是**取第一个命中**的 |
| 想让某个分类固定用某枚图标 | **首选设置面板「每个分类的图标样式」那一段里点该分类的图标**（写第 ⓪ 段 `pwb.prefs.iconNames`，本端生效、内置分类也能改）；往 `category.icon` 写图标名（第 ② 段）是「跟着数据走」的做法，但注意 `symbol`/`icon` 那条老限制仍在（见 [INTERFACES.md 6.5](../INTERFACES.md#65-分类-symbol-只存得下-2-个字符低风险待办)），经新建分类弹窗里的图标选择器存下来的名字会被后端截到 2 字符 |
| 分类图标选择器 | `buildIconPicker(box,current,onPick,keyword)` **按 `ICON_GROUPS` 分组渲染**：一个分类 = 一个 `.icon-group`，块内顺序必须是 **`.icon-row-label`（组名 + 一句说明）在上、`.icon-row`（图标排）在下**，容器 `.icon-pick` 因此是单列 grid、块间用 `.icon-group + .icon-group` 的分隔线。⚠️ 标题**不能**放在图标排下面：那样它视觉上会落到下一组的头上（2026-09-21 实测：把「密码库」的标题压在了「通用」那 28 枚上面，看起来像归属错了）。`keyword` 是名字筛选，命中 0 枚的组**整块不渲染**（标题不会残留）；全不命中时在容器里放一条提示。选中值写在 `box.dataset.value`。两个调用点：**新建分类弹窗**（`#categorySymbolPick`，保存时把它当 `symbol` 发给后端）与**设置面板的逐分类弹层**（`.icon-pop`，选中的名字写进 `pwb.prefs.iconNames`，不走后端）。⚠️ 前者受 [INTERFACES.md 6.5](../INTERFACES.md#65-分类-symbol-只存得下-2-个字符低风险待办) 限制（`symbol` 被截到 2 字符），所以**内置/既有分类要换图标，用后者**；要根治得先放宽 `M1`/`M3` 的长度限制 |

### 5.6 界面偏好与设置面板（`M4-PREF`）

| 偏好 | 取值 | 落到哪 | 效果 |
| --- | --- | --- | --- |
| 侧栏 | `expanded`（默认）/ `compact` | `body.nav-compact` | 只留图标、宽度收到 68px；鼠标停上去浮出名称 |
| 顶栏按钮文字 | `off`（默认）/ `on` | `body.bar-labels` | 默认只有图标，悬停浮出文字 |
| 顶栏样式 | `circle`（默认）/ `capsule` | `body[data-bar-style]` | 各自独立圆形按钮 / 整组收进一枚胶囊 |
| 顶栏形式 | `auto`（默认）/ `bar` / `float` | `body[data-bar-shape]` | 跟随皮肤自带形状 / 吸顶通栏 / 悬浮浮岛；与皮肤正交，换皮肤只换配色（见 5.1 节） |
| 侧边栏版式 | `arc`（默认）/ `card` / `rail` / `orb` | `body[data-nav-style]` | 弧形导航（无玻璃底板，圆形实体图标沿内凹弧线排列）/ 信息卡片 / 悬浮导轨 / 圆形胶囊（无玻璃底板，圆形实体图标居中直列）；不影响数据驱动导航，与皮肤正交。历史 `pill` 自动迁移为 `arc` |
| 圆形按钮底色 | `20`–`100`（默认 `100`） | `body` 的 `--orb-button-fill`；值存 `pwb.prefs.orbOpacity` | 只在 `orb` 圆形胶囊侧边栏显示控件并生效；数值越低，按钮底色、描边与阴影越透明，图标/文字不透明度不变 |
| 图标风格 | `plain`（默认）/ `tile` / `abstract` / `readable` | `body[data-icon-style]` | 极简线稿 / 圆角方块 / 抽象高级符号 / 直观重绘图标；后两者会刷新 SVG 本体 |
| **每个分类的图标样式** | `plain` / `tile` / `abstract` / `readable`，未设过＝跟随全局 | **逐分类**写在 `.nav-icon[data-icon-style]`；覆盖表存 `pwb.prefs.iconStyles`（`{分类key: 档位}`） | 同一个侧栏里脚本可以是圆角方块、网址可以是直观重绘，互不影响；内置分类也能改 |
| **每个分类的图标本身** | 任意 `PICKER_ICONS` 里的图标名，未设过＝按 `categoryIcon()` 自动解析 | 覆盖表存 `pwb.prefs.iconNames`（`{分类key: 图标名}`）；**不写 DOM 属性**，由 `categoryIcon()` 的第 ⓪ 段生效 | 在设置面板那一行点图标就能换（60 枚里挑，带名字筛选）；「回到自动」清掉这条覆盖。内置分类也能改 |

契约要点：

- 存在 `localStorage` 键 `pwb.prefs`。**组件规则里不写任何 JS 判断**——只认上面这些钩子，外观全在 `M4-CSS`。
- 圆形胶囊的底色用 `color-mix()` 结合 `--orb-button-fill` 绘制，**不要改 `.nav-link` 的整体 `opacity`**：那会把图标与文字一起淡化，违背「背景可透、内容清晰」的交互约定。滑块的合法值为 20–100，读取旧偏好或异常值时夹回这个范围；非圆形胶囊版式隐藏该控件。
- **逐分类覆盖的存在性由 DOM 属性表达**：设过的分类，其 `.nav-icon` 上带 `data-icon-style="<档位>"`；没设过的**不带这个属性**，于是只吃 `body[data-icon-style]` 的全局规则。这正是「跟随全局」不需要额外状态的原因，也是**行级钩子必须挂在 `.nav-icon` 而不是 `.nav-link`** 的原因——挂在行上会让 `[data-icon-style="x"].nav-link.active` 这类规则把「这个图标是什么风格」误当成「这一行被选中」。
- 关键函数（都在 `M4-MAIN`，`M4-PREF` 通过 `window.PWB_ICONS` 调用，**不要各自实现一份**）：`categoryIconStyle(category)`（自己的优先，否则全局）、`writeIconStyle(key,style)` / `writeIconName(key,name)`（空值或非法值＝删除那条覆盖，两者共用一个 `writeIconPref(field,key,value)`）、`readIconNames()`（分类 key → 用户挑的图标名）、`iconSvg(name,style)` / `iconInto(box,name,style)`（显式档位优先）、`refreshIconStyleRows()`（只重绘侧栏各行的图标，不重建导航）。`window.PWB_ICONS` 暴露 `styleOf` / `overrideOf` / `setStyle` / `overrideNameOf` / `iconOf` / `setIcon` / `iconNames` / `refreshRows` / `refresh` / `labels`。
- ⚠️ `refreshIconStyleRows()` 里图标名必须**用 `categoryIcon()` 重新算**，不能沿用 DOM 上旧的 `data-icon-name`——否则在设置面板换了图标，侧栏那一行会一直显示旧图标，直到下次整块重建导航。
- 设置面板的「每个分类的图标样式」一行一个分类：**分类名 + 当前图标按钮 + 四档样式**。点图标按钮弹出 `.icon-pop`（60 枚、带名字筛选、`is-auto` 标出「当前自动解析到的那一枚」），选一枚写 `pwb.prefs.iconNames`；弹层里的「回到自动」清掉这条覆盖。样式的四档按钮里那枚 `nav-icon` 就是**该分类自己那枚图标**在该档下的样子。**样式再点一次已选中的那一档＝取消覆盖、回到跟随全局**——所以样式没有单独的「跟随全局」按钮。那一档的覆盖值必须**在点击时现查**，不能在重建时存进闭包：设置面板重建相对点击是异步的，闭包里的值会过期，连点两次同一档就变成「再写一遍」而不是取消（2026-09-21 实测踩到）。弹层的开关状态同理，用闭包变量 `iconPopKey` 记「当前弹的是哪个分类」，**不要写进 DOM 再读回来**（再点同一按钮＝收起，靠这个判断）。
- **`.icon-pop` 必须挂在 `<body>` 上、用 `position:fixed`**（`z-index:26`：高于 `.modal` 的 10、低于拖拽幽灵的 60）。**不要**把它做成那一行内部的绝对定位元素——设置面板的滚动区是 `overflow:auto`，挂在里面会被裁掉（2026-09-21 实测：行在面板底部时弹层只露出顶部一条，下面的图标全被挡住）。位置由 `placePopover()` 按触发按钮的**视口坐标**现算：优先在按钮下方 `bottom+6`，下方可用高度不足且上方更宽裕时**向上翻**，最后把 `top`/`left` 夹在窗口内（左右用 CSS 里那个 `width:290px` 参与计算，改宽度要同步改 `POP_W`）。
- 打开弹层前先调 `revealRow()` **把那一行滚进设置面板的可视区**（滚动容器是 `.dialog-body`，见 5.4 节；行已在可视区内就不动，避免每次都跳），**然后**才定位——顺序不能反，滚动会改变按钮的视口坐标。这就是「点哪个分类就能直接改，不用先手动滚到那里」的机制。
- 因为浮层不跟着面板内容滚动，**面板滚动、窗口 resize、关面板时都要 `closeIconPopover()`**（滚动静默丢弃比让它错位飘着更好懂）。关面板那条写在 `closeSettings()` 里——浮层挂在 `body` 上，不会跟着面板一起隐藏。
- 新建分类弹窗（`#categoryModal`）的「图标样式」用的是同一个构造函数 `buildIconStyleSeg()`，建完分类**拿到 key 之后**才写 `pwb.prefs.iconStyles`（建之前没有 key 可用）。
- 悬停提示只在「文字已经被收起」的地方出现（紧凑侧栏、只显示图标的顶栏、设置面板里只画图标的图标风格按钮），展开状态下不弹重复信息；浮层是全局单例 `.hover-tip`，坐标走 CSSOM。设置面板那两处图标风格分段控件由 `buildIconStyleSeg(...,iconOnly=true)` 生成：按钮里只有图标，名称写在 `data-tip` 与 `aria-label` 上；`#categoryModal` 里的同一个控件保留文字。
- 设置面板 `#settingsModal`：皮肤网格取自 `window.PWB_SKIN.list()`，改任何一项立即生效并落盘，没有「保存」按钮；面板里的「同步文件夹 / 重新连接」走 `window.PWB_APP`。
- 入口有三处：顶栏 ⚙ 按钮、侧栏底部 ⚙ 按钮（`[data-action="settings"]`）、皮肤菜单仍保留（快捷换皮肤）。侧栏底部的折叠按钮（`[data-action="toggle-nav"]`）直接切 `expanded` / `compact`。

> ⚠️ **逐分类的图标与样式为什么不进 `catalog.json`**：两端后端都把 `category.symbol` 截到 2 字符（`server.js` 的 `normalizeCategory` 与 `main.rs`，见 [INTERFACES.md 6.5](../INTERFACES.md#65-分类-symbol-只存得下-2-个字符低风险待办)），`'tile'` 存进去会变成 `'ti'`、`'shield'` 变成 `'sh'`。它们与皮肤、品牌头像、界面偏好同属「本端界面偏好」，因此沿用 `pwb.prefs`，不进数据契约。**代价是浏览器版与桌面版各自记各自的选择**；要跨端同步得先解决 `symbol` 的长度限制（属 `M0` 变更，需同时改 `M1`/`M3`）。

### 5.7 品牌标与自定义头像

顶栏与侧栏左上角共用 `.brand-mark`：

- 默认是 M4 设计的「个」字几何标（SVG，描边 `currentColor`，背景跟 `--brand` 渐变，随皮肤变化）。
- 点击任意一处 `.brand-mark`、或在设置面板点「上传头像 / Logo」，可把本地图片设成头像；图片经 `shrinkImage()` 压到最长边 256px 后存 `localStorage`（键 `pwb.brand`），不落 `data/`、不走后端。
- 上传后两处 `.brand-mark` 会同时切换；刷新页面自动恢复。
- 在设置面板点「恢复默认图标」可回到「个」字标。
- 自定义头像用 `<img class="brand-photo">` 显示，CSS 用 `object-fit:cover` 铺满并跟随 `border-radius` 裁剪；CSP 已放行 `data:` 图片，桌面端正常显示。
- 顶栏 `.brand` 带 `data-tauri-drag-region`，但 `.brand-mark` 带 `role="button" tabindex="0"`，Tauri 2 会把它识别为可点击元素而挡住窗口拖动，因此点击它不会误触发拖拽（已验证 Tauri 2.11.5）。

---

## 禁止事项

- ❌ **不要把单文件拆成多文件**，也不要引入 CDN、外部字体或外部图标库。仅允许 `nocturne-rose-landscape.png` 作为与页面同目录、随桌面端打包的背景资源；`app/index.html` 必须能双击直接打开。
- ❌ **不要在组件样式里硬编码色值**，配色一律走 CSS 变量（见第 5 节）；新皮肤优先只写 `vars` 覆盖，不要用 `!important` 到处覆盖组件规则。
- ❌ **不要在皮肤里无条件写 `body[data-skin="xxx"] header` 改顶栏形状，也不要用 `--bar-shape-*` 覆盖形状**。皮肤自带的形状只能挂在 `body[data-bar-shape="auto"][data-skin="xxx"]` 上（见 5.1 节），否则用户选「吸顶通栏 / 悬浮浮岛」时形状会被皮肤顶住，形式开关形同失效。
- ❌ **不要在 HTML 标记或 `insertAdjacentHTML` 注入的字符串里写 `style="..."`**，桌面端 CSP 会拦掉（见 5.2 节）；也不要运行时 `createElement('style')`。动态样式走 `el.style.xxx` 或 CSS 变量。
- ❌ **不要把 GET 参数塞进 `options.body`**，要用 `options.query`；否则浏览器端不拼 query、桌面端 `input` 为空，会静默失效或 404（见第 1 节）。
- ❌ **不要用 HTML5 drag-and-drop 重写排序**（见上表，会失效）。
- ❌ **不要绕过 `request()` 直接用 `fetch` 或 `invoke`**，那会破坏双通道兼容。
- ❌ **不要在 `M4-DRAG` 里读取业务数据或调用后端接口**。它只处理 DOM 与调用 `persistOrder()`。
- ❌ **不要改 `.icon-card` / `data-id` / `.icon-grid#frequent` 这三个约定**而不改另一个脚本块。
- ❌ **不要为了「图标风格 / 侧栏版式 / 顶栏样式 / 顶栏形式 / 侧栏收起文字」在 JS 里逐个改样式**。只往 `<body>` 写 `data-icon-style` / `data-nav-style` / `data-bar-style` / `data-bar-shape` / `nav-compact` / `bar-labels` 这六个钩子，外观交给 `M4-CSS`；图标本体需要切换时调用 `window.PWB_ICONS.refresh()` 统一刷新。
- ❌ **不要给 `<svg>` 漏掉 CSS 尺寸**。SVG 不加约束时按 `300×150` 布局，会把侧栏/顶栏整个撑坏；新增图标容器时照 5.5 节那份清单补一条规则。
- ❌ **不要在带 `data-tip` 的元素上再写 `title`**，否则原生提示和浮层会一起冒出来。
- ❌ **不要把分类 `symbol` 当纯字符用**。新写数据要传图标名（见 5.5），前端虽然兼容旧的字符符号，但那是过渡兼容而不是目标形态。
- ❌ **不要在前端直接读写 `data/`**（浏览器环境本来就做不到，桌面端也别绕过 IPC）。
- ❌ **改完记得区分生效方式**：浏览器版刷新即可；**桌面版必须重新编译 exe**（`build-client.cmd`）。

越界时应怎么办：前端需要新数据时，不要自己去 `data/` 找，而是先看 [http-api.md](http-api.md) / [tauri-ipc.md](tauri-ipc.md) 找到现成接口；如果确实没有，就当作**接口新增**处理（先写文档、再改后端、最后改前端），并说明影响到了哪个模块。

---

## 变更记录

| 日期 | 变更 |
| --- | --- |
| 2026-09-21 | **精简侧栏与新增入口**（只改 `M4`，数据和两端接口不变）：移除侧栏与内容区的「待补充」展示，以及侧栏底部的收起/展开文字按钮；未修改、删除或读取任何真实条目。新增内容改为先经 `#createCategory` 选择目标分类，创建请求使用该分类的 key、字段组按其既有 `kind` 自动切换，因此包括「密码」在内的自建分类都能直接新增内容。 |
| 2026-09-21 | Nocturne · 玫瑰圣殿皮肤新增本地背景资源 `app/nocturne-rose-landscape.png`：用实际粉雾山景、远山和新古典拱廊取代 CSS 渐变拼图。该文件与 `app/index.html` 一同由桌面端 `frontendDist` 打包；除它以外，M4 仍保持单文件和无外部资源约束。 |
| 2026-09-21 | **设置面板的图标风格四档改为「只画图标」**（只改 `M4` 的 `buildIconStyleSeg()` 与两处调用，CSS、数据契约、两端后端零改动）。起因：四档风格名（极简线稿 / 圆角方块 / 抽象符号 / 直观重绘）在每个分类那一行各出现一次，设置面板里同四组字叠着排，观感很乱。① `buildIconStyleSeg(box,current,onPick,withPreview,iconOnly)` 新增第 5 个参数 `iconOnly`：为真时按钮里**只放预览图标**，不写名字，改为落 `data-tip`（悬停浮层，见 5.6 的悬停提示）与 `aria-label`（无障碍），读屏与鼠标悬停都还能拿到名称。② 调用点：设置面板的全局 `#iconStyleSeg`（`syncForm()`）与「每个分类的图标样式」逐行（`renderPerCategory()`）传 `iconOnly=true`；**新建分类弹窗 `#categoryModal` 的 `#categoryStyleSeg` 保持带字**——那里是首次建分类、没有既视感，文字更好认。③ 静态标记 `#iconStyleSeg` 的四个按钮清空文字、补 `data-tip` / `aria-label`（打开面板时本来就会被 `buildIconStyleSeg()` 整块重建，这里只是让标记与最终渲染一致）；「图标风格」区补一行 tip 说明悬停可看名字。④ 分段控件的 `data-iconstyle` 值、`on` 选中态判定、覆写逻辑全部未变，`M4-CSS` 的 `.segmented button` 规则也不需要改（`flex:1` 让四格等宽、图标居中）。 |
| 2026-09-21 | **合并包面板精简**：去掉展开面板顶部的包名称与说明文字（`#pkgPanelTitle` / `#pkgPanelHint`），去掉底部操作按钮（`#pkgRename` / `#pkgDissolve` / `#pkgPanelDone`），重命名与删除改由面板空白处右键菜单提供；包卡片右键菜单加 `stopPropagation()` 避免与面板右键冲突。DOM 契约表同步删除五个已废弃 ID。 |
| 2026-09-20 | 首次编写。冻结 `request()` 双通道适配层、DOM 契约（28 个 ID + 4 组属性约定 + `persistOrder` 跨块函数）、渲染分流规则、拖拽实现方式与三条样式类约定。 |
| 2026-09-20 | 新增 `M4-SKIN` 皮肤子模块（第三个 script 块之前为 `M4-DRAG`，现为：MAIN → SKIN → DRAG）：`SKINS` 注册表 + CSS 变量注入 + `localStorage`（键 `pwb.skin`）+ 顶栏切换菜单（`skinToggle`/`skinMenu`）。CSS 全面变量化（新增 `--line-strong` `--paper-solid` `--paper-hover` `--brand-ink` `--brand-line` `--on-brand` `--soft` `--soft-ink` `--ok` `--ok-line` `--ok-bg` `--scrim` `--blur` `--panel-border` `--panel-shadow` `--radius*`），内置皮肤：经典云白（兜底）/ 液态玻璃·暖阳（默认）/ 暗夜玻璃。布局调整：顶栏改吸顶玻璃条、侧栏顶部加品牌标、状态条改胶囊、导航激活态改实心填充。`M4-MAIN`/`M4-DRAG` 脚本块未改动。 |
| 2026-09-20 | 顶栏跟随皮肤：新增 `--bar-bg` / `--bar-border` / `--bar-shadow` 三个变量（`:root` 白色兜底，玻璃皮肤为带色调的磨砂渐变条），顶栏 backdrop-filter 饱和度提到 1.6，玻璃皮肤背景光斑增加顶部一枚（让吸顶条后有颜色可透）。配合 M3 的无边框窗口（`decorations:false`）新增自绘标题栏：`.brand`/`.drag-space` 承担 `data-tauri-drag-region` 拖拽，`#winMin`/`#winMax`/`#winClose` 三个窗口按钮仅 `body.is-desktop` 显示，窗口控制由 `M4-MAIN` 尾部的 `window.__TAURI__.window` 接线。详见 5.1 节。 |
| 2026-09-20 | 滚动条跟随皮肤：新增 `--scroll-thumb` / `--scroll-thumb-hover` / `--scroll-track` 三个变量，`::-webkit-scrollbar` 改为 12px 细胶囊滑块（`background-clip:padding-box` 内缩），并留 `scrollbar-color`/`scrollbar-width` 兜底；弹窗、开始菜单选择器等内部滚动区自动继承。 |
| 2026-09-20 | **界面改为完全数据驱动 + 编辑器按类型分流**（只重写 `M4-CSS` 与 `M4-MAIN`，`M4-SKIN`/`M4-DRAG` 两块原样保留）：侧栏与内容区块由 `state.categories` 生成，去掉写死的 `#scripts`/`#links`；编辑器改用 `#kindSwitch` 分段切换 + `.kfields[data-kind]` 四组字段，首屏只留必填项，「说明/标签」折叠进 `<details>`；新增开始菜单选择器（`#picker`，走 `/shortcuts` + `/shortcuts/icon`）、新建分类弹窗（`#categoryModal`）、待补充归类（`#assignBlock`，可内联新建分类）；`request()` 明确 GET 用 `options.query`、POST 用 `options.body`，`IPC_MAP` 扩到 14 条；图标占位由 `▣`/`↗` 符号改为字母头像（`paintLetter()`）。修复两处桌面端 CSP 拦掉的内联 `style` 属性，新增 5.2 节。 |
| 2026-09-21 | **新增「自定义外观」动态皮肤（M4-SKIN）**：`SKINS` 注册表加 `custom` 项（`vars` 留空，由 `customVars()` 按用户设置现算）；新增 `#skinCustom` 面板——导入壁纸 + 图片/蒙层透明度、图片模糊、面板不透明度与毛玻璃、底色基调（浅/深）、主色调，改动即时生效并落盘 `localStorage`（键 `pwb.skin.custom`，滑块 300ms 防抖）；壁纸经 canvas 压到 ≤1920px JPEG 后存 data URL，超存储自动降级 1440/1024。CSS 新增 `--custom-image` / `--custom-image-alpha` / `--custom-image-blur` / `--custom-veil` 四个变量（`:root` 有兜底）与 `body[data-skin="custom"]::before/::after` 背景层、range/color/预览/面板样式、通用 `button:disabled`。M4-MAIN / M4-DRAG 未动。新增 5.3 节与 DOM 契约面板 ID 行。 |
| 2026-09-21 | **分类网格的位置只由 `order` 决定**（点击不再换位）：`sortItems()` 去掉 `score()` 兜底——旧实现是「两两比较时双方都有 `order` 才比 `order`，否则比 `score()`」，而 `score()` 含打开次数与距上次打开天数，于是点一下图标它就会跳到分类里的第一位。新版为「有 `order` 的按 `order` 升序 → 缺 `order` 的按后端返回的原始顺序跟在后面 → 缺 `order`/`order` 撞号用原始下标兜底」，排序键不含任何随点击或时间变化的量。`score()` 保留，仅供 `#frequent` 常用区排序。同步修正本节 `#frequent` 取前 8（原写 6）。 |
| 2026-09-21 | **界面改版：内置图标集 + 侧栏可折叠 + 设置面板 + 两套新皮肤**。① `M4-MAIN` 新增 `ICONS` 注册表（40 枚手写 24 视口线性图标）与 `iconSvg()` / `iconInto()` / `hydrateIcons()` / `categoryIcon()` / `buildIconPicker()`；侧栏与类型切换全换成 SVG 图标，分类图标按「图标名 → label 关键词 → kind 默认」逐级解析（旧的字符型 symbol 不再上屏），分类编辑器里的「符号」输入框改成图标选择器（选中值仍发到 `symbol`）。② 顶栏按钮改成图标按钮、文字默认收起（悬停浮出），侧栏支持「只显示图标」（宽 68px）。③ 新增 `M4-PREF` 子模块：四组偏好存 `pwb.prefs`、设置面板 `#settingsModal`、全局悬停提示 `.hover-tip`；`M4-MAIN` 暴露 `window.PWB_APP`，`M4-SKIN` 暴露 `window.PWB_SKIN` 并广播 `pwb:skin` 事件。④ 新增两套皮肤 `graphite-dusk`（深色简约 B 端）与 `sakura-mist`（樱粉柔光），二者另定义 `--bar-btn-*` / `--nav-tile-*`；`classic` 的 `vars` 补上几个键，好让设置面板能画出色卡。⑤ 新增变量 `--bar-btn-*` `--nav-tile-*` `--nav-w` `--tip-*`；组件样式零硬编码色值的前提不变。 |
| 2026-09-21 | 新增 `folder` 图标（24×24 描边，`fill:none` + `stroke:currentColor`），并在 `LABEL_ICONS` **最前面**插入 `/文件夹|目录/ → folder`，让内置的「文件夹」分类不再跟「文档」共用 `file` 图标——该表按顺序取第一个命中，这条规则必须排在 `/文档|资料|笔记|文件/` 之前，否则会被「文件」二字抢先匹配。`categoryIcon()` 与渲染代码未改动。 |
| 2026-09-21 | **品牌标改版 + 支持自定义头像**：把顶栏/侧栏的「工」字换成「个」字几何标（SVG），新增 `brandFile` 与 `.brand-mark` 交互。默认点击品牌标可上传自己的照片 / Logo，图片经 `shrinkImage()` 压到 256px 后存 `localStorage`（键 `pwb.brand`），两处标记同步切换、刷新后自动恢复；设置面板新增「上传头像 / Logo」与「恢复默认图标」按钮。新增 `window.PWB_BRAND` 跨块契约与 `--brand-glyph` 变量，CSS 适配 46px / 38px / 36px 三处尺寸；文档更新 5.1 / 5.7 节与 DOM 契约。 |
| 2026-09-21 | **删除顶部状态条**（`<main>` 里的 `<div id="status" class="status">`）：连接结果、同步结果、品牌图报错原先都写在这条上，删除后统一改由新增的 `setStatus(text,bad)` 写进设置面板的 `#settingsStatus`（类名保持 `tip` / `tip bad`，不覆盖成原来那套 `status ok` / `status bad`）。注意：`connect()`、`scanFolders()`、`brandNotice()` 里原有 6 处 `$('status')` 未做空值保护，**只删元素必然出问题**——`connect()` 会在赋值处抛 `TypeError`，被自己的 `catch` 接住后又在 `catch` 里抛第二次，其后的 `loadItems()` 再也不执行，首屏直接空白；所以元素与这三处写入必须同一次改完。DOM 契约表里的「状态条 `status`」一行随之删除，「数据区 `settingsStatus`」成为唯一的状态文案出口。`.status`/`.status.ok`/`.status.bad` 三条 CSS 规则与 `paintStatus()` 已成死代码，本次保留未动。 |
| 2026-09-21 | **修复深色皮肤下原生下拉「空白、选不中」**：`select` 的 `option` 未设底色时其 `background` 是 `rgba(0,0,0,0)`，弹出列表回退到系统浅色底（白），而文字色继承皮肤的 `--ink`——暗夜玻璃（`#e8ebf8`）/ 石墨暮色（`#eef3f9`）下即为白底白字，「用哪个浏览器打开」等下拉看着是空的、任何一项都点不着；浅色皮肤文字是深色所以一直没暴露。`M4-CSS` 新增 `select option{background:var(--paper-solid);color:var(--ink)}`，一条规则覆盖全部原生下拉（网址的浏览器选择、归类弹窗的分类选择、开始菜单的分组筛选）。通用约定补进第 5 节。 |
| 2026-09-21 | **侧栏分类支持拖动排序**：`renderNav()` 给数据驱动分类加 `.category-link[data-category-key]`，`M4-DRAG` 用 Pointer Events 提供跟手幽灵与插入动画；总览、常用、待补充不带该类，保持固定。结束后 `persistCategoryOrder()` 串行调用既有的 `/categories/update` 更新 `Category.order`，避免并发写索引；补齐 IPC 映射清单中的分类更新/删除与浏览器探测条目。 |
| 2026-09-21 | **侧边栏与图标视觉扩展**：`M4-PREF` 新增 `navStyle` 偏好与 `#navStyleSeg`，在信息卡片（默认）、柔和胶囊、悬浮导轨三种侧边栏构图间切换，可与只显示图标的紧凑态叠加；`iconStyle` 从两种扩为四种，新增柔光胶囊与立体圆形。两个偏好均只通过 `body[data-nav-style]` / `body[data-icon-style]` 驱动 CSS，继续不影响数据、拖拽、DOM 导航契约。 |
| 2026-09-21 | **侧边栏新增「圆形徽章」版式（`orb`）**：`navStyleSeg` 新增一档，`body[data-nav-style="orb"]` 驱动——侧栏收成一列 48px 圆形按钮，默认只显示图标，悬停时右侧浮出文字标签（`.nav-text` 改 `position:absolute`），激活态为实心品牌色圆；与皮肤正交，用户可在任意皮肤下自由切换。`nocturne-rose` 皮肤的 `.sidebar`/`.nav-link`/`.nav-add` 圆角规则加 `:not([data-nav-style="orb"])` 守卫，避免覆盖 orb 的圆形按钮。同步更新玫瑰圣殿背景：`::before` 改为梦幻粉紫山峦渐变（月晕 + 远山 + 天空渐变），`::after` 改为古典柱廊效果（左右对称竖向渐变 + 顶部拱门弧线 + 柔焦蒙层）。DOM 契约表 `body[data-nav-style]` 一行同步补上 `orb`。 |
| 2026-09-21 | **液态玻璃与图标本体升级**：玻璃卡片从单层半透明改为变量驱动的表面高光、边缘折射、内部雾感和深度阴影（新增 `--glass-*` 变量）；图标风格中的 `soft/solid` 迁移为 `abstract/readable`，分别提供抽象高级符号与直观重绘图标两套 SVG 路径，切换时由 `window.PWB_ICONS.refresh()` 重新填充已有 DOM。 |
| 2026-09-21 | **`rail`（悬浮导轨）版式的选中态不再套深色底板**：原规则 `body[data-nav-style="rail"] .nav-link.active{background:var(--paper-solid);border-color:var(--brand-line);box-shadow:var(--panel-shadow)}` 在**紧凑侧栏**下会画出「套住图标的一圈深色椭圆框」——`.sidebar{align-items:center}` 让 `.nav` 收缩到 fit-content，`.nav-link` 于是只有 **32px 宽 × 48px 高**，`--paper-solid` 底板 + `14px` 圆角看起来就是一个椭圆（暗夜玻璃下实测该颜色正是 `#1c2038`）。现改为 `{background:transparent;color:var(--brand-ink)}`：**只保留变色**（`tile` / `abstract` / `readable` 三种图标风格本来就自带底色，`plain` 靠图标与文字变品牌色）。已用无头 Chrome 复现「暗夜玻璃 + rail + 紧凑 + tile」核对：`.nav-link` 背景 `rgba(0,0,0,0)`、边框透明、无阴影，图标底色仍是 `--brand`。 |
| 2026-09-21 | **侧边栏收敛为弧形导航**：设置项删除与信息卡片重复的 `pill`（柔和胶囊），并以 `arc`（弧形导航）替换旧的圆形徽章。弧形导航移除 `.sidebar` 的磨砂玻璃、边框、阴影和伪元素高光；圆形图标改落在 `--paper-solid` 实体按钮上，导航容器右边以带圆角的内凹弧线定义轮廓。默认偏好改为 `arc`，旧 `pill` 自动迁移为 `arc`；信息卡片仍可在设置中切回。 |
| 2026-09-21 | **恢复「圆形胶囊」版式**：`orb` 作为 `navStyleSeg` 的独立选项恢复，沿用居中直列的圆形按钮构图；为满足移除玻璃侧栏的视觉要求，它与 `arc` 一样取消 `.sidebar` 的玻璃底板、边框和阴影，按钮使用 `--paper-solid` 实体纸面。Nocturne 皮肤的侧栏玻璃规则同时排除 `arc` 与 `orb`。 |
| 2026-09-21 | **合并包改为「长在分类里」**（纯 `M4` 展示层改动，数据契约与两端接口不变）。原先建包会新占一个侧栏分类（于是出现「合并包 / 合并包 2」两行、而「应用」是空的）。现在：① 侧栏与内容区块改用新增的 `sectionCategories()`（= `visibleCategories()` 去掉包），包不再占侧栏；② 新增 `packageHostKey(kind)` / `packagesIn(key)`，包以 `.pkg-card` 文件夹卡渲染在**同类内容的宿主分类**网格最前面（应用包出现在「应用」下），宿主分类计数含包内成员；③ `state.view` 不再取包的 key——`buildPackage()` 建完只写状态栏，包面板改由 `state.pkgKey` 独立驱动，删掉「进入这个包才平铺成员」那条分支与 `.pkg-note` 提示（CSS 规则一并删除）；④ 「解散这个包」统一改称**「删除这个包」**（包卡片右键、`#pkgPanel` 底部），并在右键菜单里去掉危险的「删除分类」；⑤ 新增 `window.PWB_MERGE.intoPackage(pkgKey, itemId)`：把条目卡拖到**包卡片**上松手 = 放进这个已有包（`placeAt()` 把包卡片单列为落点、但仍不参与排序参照），补上「包不在侧栏之后无法往已有包里加内容」这个缺口；⑥ `persistOrder()` 与 `M4-DRAG` 的 `orderOf()` 改为只取 `.icon-card[data-id]`，避免包卡片混进网格后把空 id 提交给 `reorder`。 |
| 2026-09-21 | **顶栏形式与皮肤解耦**：顶栏的「形状」原先写死在皮肤规则里（`body[data-skin="nocturne-rose"] header` 及配套的 `main` / `.sidebar` 偏移），换皮肤就会连形状一起被换掉。现抽成独立偏好 `barShape`（`M4-PREF`，存 `pwb.prefs`，钩子 `body[data-bar-shape]`，设置面板「顶栏」区新增 `#barShapeSeg` + `#barShapeTip`）：`auto` 跟随皮肤自带形状（兼容旧观感）/ `bar` 吸顶通栏 / `float` 悬浮浮岛，**配色仍由皮肤变量决定，形式与皮肤正交**。新增 8 个 `--bar-shape-*` 变量（`:root` 中性兜底，只在 `float` 档生效）；`nocturne-rose` 的月牙顶栏改为**只在 `auto` 档生效**的 CSS 规则，用户选 `bar` / `float` 时它自动让位。⚠️ 皮肤形状**不能**写进 `--bar-shape-*` 变量（第一版就是这么写的：变量挂在 `<html>` 上是全局值，会把 `float` 的通用浮岛又按回月牙，实测 `auto` 与 `float` 截图完全一致、等于没解耦）；窄屏 `float` 为 `calc(100% - 28px)` + 四角 18px。设置面板原「顶栏按钮」块改名「顶栏」，拆成按钮文字 / 按钮排列 / 顶栏形式三组，并新增 `.set-sub h3` 小标题样式。详见 5.1 / 5.6 节。 |
| 2026-09-21 | **新增「合并包」：把几条内容收进一个像手机文件夹一样的包**（只改 `M4`，**两端后端零改动**）。数据上「包」就是一个普通分类（`builtin:false`、`kind` 与成员一致），建包时约定写 `symbol: 'pk'`（正好 2 个字符，能穿过两端 `normalizeCategory` 的截断）+ `note: '合并包 · 点开查看内容'`，前端 `isPackage()` 据此认包。① **建包** = `POST /categories` + 逐条 `POST /items/metadata` 改挂 `category`（逐条 await，避免并发写 `catalog.json`；`metadata` 会整段覆盖 `description`/`tags`，所以移动时把条目现有值原样回传——这点同步写进了 http-api.md）；对非收件箱条目 `metadata` 不搬磁盘文件，于是「应用」包里的 `.lnk` 仍留在 `data/应用/`，`/items/open` 完全不受影响。② **渲染**：不是当前视图时包只露一张 `.pkg-card`（42px 方块里 2×2 铺最多 4 枚成员图标），点开是 `#pkgPanel`；进入包里（`state.view === key`）则照常平铺成员，`renderNav()` 的点击因此改调 `render()`。③ **多选模式**：顶栏新增 `#multiSelect`（`layers` 图标）+ 底部 `#bulkBar`，由 `body.multi-select` 驱动；多选下卡片点击只切换 `.picked`，`M4-DRAG` 在此期间不启动拖拽。④ **拖拽合并**：`M4-DRAG.placeAt()` 里指针落在同一网格另一张卡的中间区域（四边各让出 `min(14px, 20%)`）即为合并目标，高亮 `.merge-target` 且**不再改排序**，松手由新增的跨块契约 `window.PWB_MERGE.fromDrag()` 用默认名直接建包（拖拽＝自动合并，不弹命名框）；落在卡片边缘或卡片之间仍是老规矩插空排序。⑤ **重命名 / 解散**复用既有接口：`/categories/update`、成员退回同类内置分类后的 `/categories/delete { removeFolder: true }`——**必须带 `removeFolder`**，否则留下的空目录会被 `readCatalog()` 重新收编成一个分类。⑥ 已知限制：一个包里只能放同类内容（怎么打开由所属分类的 `kind` 决定，混类型会让网址之类的条目打不开，要根治得给 `Item` 加 `kind`，属 `M0` 变更）；`symbol` 只存得下 2 字符，所以合并包统一用文件夹图标、暂不支持自选图标。 |
| 2026-09-21 | **修复「设置面板等内容多的弹窗被压扁、且滚轮无效」**（只改 `M4-CSS` 一条 `.dialog-body` 规则，`M4-MAIN` / `M4-SKIN` / `M4-DRAG` / `M4-PREF` 与两端后端零改动）。现象：窗口一矮，设置面板的每个 `.set-block` 只剩标题一条（900×560 实测 `clientHeight` 53px、`scrollHeight` 579px），里面的皮肤网格/分段开关溢出到下一个区块上、再被 `.dialog` 的 `overflow:hidden` 裁掉，而 `scrollHeight === clientHeight` 让滚动条永远不出现——用户看到的就是「有些项只加载一部分、全挤在一页」。根因：`.dialog-body` 既是 `.dialog` 的 flex 子项、又是 `overflow:auto` 的滚动容器，此时它的 `min-height:auto` 按 **0** 算而被压到内容高度以下，且它是 `display:grid`，默认 `auto` 网格行允许收缩到内容高度以下，行跟着塌陷。修法：`.dialog-body` 加 `min-height:0`（收缩许可，**单独加无效**，实测几何数值不变）与 `grid-auto-rows:min-content`（**这条才是关键**，把行锁在内容高度，`scrollHeight` 于是大于 `clientHeight`，滚动条正常出现、滚轮到得了底），另加 `overscroll-behavior:contain` 防止滚动传递到背后页面。修在通用规则上，设置面板 / 自定义外观 / 条目编辑器 / 新建分类 / 合并包命名一起受益；已在无头 Chrome 复测 1280×800 / 1280×720 / 1000×620 / 900×560 / 760×500 / 1280×400 六档窗口与四个弹窗（区块不再被压扁、`canScroll` 为真、程序化滚动 `scrollTop` 直达上限 1247、最后一个区块完整可见、弹窗始终在视口内、无控制台报错）。新增 5.4 节。 |
| 2026-09-21 | **图标改成「按形态区分」而不是「按关键词挑相近图形」**。起因：内置五类里「脚本」和「文档」的图标语义相近、细看几乎同形，且切换 `abstract/readable` 时大量图标会回落成同一份线稿，观感上「切风格＝只换颜色」。本次只动 `ICONS` 与三张配套表，**不新增后端接口、不改 DOM 契约、不改拖拽**。① `LABEL_ICONS` 最前面插入七条形态映射（概览→`stack`、常用→`flame`、待整理→`folder-question`、密码→`keyhole`、网址→`pin`、应用→`puzzle`、文档/文件→`book-open`），并在原「文档」那条上加了 `(?!夹)` 负向断言——**这张表是按顺序取第一个命中的**，内建分类的 `symbol` 是 `▣ ↗ ◈ ▤` 这类旧字符（`hasIcon` 不认），实际生效的就是这里；不加断言「文件夹」会被「文件」二字抢先匹配成 `book-open`，跟内建「文件」分类撞脸。② `renderNav()` 里「全部内容 / 常用」两行的写死图标由 `grid / star` 换成 `stack / flame`（grid 会和「应用」的拼图撞、star 同时是「收藏」的语义）。③ 设置面板的 `DEMO_ICONS` 换成形态图标当样本，因为它三档下是三套不同路径，能在设置面板里立刻看出风格是否真的生效。 |
| 2026-09-21 | **图标样式改为「每个分类各挑各的」**（只改 `M4`，两端后端零改动、数据契约不变）。原先「图标风格」是一个全局档，切一次全站一起变；现在每个分类可以单独覆盖。① 存储：覆盖表放在 `localStorage` 的 `pwb.prefs.iconStyles`（`{分类key: 档位}`，`M4-MAIN` 读写、`M4-PREF` 写入），**不进 `catalog.json`**——两端后端都把 `symbol` 截到 2 字符，`'tile'` 会被截成 `'ti'`（见 5.6 节的说明与 [INTERFACES.md 6.5](../INTERFACES.md#65-分类-symbol-只存得下-2-个字符低风险待办)）。② DOM 钩子：行级 `data-icon-style` **挂在 `.nav-icon` 上**（不是 `.nav-link`），属性值与全局钩子完全相同，`M4-CSS` 里每个变体写两遍选择器（`body[data-icon-style="x"] .nav-icon` 与 `[data-icon-style="x"].nav-icon` 及其 hover/active 变体）；设过的分类带这个属性、没设过的不带（＝跟随全局），所以「跟随全局」不需要额外状态。③ 渲染：`iconSvg(name,style)` / `iconInto(box,name,style)` 支持显式档位；新增 `categoryIconStyle()` / `writeIconStyle()` / `refreshIconStyleRows()`；`renderNav()` 逐行落 `data-icon-style`；`window.PWB_ICONS` 扩成 `{refresh, styleOf, overrideOf, setStyle, refreshRows, labels}`。④ 界面：设置面板新增「每个分类的图标样式」区块（`#perCategoryStyles`）——一行一个分类、四个按钮＝四档，按钮里的预览用的就是**该分类那枚图标**在该档下的样子，换图标时四格一起换；**再点一次已选中的那一档＝取消覆盖、回到跟随全局**（没有单独的「跟随全局」按钮）；新建分类弹窗（`#categoryModal`）也加了 `#categoryStyleSeg`，用同一个构造函数 `buildIconStyleSeg()`。⑤ 四档外观差异加大：`tile` / `abstract` / `readable` 的容器统一到 32×32，`abstract` 常驻**圆形**徽章（原先只有激活态是圆的）。⑥ 过程中修掉两个自作 bug，都写进了注释：`readIconStyles()` 里 `const map=saved&&typeof saved.iconStyles` 把「类型名字符串」当值用（它自己 `typeof` 也是 `'object'`，判断假通过 → 覆盖永远读不回来）；`activeIconVariant()` 只认字面量 `abstract\|readable`，遇到 `tile` 返回 `plain`（全局选圆角方块时，没单独设样式的分类会丢底色）。另外「再点一次取消」必须在点击时**现查**覆盖值，不能把重建时的快照存进闭包（连点两次会变成再写一遍）。 |
| 2026-09-21 | **图标选择器扩到 32 个「形态方案」**（只改 `M4`，两端后端零改动、DOM 契约与拖拽不变）。用户要的是「`assets/icons/category-icon-forms-preview.html` 里那几个都能挑」，那个预览页是 **8 个分类 × 每类 4 个物体** 的形态方案，所以本次把整套搬进图标库。① `ICONS` 新增 25 枚（另外 7 枚 `stack` / `flame` / `terminal` / `pin` / `puzzle` / `book-open` / `keyhole` 上一轮已有），命名按语义而非预览页的临时 id：`grid-board`（宫格底板）`drawer` `bundle` `bookmark` `rise` `brackets` `scroll` `file-gear` `chain` `external` `rocket` `case` `page-fold` `page-stack` `book-closed`（预览的「翻开的书」）`clipboard` `tray` `dashed` `file-spark` `key` `shield` 等。② **避坑两处**：预览页里「翻开的书」与库里既有的「书脊朝左的书本」是两枚不同的书，所以旧的那枚保留 `book`（`LABEL_ICONS` 里 `/学习\|课程\|读书/` 用它），新的那枚叫 `book-closed`；`layers` 是库内既有的层叠图标（顶栏「多选」按钮在用），**没有被新条目覆盖**。③ 新增 `ICON_FORMS` 常量（32 枚，按分类分组、组内顺序与预览页一致）与 `PICKER_ICONS=[...ICON_FORMS, …语义图标]`，选择器从 39 枚扩到 **60 枚**；顺带把一直不在选择器里的 `folder` 与 `book-open` 补上（它们是 `categoryIcon()` 关键词表的主力回落，以前用户看不到也挑不到）。④ 三张表的变体补齐：`ABSTRACT_ICONS` / `READABLE_ICONS` 各从 20 枚扩到 **60 枚**（选择器里的每一枚都有三档路径，切风格时形状真的会变）。⑤ ⚠️ **教训**：新增图标只写进 `ICONS` 而忘了 `PICKER_ICONS`，界面上就是「挑不到」，而 `npm run check` 查不出来——本次就是这么漏掉的，所以最终复核脚本里加了一条「库内图标（窗口控件除外）必须都在选择器里」的断言。 |（只改 `M4-MAIN` 的 `connect()`，`M4-CSS`/`M4-SKIN`/`M4-DRAG`/`M4-PREF` 与两端后端零改动）。起因是「工作台打开有点慢，是不是加载太多」。实测结论：**慢的是运行时，不是应用代码**——桌面版从点开 exe 到可交互 830–1120ms，其中 WebView2/Tauri 启动 550–800ms，页面自身只有 120–200ms（`ScriptDuration` 7ms / `LayoutDuration` 14ms）；浏览器版整页 `load` 27–84ms、内容渲染完 186–237ms。`GET /items` 响应 134KB、其中 129.9KB（96.7%）是图标 base64，但网络耗时中位数只有 3.4ms。对照实验（逐一去掉 `backdrop-filter` / 面板阴影 / 玻璃内阴影 / 皮肤背景层，各 3 轮）显示这些都不是主因：去掉全部 `backdrop-filter` 只省 ~24ms，去掉全部 `filter`+`box-shadow`+`background-image` 约省 8ms——**155KB 内联 CSS/JS 与玻璃模糊都不值得为它牺牲观感**。唯一落地改动：`connect()` 由「`await /status` → `await /items`」改为 `Promise.all` 并行，并用无头 Chrome 验证两个请求发起时刻相差 1–2ms（改前是严格串行）；冷启动 A/B 对拍（各 4 轮全新 profile）内容就绪中位数 **145ms（并行）vs 171ms（串行）**，省下一个往返约 25–35ms。上述预算写进第 1 节，测量脚本与原始数据留在 `tmp/perf/`（不入库）。 |
| 2026-09-21 | **首屏性能实测 + 启动请求并行化**（只改 `M4-MAIN` 的 `connect()`，`M4-CSS`/`M4-SKIN`/`M4-DRAG`/`M4-PREF` 与两端后端零改动）。起因是「工作台打开有点慢，是不是加载太多」。实测结论：**慢的是运行时，不是应用代码**——桌面版从点开 exe 到可交互 830–1120ms，其中 WebView2/Tauri 启动 550–800ms，页面自身只有 120–200ms（`ScriptDuration` 7ms / `LayoutDuration` 14ms）；浏览器版整页 `load` 27–84ms、内容渲染完 186–237ms。`GET /items` 响应 134KB、其中 129.9KB（96.7%）是图标 base64，但网络耗时中位数只有 3.4ms。对照实验（逐一去掉 `backdrop-filter` / 面板阴影 / 玻璃内阴影 / 皮肤背景层，各 3 轮）显示这些都不是主因：去掉全部 `backdrop-filter` 只省 ~24ms，去掉全部 `filter`+`box-shadow`+`background-image` 约省 8ms——**155KB 内联 CSS/JS 与玻璃模糊都不值得为它牺牲观感**。唯一落地改动：`connect()` 由「`await /status` → `await /items`」改为 `Promise.all` 并行，并用无头 Chrome 验证两个请求发起时刻相差 1–2ms（改前是严格串行）；冷启动 A/B 对拍（各 4 轮全新 profile）内容就绪中位数 **145ms（并行）vs 171ms（串行）**，省下一个往返约 25–35ms。上述预算写进第 1 节，测量脚本与原始数据留在 `tmp/perf/`（不入库）。 |
| 2026-09-21 | **设置面板里「一个分类一行，图标和样式都能改」**（只改 `M4`，两端后端零改动、数据契约不变）。用户指出「每个分类的图标样式」那段只能换风格、换不了图标本身。① 新增第二张逐分类覆盖表 `pwb.prefs.iconNames`（`{分类key: 图标名}`），与 `iconStyles` 共用同一个 `pwb.prefs` 键和一个公共写入 `writeIconPref(field,key,value)`（空值/非法值＝删除覆盖）；三张图标表、选择器都不动。② `categoryIcon()` 增加**第 ⓪ 段**：用户显式挑过的图标优先于 `symbol` / `icon` / 关键词 / `kind` 四段回落——它是用户的显式选择，理应在最前。③ `renderPerCategory()` 的行结构由「分类名 + 四档」变为「**分类名 + 当前图标按钮 + 四档**」：点图标按钮弹出 `.icon-pop`（`.per-cat-row` 内绝对定位），里面铺全部 60 枚（`buildIconPicker` 新增第 4 个参数 `list` 以便只铺筛选后的子集）、带名字筛选框、`is-auto` 虚框标出「当前自动解析到的那一枚」，另有「回到自动」清掉覆盖。两处入口都只改本端偏好。④ `refreshIconStyleRows()` 改为**用 `categoryIcon()` 重新算图标名**（原先沿用 DOM 上的 `data-icon-name`，换了图标侧栏不会变）、并同时更新 `data-icon-name`；`window.PWB_ICONS` 增 `iconOf` / `overrideNameOf` / `setIcon` / `iconNames`。⑤ 踩到并修掉一个自作问题：弹层的「再点同一按钮＝收起」原先靠回读 `pop.dataset.for` 判断，改成闭包变量 `iconPopKey`——开关状态放内存比写进 DOM 再读回来可靠（写回归测试时正是这一条先暴露出来）。⑥ 验证：写了一份假 DOM 冒烟脚本把 `M4-PREF` 原样跑起来（15 项断言：初始化、行结构、弹层、选中写入、收起、切行不叠加）全通过；另有 19 项覆盖链断言（读写容错、`categoryIcon` 七段优先级、四档渲染）全通过。 |
| 2026-09-21 | **图标选择器改为按分类分组**（只改 `M4`，两端后端零改动、数据结构不变）。用户指出 60 枚图标一维平铺在一起、看不出哪个属于哪类。① 新增 `ICON_GROUPS`：把原来一维的 `ICON_FORMS` 升级成**带组名与说明的分组数据**——9 组 = 8 个分类（概览 / 常用 / 脚本 / 网址 / 应用 / 文档 / 待补充 / 密码库，各 4 个形态方案，组内顺序与 `assets/icons/category-icon-forms-preview.html` 一致）+ 1 个「通用」组（28 枚语义图标）。`ICON_FORMS`（32 枚）与 `PICKER_ICONS`（60 枚）改为由 `ICON_GROUPS` 派生，仍是一维视图，供既有判断与文档引用使用。② `buildIconPicker()` 的四参由「要铺的数组」改为「名字筛选关键字」，并**按组渲染**：每组输出 `.icon-row`（图标排）+ `.icon-row-label`（组名 + 说明），容器 `.icon-pick` 从多列 grid 改为单列 grid；命中 0 枚的组连标题一起隐藏，全不命中时给一条提示。③ 逐分类弹层不再自己筛数组，直接把输入框内容当 `keyword` 传下去；弹层与新建分类弹窗**共用同一套分组**，两处观感一致。④ 验证：写了一份假 DOM 校验脚本（按括号配对精确提取 `ICON_GROUPS` 与 `buildIconPicker`，不拖整段脚本的依赖链）共 32 项断言——组数/组名/每组枚数、9 组渲染、组名顺序、按钮总数 60、选中态唯一、筛选只留命中组且空组标题消失、点击写回 `dataset.value`——全通过。⑤ 说明：**通用组不与前 8 组重复**（同一枚图标只出现在一组里），所以选中态可以直接按名字标记，不必处理「同一枚出现在两处」。 |
| 2026-09-21 | **修正分组选择器的标题位置**（只改 `M4-CSS` 与 `M4-MAIN` 的 `buildIconPicker()` 的节点组装，分组数据与归属不变）。用户截图指出「密码库 锁·凭证·保护」这个标题下方显示的是通用组的 28 枚图标，看起来像归属错了。核对结果：**数据归属是对的**（`密码库` 只有 `lock` / `key` / `shield` / `keyhole`，那 28 枚确实在 `通用` 组），错的是排版——上一版把结构做成「`.icon-row`（图标排）+ `.icon-row-label`（组名）交替」，标题于是落在**它自己那组图标的下方**，视觉上就压到了下一组的头上。修法：每组包成一个 `.icon-group`，块内顺序改为**组名在上、图标排在下**，块间加 `.icon-group + .icon-group` 的分隔线；`.icon-pick` 的单列 grid 只负责排这些块。顺带把筛选逻辑从「跳过空组」改成「整块不渲染」——这样「标题留下、图标没了」这种残留从结构上就不可能发生（原先靠 JS 成对判断，现在靠块级结构保证）。验证：假 DOM 校验脚本 26 项断言全通过，其中专门有一条「每个块的第 1 个子节点必须是 `.icon-row-label`、第 2 个必须是 `.icon-row`」，以及「筛选后块数 == 组名数」「全不命中时没有残留标题」。 |
| 2026-09-21 | **换图标弹层改为浮在最上层 + 打开时自动滚到那一行**（只改 `M4-CSS` 的 `.icon-pop` 与 `M4-PREF` 的三个函数，数据与偏好结构不变）。用户反馈两点：① 点开后弹层被面板下沿遮住；② 每次都要先手动滚到目标分类。修法：① `.icon-pop` 从「`.per-cat-row` 内的 `position:absolute`」改为「**挂在 `<body>` 上的 `position:fixed`**」，`z-index:26`（高于 `.modal` 的 10、低于拖拽幽灵的 60）——原来它挂在设置面板的滚动区里，而那是 `overflow:auto`，所以行在面板底部时弹层被裁掉、只露出顶部一条；脱离面板后与它再无裁剪关系。位置改由新增的 `placePopover()` 按触发按钮的**视口坐标**现算：优先在下方 `bottom+6`，下方空间不足且上方更宽裕时向上翻，最后把 `top`/`left` 夹在窗口内（左右用 CSS 的 `width:290px` 参与计算，故 JS 里同步维护 `POP_W`）。② 新增 `revealRow()`：打开前先把那一行滚进 `.dialog-body` 的可视区（行内已在可视区就不动），**再**测按钮坐标定位——顺序不能反。③ 浮层不跟面板滚动，所以 `scroll`（捕获阶段）、`resize`、关面板都要 `closeIconPopover()`；关面板那条写进 `closeSettings()`（浮层挂在 `body` 上，不会随面板隐藏）。④ `.per-cat-row` 上那句 `position:relative` 随之删掉（不再需要定位上下文）。验证：假 DOM 校验脚本 17 项断言全通过——弹层在 `document.body` 上而不在行里、行在可视区外时 `scrollTop` 恰为「行顶到可视区顶部 −12」、空间足够时 `top = bottom+6`、空间不足时翻到上方且整块在窗口内、左右被夹住、再点同一按钮收起、滚动/关面板收起。 |
| 2026-09-21 | **新增网址时自动读取网站图标**（M4 配合 M1/M3 新接口）。网址输入框失焦时会请求 `POST /links/icon` 并把返回值经既有 `shrinkImage()` 压成 128px PNG；若用户直接保存且尚未有自选图标，也会自动补读一次。读取失败不阻止保存，继续使用默认图标；自选上传图标永远优先、不会被自动读取覆盖。`IPC_MAP` 已映射到 `fetch_link_icon`，因此浏览器版与桌面版一致。 |
| 2026-09-22 | **优化自动网址图标清晰度**（M1/M3 同步）。自动读取的图标现保留至 256px PNG（手动上传和应用图标仍为 128px）；两端会优先抓取网页声明的高清尺寸、SVG 或 Apple Touch 图标。数据字段、接口和自选图标优先级均不变。 |
| 2026-09-21 | **圆形胶囊侧栏支持调节按钮底色透明度**（只改 `M4-CSS` / `M4-PREF`，后端与数据契约不变）。设置面板在选中「圆形胶囊」版式时显示 `#orbOpacity` 滑块，范围 20–100%，默认 100%，值存在本端 `pwb.prefs.orbOpacity`。它写入 `--orb-button-fill`，用 `color-mix()` 同步淡化每个圆形按钮的纸面、描边与阴影；图标/文字不使用整体 `opacity`，因而始终清晰。 |
