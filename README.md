# Easy Switch

<img src="src-tauri/icons/128x128.png" alt="Easy Switch 图标" width="80" height="80">

Windows x64 的 Codex 供应商与会话管理工具，使用 Tauri 2、Rust、React/TypeScript。

**[下载 v1.0.4 安装包](https://github.com/yuan-me/easy-switch/releases/download/v1.0.4/Easy.Switch_1.0.4_x64-setup.exe)** · [最新正式版](https://github.com/yuan-me/easy-switch/releases/latest) · [更新记录](CHANGELOG.md)

v1.0.4 修复切换 API 时“分页历史偏移未对齐事件边界”，正确区分同一会话的不同历史文件，并保留 v1.0.3 的重复索引修复。无需手动删除会话或数据库。

## 安装与使用

1. 下载并运行 `Easy.Switch_1.0.4_x64-setup.exe`，安装到当前 Windows 用户。缺少 WebView2 时，安装器会联网下载其引导程序。
2. 打开 Easy Switch，在“设置”中确认 Codex 数据目录和桌面程序路径。
3. 在“供应商”中添加配置并切换。使用 Sub2API 图片能力时，选择 Responses 协议并开启“Sub2API 图片工具兼容”，完整重启 Codex 后在新会话验证。

普通使用无需安装 Rust 或 Node.js。`test-channel` 是独立验收通道，日常使用请选择正式版。

## 功能

- 官方登录、混入 API、纯 API、聚合路由；Responses 透传和 Chat Completions 转换；按会话、轮转、权重与故障转移。
- Sub2API 图片工具兼容开关。使用 `requires_openai_auth=false` 和指定 actor header，保留功能开关；不添加 `image_generation` 配置项。客户端工具注册和实际出图分别验收。
- 会话搜索、分页、预览、Token 统计、Markdown 导出、归档恢复、Provider 修复、项目关联迁移及可恢复删除。
- 当前 Windows 用户的 DPAPI 凭证与备份加密；提交日志、摘要校验、外部修改与 SQLite WAL 保护。关闭 Easy Switch 窗口不终止仍在服务的独立 Runtime。
- 系统/浅色/深色主题；供应商编辑抽屉、会话分栏、窗口尺寸与阅读位置保存。
- GitHub Releases 签名更新，支持自动下载和手动检查。

## 自动更新

默认在启动后后台检查更新，运行期间每 24 小时再检查一次；发现新版后自动下载，在“设置 → 软件更新”中确认安装并重启。可分别关闭自动检查和自动下载，也可点击“检查更新”。

安装前要求 Codex 已正常退出、代理无活动请求且无会话修改操作。下载失败可重试，签名或版本校验失败时拒绝安装；配置和会话存放在独立数据目录。需要回退时，可从 Releases 重新安装上一版本。

## 已验证与当前边界

- Windows 11 虚拟机：Sub2API 新会话实际出图、官方/API 往返切换、分页历史元数据扩容及覆盖安装。
- GitHub 真实更新：测试版 `test.1 → test.2` 自动下载、签名校验、确认安装、自动重启及旧包重装；配置、DPAPI 凭证、会话和阅读位置保留。正式版更新地址也已验证。
- v1.0.4：99 项 Rust 测试、3 组界面测试和 9 项隔离原生烟测通过；合成数据复现旧版偏移报错，修复后覆盖多文件索引、官方/API 往返切换及回滚。GitHub CI、发布包签名、图标及公开更新清单/下载验证通过。本轮未在报告问题的另一台电脑上复验，也未重新执行 VM 安装。

实际官方账号请求、混入模式、历史会话继续发送、完整批量故障场景，以及 Windows 系统 DPI 和长期资源验收仍未全部覆盖。详见 [验证记录](docs/VALIDATION.md)，不将构建成功视为全部功能验收完成。

## 数据与迁移

正式通道使用 `%LOCALAPPDATA%\EasySwitch`，测试通道使用 `%LOCALAPPDATA%\EasySwitchTest`。首次启动会检查旧 `%LOCALAPPDATA%\CodexSwitch`，导入供应商、设置、DPAPI 凭证与备份引用，保留旧目录。必须在原 Windows 用户下执行；DPAPI 不能直接跨账户迁移。

旧备份路径继续指向旧目录，不要手动删除或移动。合成旧格式测试及虚拟机真实旧备份隔离恢复已通过。遇到未完成操作，先恢复再继续；检测到外部改动时不会强制覆盖。

切换操作会正常退出 Codex、备份与修改、校验并重启。请先完成正在生成的任务。项目关联迁移只更新历史会话的目录与项目索引，不移动项目代码。Chat 转换不能承载原生图片、加密历史或原生 compact，会明确报错。

## 开发

需要 Node.js 22+、Rust stable/MSVC、Windows SDK、WebView2 Runtime。

```powershell
npm ci
npm run tauri -- dev
```

构建内置前端的桌面程序，或生成安装包：

```powershell
./scripts/build.ps1
./scripts/build.ps1 -Installer
```

签名发行包使用 `scripts/prepare-signing.ps1` 生成一次密钥，再运行 `scripts/build-signed.ps1`。不要重新生成正在使用的签名密钥；私钥及密码材料放在 Git 忽略的 `.local` 目录。保护并另外安全备份它们，丢失私钥将无法向已安装应用发布更新。

## 测试

```powershell
cargo test --workspace --locked
npm run build
npx playwright test
node scripts/native-smoke.cjs
```

最后一项需要已构建的 release EXE，通过仅监听回环地址的 WebView2 调试端口验证 Easy Switch 自身，并使用独立合成目录。浏览器预览明确标记“合成数据”；它不能代替原生应用或 VM 验收。

架构见 [ARCHITECTURE.md](docs/ARCHITECTURE.md)，界面约定见 [DESIGN.md](DESIGN.md)，发布与通道隔离见 [RELEASING.md](docs/RELEASING.md)。
