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
# 界面约定

## 当前布局

导航依次为仪表盘、供应商、Token 统计、会话和设置。侧栏可收起为图标，保留按钮名称和键盘焦点，并记住折叠状态。主内容区左上角使用 16px 圆角，侧栏不画额外竖线。

默认窗口目标为 1180×840 逻辑像素，显示前根据屏幕工作区和系统缩放调整并居中。最低尺寸为 940×620，小屏仍受工作区边界限制。卡片、弹窗和文字采用紧凑尺寸；顶部元数据为历史设计参考，实际样式以 `src/design.css` 和 `src/styles.css` 为准。

## 颜色与动效

浅色以灰白为底，深色以深灰为底。页面、标题栏、侧栏、表格交替行和小卡片混入少量当前主题色，文字使用中性色。按钮和选中状态使用主题色，成功、警告和错误保留各自颜色及文字提示。弹窗和操作进度使用实色背景。

侧栏展开为 300ms、收起为 220ms，使用 `cubic-bezier(.16,1,.3,1)`。快速反向切换接续当前进度，文字保持单行。系统要求减少动态效果时关闭动画。滚动条在滚动时显示。

## 供应商

列表使用单列卡片，名称、使用中状态、模型、地址和操作各有固定位置。卡片底部的“Fast模式已启用”表示该供应商已保存开关，不表示客户端已选中 Fast 或供应商已执行优先请求。

编辑窗口分为基本信息、连接、模型和高级设置。“解锁原生Fast”与“兼容图片工具”位于高级设置顶部，双列排列，无彩色底块或末尾装饰图标。Fast 直接使用显示模型列表，默认模型不能取消勾选。

## 图表与状态

输入与输出使用同色系深浅区分，配合图例和明细。柱高按实际比例绘制，不为极少的输出量人为放大。图表标签应在最小窗口内留有空间。

保存、应用和启动分别反馈。配置提交后，即使启动失败，也要刷新当前供应商。确认框说明会退出哪些程序；提交和回滚期间不可取消。普通通知 10 秒后关闭，更新安装和确认框由用户操作。

## 无障碍

图标按钮提供名称，交互可用键盘操作，焦点可见。状态不能只靠颜色表达。错误提示保留用户输入并提供重试路径。界面检查与实际客户端验证见 [验证记录](docs/VALIDATION.md)。
