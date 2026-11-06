# 第三方代码

## CodexPlusPlus 原生 Fast 模块

`crates/core/assets/native-fast-unlock.js` 基于 CodexPlusPlus 贡献者的 [00a1355 版本](https://github.com/BigPizzaV3/CodexPlusPlus/blob/00a1355850cec86d606cf09331cfbc5c66d6512d/assets/inject/native-fast-unlock.js)，采用 [GNU AGPL v3](CodexPlusPlus-LICENSE.txt)。其中内嵌的 tokenizer 保留 MIT 声明。

Easy Switch 的修改包括限定本机 API 身份、发现客户端模块，以及适配 Codex 26.1103 拆分后的 Rspack 模块。修改日期为 2026-11-06。脚本随程序分发，不在运行时下载代码。

同目录的原生 Fast 测试来自上游并作了适配，继续采用相同许可证。未打包的 Codex++ 菜单和设置不在测试范围内；Easy Switch 自身的设置由本项目测试覆盖。

Easy Switch 从 1.4.0 起采用 AGPL-3.0。发行标签中的源码、锁文件与构建脚本对应同版本程序；其他依赖继续遵循各自许可证。
