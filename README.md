# Easy Switch

Windows x64 的 Codex 供应商与会话管理工具，使用 Tauri 2、Rust、React/TypeScript。

**当前为本地验收候选版，尚未发布正式版本。** 真实 Sub2API 作图、旧安装升级及完整虚拟机验收未完成；通过构建或合成测试不能替代这些验收。最新边界见 [验证记录](docs/VALIDATION.md)。

## 功能

- 官方登录、混入 API、纯 API、聚合路由；Responses 透传和 Chat Completions 转换；按会话、轮转、权重与故障转移。
- Sub2API 图片工具兼容开关。使用 `requires_openai_auth=false` 和指定 actor header，保留功能开关；不添加 `image_generation` 配置项。客户端工具注册和实际出图分别验收。
- 会话搜索、分页、预览、Token 统计、Markdown 导出、归档恢复、Provider 修复、项目关联迁移及可恢复删除。
- 当前 Windows 用户的 DPAPI 凭证与备份加密；提交日志、摘要校验、外部修改与 SQLite WAL 保护。关闭 Easy Switch 窗口不终止仍在服务的独立 Runtime。
- 系统/浅色/深色主题；供应商编辑抽屉、会话分栏、窗口尺寸与阅读位置保存。
- GitHub Releases 签名更新：启动后台检查，24 小时轮询；自动下载、用户确认安装。安装前要求 Codex 已退出、代理无活动请求且无会话修改操作。

## 数据与迁移

正式通道使用 `%LOCALAPPDATA%\EasySwitch`，测试通道使用 `%LOCALAPPDATA%\EasySwitchTest`。首次启动会检查旧 `%LOCALAPPDATA%\CodexSwitch`，导入供应商、设置、DPAPI 凭证与备份引用，保留旧目录。必须在原 Windows 用户下执行；DPAPI 不能直接跨账户迁移。

旧备份路径继续指向旧目录，不要手动删除或移动。合成旧格式恢复测试已通过，真实旧备份兼容性仍待 VM 验收。遇到未完成操作，先恢复再继续；检测到外部改动时不会强制覆盖。

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
