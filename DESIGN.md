---
name: Easy Switch
description: Windows Codex 操作工作台，以清楚的状态、紧凑卡片和可恢复操作组织界面。
colors:
  bg: "#f8f9fb"
  surface: "#fff"
  sidebar: "#f2f4f7"
  text: "#202834"
  muted: "#657080"
  line: "#e1e5eb"
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
  dark-bg: "#15181e"
  dark-surface: "#1e222a"
  dark-sidebar: "#11151a"
  dark-text: "#e8ecf2"
  dark-muted: "#a0abba"
  dark-line: "#343b47"
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
typography:
  headline:
    fontFamily: '"Segoe UI Variable", "Segoe UI", "Microsoft YaHei UI", sans-serif'
    fontSize: "27px"
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
  navigation: "9px"
  icon: "11px"
  panel: "12px"
  modal: "15px"
  empty-icon: "18px"
spacing:
  tight: "6px"
  small: "8px"
  control: "12px"
  medium: "16px"
  panel: "20px"
  form: "24px"
  page: "32px"
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
    padding: "12px 14px"
  active-tag:
    backgroundColor: "{colors.green-bg}"
    textColor: "{colors.green}"
    rounded: "{rounded.tag}"
    padding: "3px 6px"
  provider-card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.panel}"
    padding: "18px 20px 0"
  session-workspace:
    backgroundColor: "{colors.surface}"
    rounded: "{rounded.panel}"
  provider-drawer:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    width: "520px"
    height: "100vh"
---
# Design System: Easy Switch

## Overview

**Creative North Star: "Codex 操作工作台"**

延续本目录 `index.html` 的已批准方向合约：用中性浅色与炭黑表面、蓝色动作和中文系统字体，让当前供应商、已保存配置与实际应用状态清楚可辨。界面紧凑、平静，信息层级依靠留白、文字与边框建立。

这是 Windows x64 Tauri 2 + Rust + React 实现的视觉记录。依据 `src/styles.css`、`src/App.tsx`、`src/Providers.tsx`、`src/Sessions.tsx`、`src/components.tsx` 及 `artifacts/screenshots/`；不继承上级旧 WPF 设计。本文件记录视觉事实，不能替代 `docs/VALIDATION.md` 中尚未完成的真实 VM 图片、账户切换和跨版本升级验收。

**Key Characteristics:**

- 固定侧栏、四个导航入口、单列供应商卡片。
- 浅色、深色和跟随系统主题共享同一语义色角色。
- 右侧编辑抽屉与可调会话分栏保持上下文。
- 短时状态过渡、清楚焦点、尊重减少动画设置。

## Colors

主色是明确的操作蓝，中性灰组织背景与文字；绿、黄、红仅表达状态，不构成装饰性辅色体系。前置 token 是规范值；无前缀为浅色，`dark-` 为深色同名角色，对应 `:root[data-theme=dark]` 的 CSS 变量覆盖。

### Primary

- **操作蓝（accent / accent-hover）：** 添加与保存等主要动作，以及悬停反馈。
- **浅蓝底与蓝色文字（tint / accent-text）：** 导航选中项、切换动作、批量操作区与用户消息。深色主题用较亮蓝色及压低明度的蓝底。
- 浅色主按钮用白字；深色主按钮源码固定用深蓝墨色文字（`#101b33`），不使用浅色的白字组合。

### Neutral

- **工作区底色（bg）、内容表面（surface）、侧栏底色（sidebar）：** 三种表面明确区域，不使用渐变。
- **主要文字（text）、辅助文字（muted）：** 标题与内容优先，字段提示、路径和元数据退后。
- **边界（line）、柔和填充（soft）、悬停填充（hover）：** 支持列表、分段控件和表单的日常状态。
- **状态色（green / warning / error 及各自 bg）：** 使用中、注意与失败分别配文字或图标，不能仅靠颜色表达。

**The State Truth Rule.** “配置已保存”“使用中”“图片兼容已配置”是不同状态；视觉标签与操作结果必须保留这一差别。

## Typography

所有日常 UI 使用前置 `body` 的 Windows 中文系统字体栈；不加载网络字体。高级请求头 JSON 使用 `code` 字体，聊天正文仍使用 UI 字体。

### Hierarchy

- **Headline：** 页面标题，使用前置 `headline`；无大号宣传式 display 字体。
- **Title：** 常规二级标题使用 `title`；供应商名、设置标题和详情标题采用 `card-title`。对话框标题另为 18px、600。
- **Body：** 根字号 14px；页面说明 13px；表单、卡片模型和阅读器正文主要为 12px。普通段落行高 1.65；消息正文 1.8。
- **Label：** 字段说明 10px；元数据主要为 11px；表单标签使用 `form-label`。不要把这些现有尺寸误读为统一的 14px 正文。
- **数值与长文本：** Token 表格采用等宽数字；路径、端点与列表标题用省略号，阅读正文换行并允许选择复制。

**The Legible Hint Rule.** 占位文字使用 `muted` 且 opacity 为 1；不得再叠加透明度使浅色提示变淡。

## Layout

应用壳为 `206px minmax(0,1fr)` 两列，高度 100vh；CSS 最小高度 520px。产品约定的桌面最低逻辑尺寸为 940×620，不能将 CSS 最小高度当作窗口验收值。

侧栏始终固定 206px，包含供应商、会话、备份恢复、设置四个入口；底部容纳当前供应商与三态主题切换。主区独立滚动；顶部栏高 59px。普通页面最大宽度 1260px，居中，内边距为 31px 32px 25px；设置页最大宽度 1000px。

供应商列表在所有桌面宽度均为单列，卡片间距 13px。卡片内部模型/端点为两栏，比例 0.75:1.25，间距 20px；这不意味着卡片列表可以改为两列。搜索框默认宽 245px、最小 150px。

会话工作区采用列表 / 5px 分隔条 / 详情布局。列表默认 300px，可拖动到 230–420px；键盘左右箭头每次调整 10px。会话页高度为视口减去顶栏，内部列表与阅读器分别滚动。当前分栏宽度保存在组件状态，未持久化为用户设置。

仅有一个宽度媒体查询（≤1000px）：页面改为 25px 22px 内边距、侧栏缩减内边距、搜索框改为 210px、卡片内部取消左缩进，并压缩部分间距。侧栏宽度仍为 206px，没有 180px 侧栏规则，也没有 ≥1400px 双列卡片规则。没有已实现的移动端折叠菜单。

**The Stable Workbench Rule.** 保持固定侧栏和供应商单列；宽度变化通过内容内边距和内部密度适配。

## Elevation & Depth

常规工作区、卡片和列表默认平面化，使用背景色阶及 1px 边框分层。浮层才使用阴影：菜单、对话框、编辑抽屉和操作进度条共享主题阴影；分段筛选的选中项仅有很轻的局部阴影。

### Shadow Vocabulary

- **浅色浮层：** `0 10px 36px #13223a18`。
- **深色浮层：** `0 12px 40px #0006`。
- **分段选中项：** `0 1px 3px #10182010`。
- **遮罩：** `#0c142866` 配合 2px 背景模糊，只用于 modal / drawer 的 backdrop。

**The Quiet Surface Rule.** 卡片不添加悬浮位移或常驻阴影；悬停通过边框颜色表达。

## Shapes

以轻圆角矩形为主：标签、控件、导航、面板、模态框分别使用前置圆角角色。侧边编辑抽屉为直角全高面板，不继承模态框圆角。状态点和切换控件圆点使用圆形；品牌符号为无字母的青绿、暖橙、金黄交织抽象图形，外置浅象牙圆角框。参考 CC Switch 的多色圆润语言，不沿用其星形图案。Brand 与窗口、安装器图标共用 assets/app-icon.png，主标记 38×38px。

边框默认 1px。卡片激活边框为 accent 与 line 按 65% / 35% 混合，悬停边框为 32% / 68% 混合。圆角和边框不承担操作成功的唯一信号。

## Components

### Buttons

清楚而克制。常规按钮最小高度 34px；主按钮沿用同一形状。页面添加按钮最小高度 37px，卡片操作按钮最小高度 29px。次按钮为表面底色和细边框；文本按钮用于编辑等低强调操作；切换按钮使用蓝色浅底。

按钮悬停更换背景，按下向下移动 1px；禁用 opacity 0.48。背景、边框、位移过渡为 160ms。主要动作的深色前景特例见 Colors。危险按钮使用 error 底与 surface 字色，是现有实现，不代表完整对比度认证。

### Inputs / Fields

表单控件最小高度 37px、细边框、表面背景，使用 control 圆角。搜索框为带 17px 图标的整体容器，内层输入取消自身边框；容器 focus-within 显示焦点轮廓。输入禁用 opacity 0.6。

按钮、链接、输入、选择器、文本域、summary 与会话分隔条拥有 2px accent 焦点轮廓，偏移 3px；搜索整体偏移 2px。错误以文字区域呈现，不把错误仅标在红边框上。

### Navigation / Segmented Controls

侧栏入口为图标、文字和当前项尾部箭头，最小高度 43px；默认 muted，选中使用 tint / accent-text 和 600 字重。侧栏图标为 19px 线条图标。

筛选采用 soft 底分段组，选中项使用 surface 和轻阴影；主题按钮采用同样的轻量选中反馈。主题模式包含浅色、深色、系统，系统模式监听系统变化。

### Chips / Status

“使用中”标记带勾图标，采用 green-bg / green，10px 文字与 tag 圆角。只有匹配活动供应商 ID 的卡片显示该标签；保存配置不会直接把标签移到新配置。

### Provider Cards

卡片组织为身份区、模型与连接地址、能力与操作底栏。主标题 15px，图标容器 42×42px。下方边线分离状态与动作。卡片平面显示，激活边框与“使用中”标签共同表达状态。

更多操作菜单宽 140px，位于触发器右侧，采用主题浮层阴影。官方供应商图标用 text / surface 反色，不引入供应商品牌图片依赖。

### Editor Drawer / Confirmation Modal

编辑抽屉从右侧打开，宽 520px、最大 90vw、高 100vh。表单内容独立滚动，底部保存/取消操作保持在表单末端固定区域；表单双列字段间距 15px。关闭、Esc 与遮罩点击通过现有关闭处理器响应，保存期间关闭被处理器阻止。

普通确认模态框宽 550px、最大 `calc(100vw - 40px)`、最大高度 86vh。二者使用原生 dialog。抽屉入场为 200ms、`cubic-bezier(.16,1,.3,1)`，位移从 22px 到 0、透明度从 0.7 到 1。

### Session Workspace

列表选中行使用 tint，悬停使用 soft；详情以对话、Token 历史、线程信息三个页签组织。页签选中态采用蓝字和 2px 蓝色下划线。阅读正文保留换行；用户消息使用 tint，其他消息使用 soft。详情底栏提供继续、复制和导出操作。

### Motion / Feedback

常规交互以 160ms、180ms、200ms 为实际时长，符合已批准的 150–200ms 方向；卡片和开关为 180ms。加载图标为 1s 线性循环，是持续状态例外。系统请求减少动画时，所有 transition 与 animation 均关闭，滚动行为设为 auto。

操作进度条位于主区底部，使用 surface 底、text 主字、muted 辅助字与 line 细边框，无浮层阴影；细条与加载图标沿用 accent，浅深主题同步。成功、失败、更新提示均带明确文字；浏览器预览显示“演示预览 · 合成数据”。

## Do's and Don'ts

### Do:

- **Do** 保留 206px 侧栏、四导航入口与单列供应商卡片。
- **Do** 使用成对的浅深主题语义色，检查两种主题的焦点、提示文字和活动状态。
- **Do** 用文字和图标区分已保存、使用中、兼容配置及诊断结果。
- **Do** 为可交互元素保留焦点反馈，并尊重减少动画设置。
- **Do** 以实际源码和本目录截图更新视觉记录，功能验收单独查阅 docs/VALIDATION.md。

### Don't:

- **Don't** 引入 180px 窄侧栏、宽屏两列供应商卡片或未批准的移动端导航替代布局。
- **Don't** 再给占位文字叠加 opacity，或依赖颜色单独解释状态。
- **Don't** 为日常卡片添加装饰渐变、持续阴影或悬停浮起效果。
- **Don't** 将保存、构建成功、合成数据截图或本设计文档视为真实切换、图片生成和升级验收。

界面文案仅保留操作名称、状态、必要条件与错误恢复提示；不使用页面口号或装饰性副标题。

## 1.1.0 已实现界面增量

- 采用已确认的云母方案，Windows 11 原生 Mica 基底，标题栏 44px；默认窗口 1280×900、最小 940×620，已有窗口尺寸继续由原插件保存。
- 统计图输入色 #658DCE、输出色 #AECBEF，深色主题适当提高输入色亮度。图表使用真实数据比例，概念图中的装饰比例不直接照搬。
- 侧栏增加 Token 统计，底部供应商图标不带白色外框；不再显示统计页底部缓存说明。
- 会话页工具栏合为一行，列表两行信息，正文 14px；复制／导出移至详情页头，继续按钮保留在底部。
- 普通提示 10 秒后消失，连续提示重新计时；需要用户处理的对话框、进度和更新横条仍保留。
