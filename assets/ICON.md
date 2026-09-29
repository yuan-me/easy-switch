# 应用图标

唯一消费源为 `app-icon.png`，由内置 image_gen 生成。侧栏直接引用；`node scripts/generate-icons.mjs` 用 Tauri CLI 生成 Windows PNG/ICO。外框以外保留透明通道。旧设计不参与构建。

最终提示词摘要：去掉所有字母与文字；青绿、暖橙、金黄三色组成圆润、粗线条、交织切换的抽象图形；外置浅象牙圆角方框；外框之外透明；适合 16–256px 的桌面应用图标。不复刻参考中的星形。

风格参考：[CC Switch 官方图标](https://github.com/farion1231/cc-switch/blob/main/src-tauri/icons/icon.png)。用户先确认三色风格，再要求去掉字母并增加圆角框。
