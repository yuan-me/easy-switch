# Easy Switch

<img src="src-tauri/icons/128x128.png" alt="Easy Switch 图标" width="80" height="80">

在 Windows 上切换 Codex 供应商，管理本地会话和 Token 用量。

[下载 v1.4.0](https://github.com/yuan-me/easy-switch/releases/download/v1.4.0/Easy.Switch_1.4.0_x64-setup.exe) · [更新记录](CHANGELOG.md) · [使用 Fast](docs/NATIVE_FAST.md)

## 安装

1. 下载并运行 `Easy.Switch_1.4.0_x64-setup.exe`。安装到当前 Windows 用户，缺少 WebView2 时会联网下载。
2. 打开“设置”，确认 Codex 数据目录和桌面程序路径。
3. 添加供应商，填写 API 地址、密钥和模型，然后点击“切换”。切换会正常退出并重新启动 Codex，请先完成正在生成的任务。

支持 Windows x64，日常使用不需要 Rust 或 Node.js。`test-channel` 用于隔离测试，请使用正式发行版。

## 1.4.0 更新

- 按供应商设置“解锁原生Fast”，默认关闭，使用同一份显示模型列表。适配新版 Codex 的异步脚本，修复解锁失败和过早检查的问题。
- 切换已保存但启动失败时，界面仍显示实际保存的供应商。Codex 已启动但 Fast 未解锁时，会单独说明原因。
- 窗口和卡片更紧凑，侧栏可折叠。背景带少量主题色，图表缩小字号并增加输入、输出明细。
- “兼容图片工具”和 Fast 位于供应商高级设置顶部。保存当前供应商的 Fast 开关后可选择立即重启或稍后应用。

Fast 解锁只让客户端提供选项。重启后还需要在模型菜单中选中 Fast，蓝色闪电表示已选中。供应商是否支持优先处理、实际速度和费用由供应商决定。详见 [Fast 的使用范围与测试结果](docs/NATIVE_FAST.md)。

## 功能

- 官方登录、纯 API、混入 API 和聚合路由。支持 Responses 透传、Chat Completions 转换，以及按会话、轮转、权重和故障转移选择成员。
- 获取供应商模型列表，选择默认模型及在 Codex 中显示的模型。启动后及每小时检查列表，保留手动排除项，提示新增和移除的模型。
- 仪表盘显示当前供应商、今日输入与输出、最近七天用量、最近会话和缓存摘要。统计只读本地历史，缓存属于输入，不重复计入总量；缺失数据会提示。
- 搜索、预览、导出、归档和删除会话，修复供应商关联，调整项目关联。项目关联操作不移动项目代码。
- 浅色、深色、跟随系统主题及自定义主题色。设置自动保存，失败后可重试。
- 检查和下载 GitHub 更新，验证签名后由用户确认安装。

Responses 供应商可开启“兼容图片工具”。它提供客户端兼容配置，实际出图仍取决于供应商。Chat Completions 转换支持普通函数工具，不支持原生图片工具、加密历史和原生 compact。

## 数据与更新

正式版数据保存在 `%LOCALAPPDATA%\EasySwitch`，测试版使用 `%LOCALAPPDATA%\EasySwitchTest`。首次启动可导入旧 `%LOCALAPPDATA%\CodexSwitch` 的供应商、设置和数据目录引用。密钥及自定义请求头使用当前 Windows 用户的 DPAPI 加密，不能直接跨账户迁移。

切换时先正常退出 Codex，再临时备份配置和会话，校验修改并重启。成功后清理操作副本；中断的操作保留副本并提供恢复入口。检测到外部修改、数据库仍在写入或文件异常时会停止。永久删除会话无法撤销，需要保留时请先导出或归档。

Codex 使用的 `auth.json` 和 `config.toml` 按客户端原生格式保存，其中可能含密钥，请勿公开。关闭 Easy Switch 窗口不会终止仍在处理请求的本地代理。

默认启动后检查更新，运行期间每 24 小时再检查。下载完成后提示安装，确认后关闭 Codex 和 Easy Switch；取消会保留更新包。签名或版本校验失败时拒绝安装。可以在设置中分别关闭自动检查和自动下载。回退版本时可从 Releases 安装旧包，数据目录独立于安装目录。

## Linux Remote-SSH

仓库另有 VS Code 扩展，用于远端 Linux 的供应商切换和聊天迁移，版本独立于 Windows 桌面版。安装位置、受支持的 Codex 版本及测试范围见 [扩展说明](extensions/vscode/README.md)。

## 开发与测试

需要 Node.js 22+、Rust stable/MSVC、Windows SDK 和 WebView2 Runtime。

```powershell
npm ci
npm run tauri -- dev
```

构建程序或安装包：

```powershell
./scripts/build.ps1
./scripts/build.ps1 -Installer
```

运行回归检查：

```powershell
cargo test --workspace --locked
npm run build
node crates/core/assets/native-fast-unlock.test.cjs
npx playwright test
```

`node scripts/native-smoke.cjs` 使用独立合成目录检查已构建的桌面程序。浏览器测试不能代替真实客户端验收，具体结果见 [验证记录](docs/VALIDATION.md)。架构和发布流程见 [ARCHITECTURE.md](docs/ARCHITECTURE.md) 与 [RELEASING.md](docs/RELEASING.md)。

## 许可证

从 1.4.0 起，Easy Switch 以 [GNU AGPL-3.0](LICENSE) 发布。每个发行标签提供对应源码，构建所需的锁文件和脚本也在仓库中。第三方代码保留原有声明，Fast 模块的来源与本地修改见 [第三方说明](third-party/README.md)。
