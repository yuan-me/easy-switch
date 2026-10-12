---
name: Easy Switch
description: Windows Codex 浮岛工作台，以实色岛面、清楚状态和可恢复操作组织界面。
colors:
  window-solid: "#f2f6fc"
  mica-tint: "rgba(242,246,252,.50)"
  surface: "#fff"
  sidebar: "#fff"
  overlay-surface: "#fff"
  text: "#172c4c"
  muted: "#5f718c"
  line: "#dde5ef"
  soft: "#eef1f6"
  hover: "#e9edf3"
  accent: "#2864e8"
  accent-hover: "#1c51c6"
  tint: "#edf3ff"
  accent-text: "#2456b8"
  green: "#23704e"
  green-bg: "#ecf7f0"
  warning: "#946000"
  warning-bg: "#fff6df"
  error: "#b93642"
  error-bg: "#fff0f0"
  chart-input: "#658dce"
  chart-output: "#aecbef"
  dark-window-solid: "#141c29"
  dark-mica-tint: "rgba(20,28,41,.62)"
  dark-surface: "#1f2734"
  dark-sidebar: "#1f2734"
  dark-overlay-surface: "#1f2734"
  dark-text: "#e8eef9"
  dark-muted: "#a9b8cf"
  dark-line: "#364256"
  dark-soft: "#252b35"
  dark-hover: "#303846"
  dark-accent: "#648fff"
  dark-accent-hover: "#80a4ff"
  dark-tint: "#23304a"
  dark-accent-text: "#a4beff"
  dark-green: "#93d6b2"
  dark-green-bg: "#1c342a"
  dark-warning: "#edc275"
  dark-warning-bg: "#392e1e"
  dark-error: "#ff9fa8"
  dark-error-bg: "#3a242c"
  dark-chart-input: "#7ea2db"
  dark-chart-output: "#aecbef"
typography:
  headline:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "25px"
    fontWeight: 650
    letterSpacing: "-0.025em"
  title:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "16px"
    fontWeight: 600
  card-title:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "15px"
    fontWeight: 600
  body:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "14px"
    fontWeight: 400
  field-label:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "10px"
    fontWeight: 400
  form-label:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "12px"
    fontWeight: 550
  code:
    fontFamily: "Consolas, monospace"
    fontSize: "12px"
rounded:
  tag: "5px"
  control: "7px"
  group: "8px"
  navigation: "10px"
  icon: "11px"
  panel: "16px"
  modal: "18px"
  sidebar: "18px"
  empty-icon: "18px"
spacing:
  tight: "6px"
  small: "8px"
  control: "12px"
  medium: "16px"
  panel: "20px"
  form: "24px"
  page: "24px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.surface}"
    rounded: "{rounded.control}"
    padding: "7px 12px"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
  button-secondary:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "7px 12px"
  button-text:
    backgroundColor: "transparent"
    textColor: "{colors.muted}"
    padding: "4px 7px"
  button-switch:
    backgroundColor: "{colors.tint}"
    textColor: "{colors.accent-text}"
    rounded: "{rounded.control}"
    padding: "5px 10px"
  field:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "8px 10px"
  navigation-active:
    backgroundColor: "{colors.tint}"
    textColor: "{colors.accent-text}"
    rounded: "{rounded.navigation}"
    padding: "10px 12px"
  active-tag:
    backgroundColor: "{colors.green-bg}"
    textColor: "{colors.green}"
    rounded: "{rounded.tag}"
    padding: "3px 6px"
  provider-card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.panel}"
    padding: "20px 22px 0"
  session-workspace:
    backgroundColor: "transparent"
  session-pane:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.panel}"
  search:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.muted}"
    rounded: "{rounded.control}"
    padding: "0 10px"
  provider-editor:
    backgroundColor: "{colors.overlay-surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.modal}"
    width: "960px"
    maxHeight: "min(880px, calc(100vh - 48px))"
---
# Design System: Easy Switch

## Overview

**Creative North Star: "Codex 浮岛工作台"**

采用本目录 `index.html` 中用户指定的浮岛方向：灰蓝 Mica 背景承托白色或深灰实色岛面，侧栏、供应商、会话与统计区域通过间隔和柔和阴影分开。蓝色动作和 Windows 中文系统字体保留紧凑工作台的可读性，让当前供应商、已保存配置与实际应用状态清楚可辨。方向流程中的 seed `b0a8731e` 只记录选择过程，用户指定的浮岛风格是视觉依据。

这是 Windows x64 Tauri 2 + Rust + React 实现的视觉记录。依据 `src/styles.css`、`src/App.tsx`、`src/Providers.tsx`、`src/Models.tsx`、`src/Sessions.tsx`、`src/Usage.tsx`、`src/components.tsx` 及 `artifacts/screenshots/`；不继承上级旧 WPF 设计。本文件记录视觉事实，不能替代 `docs/VALIDATION.md` 中尚未完成的真实 VM 图片、账户切换和跨版本升级验收。

**Key Characteristics:**

- 脱离窗口边缘的侧栏、五个导航入口、单列供应商浮岛。
- 浅色、深色和跟随系统主题共享同一语义色角色。
- 居中宽编辑弹窗与两块独立会话岛面保持上下文。
- 搜索图标与输入横向对齐；滚动条在实际滚动时显示，停止后隐藏。
- 短时状态过渡、清楚焦点、尊重减少动画设置。

## Colors

主色是明确的操作蓝，中性灰组织背景与文字；绿、黄、红仅表达状态，不构成装饰性辅色体系。前置 token 是规范值；无前缀为浅色，`dark-` 为深色同名角色，对应 `:root[data-theme=dark]` 的 CSS 变量覆盖。

### Primary

- **操作蓝（accent / accent-hover）：** 添加与保存等主要动作，以及悬停反馈。
- **浅蓝底与蓝色文字（tint / accent-text）：** 导航选中项、切换动作、批量操作区与用户消息。深色主题用较亮蓝色及压低明度的蓝底。
- 浅色主按钮用白字；深色主按钮源码固定用深蓝墨色文字（`#101b33`），不使用浅色的白字组合。

### Neutral

- **工作区基底（window-solid / mica-tint）：** 不支持 Mica 时使用实色基底；启用原生 Mica 后，仅应用壳使用透明色覆盖，岛面保持实色。
- **内容表面（surface）、侧栏（sidebar）、弹层（overlay-surface）：** 同主题的实色面通过留白与阴影区分区域，不使用装饰渐变。dialog 与操作进度显式绑定 overlay-surface，避免下层内容透出。
- **主要文字（text）、辅助文字（muted）：** 标题与内容优先，字段提示、路径和元数据退后。
- **边界（line）、柔和填充（soft）、悬停填充（hover）：** 支持列表、分段控件和表单的日常状态。
- **状态色（green / warning / error 及各自 bg）：** 使用中、注意与失败分别配文字或图标，不能仅靠颜色表达。
- **统计输入 / 输出（chart-input / chart-output）：** Token 图表使用同一蓝色系的深浅区分，并配图例与数值。

请求减少透明度时使用实色窗口背景；侧栏浅色为 `#edf2f8`、深色为 `#161e2b`。强制颜色模式交由系统 Canvas、CanvasText 和 Highlight 等颜色表达边界与图表。

**The State Truth Rule.** “配置已保存”“使用中”“图片兼容已配置”是不同状态；视觉标签与操作结果必须保留这一差别。

## Typography

所有日常 UI 使用前置 `body` 的 Windows 中文系统字体栈；不加载网络字体。高级请求头 JSON 使用 `code` 字体，聊天正文仍使用 UI 字体。

### Hierarchy

- **Headline：** 页面标题，使用前置 `headline`；无大号宣传式 display 字体。
- **Title：** 常规二级标题与会话详情标题使用 `title`；供应商名和设置标题采用 `card-title`。对话框标题另为 18px、600。会话页标题为 23px，窄窗为 21px。
- **Body：** 根字号 14px；页面说明 13px；表单与卡片模型主要为 12px；会话正文为 14px、行高 1.6，工具记录为 12px。普通段落行高 1.65。
- **Label：** 字段说明 10px；元数据主要为 11px；表单标签使用 `form-label`。不要把这些现有尺寸误读为统一的 14px 正文。
- **数值与长文本：** Token 表格采用等宽数字；路径、端点与列表标题用省略号，阅读正文换行并允许选择复制。

**The Legible Hint Rule.** 占位文字使用 `muted` 且 opacity 为 1；不得再叠加透明度使浅色提示变淡。

## Layout

应用壳为 `216px minmax(0,1fr)` 两列，`44px minmax(0,1fr)` 两行，高度 100vh、min-height 为 0；首行是跨列标题栏。默认窗口为 1280×900，最低逻辑尺寸为 940×620，沿用已有窗口尺寸保存机制。

侧栏所在列宽 216px，外边距为 `10px 0 16px 14px`，内边距为 `18px 10px 14px`；包含供应商、会话、Token 统计、备份恢复、设置五个入口，底部保留当前供应商与三态主题切换。主区独立滚动。普通页面最大宽度 1440px，横向居中，内边距为 `20px 24px 24px`。

供应商列表在所有桌面宽度均为单列，卡片间距 18px，内边距见前置 provider-card。卡片内部模型/端点为两栏，比例 0.75:1.25，间距 20px；这不意味着卡片列表可以改为两列。搜索框默认宽 245px、最小 150px；模型选择器中占满可用宽度。

会话工作区采用列表 / 5px 分隔条 / 详情布局，相邻列各留 6px 间隔；列表和详情分别构成浮岛。列表默认 300px，可拖动到 230–420px；键盘左右箭头每次调整 10px。会话页随主区弹性填满可用高度，内边距为 `20px 16px 16px`，内部列表与阅读器分别滚动。当前分栏宽度保存在组件状态，未持久化为用户设置。

≤1040px 时供应商卡片底栏允许换行、模型提示最大宽度缩至 240px。≤1000px 时侧栏列宽缩至 190px，外边距改为 `8px 0 12px 10px`、内边距为 `15px 8px`；普通页面内边距为 `20px 18px 18px`，会话页为 `20px 12px 12px`；供应商卡片改为 `18px 18px 0`，搜索框宽 210px，并压缩内部间距。没有已实现的移动端折叠菜单或宽屏双列供应商列表。

**The Stable Workbench Rule.** 保持五入口侧栏和供应商单列；窄窗通过侧栏列宽、外边距及内容密度适配，保留浮岛间隔。

## Elevation & Depth

侧栏、供应商卡片、设置区、备份列表、Token 指标与图表、操作进度和会话两栏使用实色岛面与常驻柔和阴影。岛面之间的留白形成分区；菜单、对话框和编辑抽屉继续使用较深的浮层阴影。分段筛选选中项仅有很轻的局部阴影。

### Shadow Vocabulary

- **浅色岛面（island-shadow）：** `0 4px 18px #172c4c0a,0 1px 3px #172c4c05`。
- **深色岛面（island-shadow）：** `0 5px 20px #00000024,0 1px 3px #00000016`。
- **浅色浮层：** `0 10px 36px #13223a18`。
- **深色浮层：** `0 12px 40px #0006`。
- **分段选中项：** `0 1px 3px #10182010`。
- **遮罩：** `#0c142866` 实色半透明遮罩，只用于 modal / drawer 的 backdrop；不对整窗做实时背景模糊。

**The Quiet Surface Rule.** 常驻阴影只表达岛面层次；卡片悬停不改变边框或产生位移，活动状态以细轮廓和“使用中”标签表达。

## Shapes

以圆角浮岛为主：内容岛面使用 panel 圆角，侧栏、模态框和编辑抽屉使用 18px 圆角；抽屉四周保留窗口边缘间隔。标签、控件、导航仍使用较小圆角。状态点和切换控件圆点使用圆形；品牌符号为无字母的青绿、暖橙、金黄交织抽象图形，外置浅象牙圆角框。参考 CC Switch 的多色圆润语言，不沿用其星形图案。Brand 与窗口、安装器图标共用 assets/app-icon.png，主标记 38×38px，标题栏标记 25×25px。

控件边框默认 1px；内容岛面保留透明边框或无边框，侧栏无边框。供应商活动卡片使用 accent 与 line 按 55% / 45% 混合的 1px 内侧轮廓（offset -1px）。强制颜色模式为岛面恢复 CanvasText 边框并去除阴影。圆角和边界不承担操作成功的唯一信号。

## Components

### Buttons

清楚而克制。常规按钮最小高度 34px；主按钮沿用同一形状。页面添加按钮最小高度 37px，卡片操作按钮最小高度 29px。次按钮为表面底色和细边框；文本按钮用于编辑等低强调操作；切换按钮使用蓝色浅底。

按钮悬停更换背景，按下向下移动 1px；禁用 opacity 0.48。背景、边框、位移过渡为 160ms。主要动作的深色前景特例见 Colors。危险按钮使用 error 底与 surface 字色，是现有实现，不代表完整对比度认证。

### Inputs / Fields

表单控件最小高度 37px、细边框、表面背景，使用 control 圆角。搜索框为带 17px 图标的横向容器，图标不收缩；内层输入取消自身边框、背景和轮廓，容器 focus-within 显示焦点轮廓。表单的竖向 label 规则显式排除搜索框与模型选项，避免搜索图标与输入上下排列。输入禁用 opacity 0.6。

认证模式、协议、路由策略、项目、会话状态、统计模型、主题及模型候选共八处选择控件复用 Select，保留原生选择语义，共享 `base-select` 样式：12px 正文、统一箭头、10px 圆角实色菜单、5px 内边距、34px 最小选项高度、选中浅蓝底与勾号。长选中文字通过 selectedcontent 省略并为箭头保留位置；选项进入 top layer，按窗口空间自动上下展开；更多操作菜单复用同样的表面、圆角和行高。模型候选使用输入框右侧的下拉箭头，输入框仍允许填写列表外 ID。支持该平台能力的 WebView2 使用统一菜单；较旧运行时保留可操作的系统选择器。

按钮、链接、输入、选择器、文本域、summary 与会话分隔条拥有 2px accent 焦点轮廓，偏移 3px；搜索整体偏移 2px。错误以文字区域呈现，不把错误仅标在红边框上。

### Navigation / Segmented Controls

五个侧栏入口为图标、文字和当前项尾部箭头，最小高度 44px；默认 muted，选中使用 tint / accent-text 和 600 字重。侧栏图标为 19px 线条图标。

筛选采用 soft 底分段组，选中项使用 surface 和轻阴影；主题按钮采用同样的轻量选中反馈。主题模式包含浅色、深色、系统，系统模式监听系统变化。

### Chips / Status

“使用中”标记带勾图标，采用 green-bg / green，10px 文字与 tag 圆角。只有匹配活动供应商 ID 的卡片显示该标签；保存配置不会直接把标签移到新配置。

### Provider Cards

卡片组织为身份区、模型与连接地址、能力与操作底栏。主标题 15px，图标容器 42×42px。下方边线分离状态与动作，底栏纵向内边距 12px。卡片使用实色面、panel 圆角与岛面阴影，活动轮廓与“使用中”标签共同表达状态。

更多操作菜单宽 140px，位于触发器右侧，采用主题浮层阴影。官方供应商图标用 text / surface 反色，不引入供应商品牌图片依赖。

### Provider Model Settings

模型配置只有供应商编辑入口：默认模型采用可填写 ID 的下拉组合框，右侧按钮为“获取模型列表”，下方显示最近更新时间，再显示加入 Codex 的勾选列表。默认模型与勾选、供应商配置一起保存；取消放弃本次草稿，刷新列表保留未保存的勾选。聚合路由保留默认模型输入，不展示 API 获取按钮。新增供应商也使用同一表单。

卡片上的 NEW 模型名、模型变化或更新失败是纯展示标签，没有按钮语义、点击行为或键盘焦点。保存编辑内容后确认模型通知。独立模型管理窗口与入口已移除；启动和每小时同步仍在后台运行，界面不再显示自动同步说明和保存后启动 Codex 的提示句。

### Provider Editor / Confirmation Modal

编辑供应商采用居中宽弹窗：宽 960px，上限为视口宽减 48px；高度随内容增长，最大为 880px 与视口高减 48px 中的较小值。标题、关闭与保存按钮固定，中部内容独立滚动。基本信息、连接设置、模型设置与高级设置上下排列。基本信息仅显示名称与认证模式，Provider ID 继续自动生成和内部保存，不展示输入框。连接设置第一行是协议与 API 地址，协议较窄、地址较宽，第二行是独占整行的 API Key；聚合供应商在连接位置展示协议、策略与成员。模型设置独占整行，高级设置默认折叠，内含上下文窗口、自动压缩、图片兼容和自定义请求头；字段原生校验失败会展开对应区域。协议限制说明留在连接区域。

加入 Codex 的模型按行优先双列排列，默认模型保持选中且不可取消；标题右侧提供总数和“全选”，全选覆盖当前完整列表，不受搜索限制，并保留未出现在当前列表中的排除记录。全选后按钮变为“取消全选”，再次点击取消全部可选模型但保留默认模型；只有默认模型或请求进行中时禁用按钮。720px 以下退为单列；最低受支持的 940×620 窗口仍显示双列。表单滚动区域用 soft 底色，各设置块用 surface 实色、1px line 边框、12px 圆角与 20px 内边距，区块间距 16px，保持明确边界，不添加模糊或额外阴影。

API Key 使用密码输入与右侧小眼睛。已保存但尚未查看的密钥以掩码占位；只有点击显示才读取指定供应商密钥，常规 bootstrap 不含密钥。查看与隐藏不会更改保存值，修改后才提交新密钥；读取失败提供局部重试提示，关闭编辑器后不保留显示状态。不再显示“留空保留已有密钥”。

普通确认模态框仍宽 550px、最大 `calc(100vw - 40px)`、最大高度 86vh。编辑与确认复用原生 dialog；编辑从下方 12px 过渡至居中位置，260ms、`cubic-bezier(.22,1,.36,1)`，只动画 transform，遮罩单独淡入 160ms。面板不透明，无整窗模糊；减少动画时关闭过渡。打开时焦点进入名称输入框，关闭时回到触发按钮，获取与保存期间阻止关闭。

### Session Workspace

列表选中行使用 tint 和 2px chart-input 内侧标记，悬停使用 soft；详情以对话、Token 历史、线程信息三个页签组织。页签选中态采用蓝字和 2px 蓝色下划线。阅读正文保留换行；用户消息使用 tint，工具消息使用 soft，助手正文背景透明。复制与导出位于详情页头，继续操作位于底栏。分隔条本体透明，中间 40px 长把手使用 line，悬停与聚焦时变为 accent。

### Scrollbars

所有滚动区域固定使用 `scrollbar-width:thin`，空闲时滑块与轨道均透明；实际 scroll 事件为目标元素设置 `data-scrolling`，滑块显示 muted，最后一次滚动后 800ms 移除标记。悬停本身不显示滚动条，显示与隐藏不切换宽度。强制颜色模式使用 `scrollbar-color:auto`，保留系统可见性。

### Motion / Feedback

常规交互以 160ms、180ms、200ms 为实际时长；卡片和开关为 180ms。编辑弹窗使用 260ms 位移过渡，遮罩为 160ms。加载图标为 1s 线性循环，是持续状态例外。系统请求减少动画时，所有 transition 与 animation 均关闭，滚动行为设为 auto。

操作进度条在主区底部 sticky，距底部 16px，使用不透明 overlay-surface、text 主字、muted 辅助字和岛面阴影；细条与加载图标沿用 accent，浅深主题同步。不定进度条为 1.4s 往返动画，并遵循减少动画设置。成功、失败、更新提示均带明确文字；浏览器预览显示“演示预览 · 合成数据”。

## Do's and Don'ts

### Do:

- **Do** 保留 216px 侧栏列（窄窗 190px）、五导航入口与单列供应商浮岛。
- **Do** 使用成对的浅深主题语义色，检查两种主题的焦点、提示文字和活动状态。
- **Do** 用文字和图标区分已保存、使用中、兼容配置及诊断结果。
- **Do** 为可交互元素保留焦点反馈，并尊重减少动画设置。
- **Do** 保持岛面实色、间隔与轻阴影；保证搜索横向对齐、抽屉头尾可见、滚动区宽度稳定。
- **Do** 以实际源码和本目录截图更新视觉记录，功能验收单独查阅 docs/VALIDATION.md。

### Don't:

- **Don't** 引入宽屏两列供应商卡片或未批准的移动端导航替代布局。
- **Don't** 再给占位文字叠加 opacity，或依赖颜色单独解释状态。
- **Don't** 为日常卡片添加装饰渐变、悬停位移或额外加重阴影。
- **Don't** 将保存、构建成功、合成数据截图或本设计文档视为真实切换、图片生成和升级验收。

界面文案仅保留操作名称、状态、必要条件与错误恢复提示；不使用页面口号或装饰性副标题。

## 1.1.0 已实现界面增量

以下保留 1.1.0 的历史增量；当前布局、尺寸和材质以以上规范为准。

- 采用已确认的云母方案，Windows 11 原生 Mica 基底，标题栏 44px；默认窗口 1280×900、最小 940×620，已有窗口尺寸继续由原插件保存。
- 统计图输入色 #658DCE、输出色 #AECBEF，深色主题适当提高输入色亮度。图表使用真实数据比例，概念图中的装饰比例不直接照搬。
- 侧栏增加 Token 统计，底部供应商图标不带白色外框；不再显示统计页底部缓存说明。
- 会话页工具栏合为一行，列表两行信息，正文 14px；复制／导出移至详情页头，继续按钮保留在底部。
- 普通提示 10 秒后消失，连续提示重新计时；需要用户处理的对话框、进度和更新横条仍保留。

## 2026-10-12 浮岛与搜索、滚动增量

- 用户指定浮岛方向已合并到上面的现行规则；同步侧栏、内容岛面、编辑抽屉、搜索与按滚动显示的滚动条。
- 本轮 14 项 Playwright 检查通过，独立 finish review 无 material findings；浏览器证据包括 `providers-light.png`、`providers-dark.png`、`providers-min.png`、`islands-session-min.png`、`islands-usage-min.png`（均位于 `artifacts/screenshots/`），以及临时目录的 `easy-switch-islands-editor-min.png`。
- 本轮未完成 Windows 原生窗口验收，未执行安装或发布；以上浏览器合成数据检查不替代既有后端、真实账户与升级验收边界。
