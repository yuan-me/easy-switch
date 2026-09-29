# 应用图标

唯一设计源为 `app-icon.png`，由内置 image_gen 生成。`node scripts/generate-icons.mjs` 用 Tauri CLI 生成 Windows PNG/ICO；侧栏复用其中的 128px PNG，避免加载大图。外框以外保留透明通道。旧设计不参与构建。

主程序、NSIS 安装器及卸载器显式使用同一 ICO。构建脚本监控 icons 目录变更；签名构建及发布流程使用 `scripts/verify-icons.ps1` 对实际 PE 资源进行六种尺寸的逐字节摘要比对，拒绝旧图标或额外图标资源。

最终提示词摘要：去掉所有字母与文字；青绿、暖橙、金黄三色组成圆润、粗线条、交织切换的抽象图形；外置浅象牙圆角方框；外框之外透明；适合 16–256px 的桌面应用图标。不复刻参考中的星形。

风格参考：[CC Switch 官方图标](https://github.com/farion1231/cc-switch/blob/main/src-tauri/icons/icon.png)。用户先确认三色风格，再要求去掉字母并增加圆角框。
