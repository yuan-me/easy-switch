# 发布与更新

目标仓库为 `yuan-me/easy-switch`。仓库只应包含本目录内的 Rust/Tauri 新版源代码、锁文件、测试、设计文档及工作流；旧 C# 项目、虚拟机资料、真实会话和本地密钥不进入仓库。

## 首次准备

1. 完成 `VALIDATION.md` 中的发布门槛。
2. 在 GitHub 创建上述公开仓库并推送新版。配置 `release` Environment；签名私钥和密码分别存入 `TAURI_SIGNING_PRIVATE_KEY` 与 `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` Secrets。只内置 `src-tauri/tauri.conf.json` 的公钥，绝不把私钥上传为代码、附件或日志。
3. 本地密钥由 `scripts/prepare-signing.ps1` 创建，密码单独以 DPAPI 保存。`scripts/build-signed.ps1` 在内存中解密并传递环境变量，结束后清除。保留安全离线备份；不要轮换正在使用的公钥，除非实施了兼容迁移。
4. 运行 Check 工作流。发布工作流遇到缺少私钥或公钥会直接失败。

## 正式发布

同步 `package.json`、应用 Cargo package、Tauri 配置与锁文件版本；更新 CHANGELOG。手动触发 Release 工作流，它构建 Windows x64 NSIS、更新包签名和 `latest.json`，先创建 **draft**。确认 VM 安装与功能结果后，再发布 draft。发布只保留此入口，避免公布草稿时创建版本标签而再次构建同一版本；已公开的版本不要重复构建或替换安装包。

正式应用读取：

```
https://github.com/yuan-me/easy-switch/releases/latest/download/latest.json
```

每个安装包签名绑定自身版本；`requireSignedVersion=true` 防止把旧包签名配上伪造的新版本号。应用不会自动确认安装，Codex 仍运行或代理有请求时拒绝安装。下载失败可以重试，签名失败永不安装。

## 隔离测试通道

手动运行 Test channel，版本须如 `0.9.0-test.1`。工作流使用 `test-channel` Cargo feature 与 `tauri.test.conf.json`：

| 内容 | 正式 | 测试 |
|---|---|---|
| 安装名称 | Easy Switch | Easy Switch Test |
| identifier | me.yuan.easy-switch | me.yuan.easy-switch.test |
| 数据目录 | EasySwitch | EasySwitchTest |
| 清单 | latest release/latest.json | test-channel/latest-test.json |
| 发布类型 | stable | prerelease |

先安装 test.1，配置测试数据，再构建并发布 test.2。观察 test.1 检查、下载、签名验证与确认安装，重启后检查供应商、DPAPI 密钥、会话文件摘要、线程 ID、主题与阅读位置。模拟网络失败和活动代理请求；安装完成后也必须测试旧版本重新安装恢复。Test channel 会覆盖测试清单，保留各版本安装资产；不覆盖正式清单。

## 失败处理与旧版退役

失败时保存脱敏的错误、操作 manifest、文件摘要和版本，保留当前程序、旧安装包及数据目录。跨文件数据恢复必须通过应用校验；不要手动覆盖有新写入的数据库。

全部关键门槛通过且用户明确确认新版可用后，先保存可恢复的旧源码归档，再移除旧工作副本。目前未获该确认，旧版保留。
