# Easy Switch

在 VS Code 的 Linux Remote-SSH 窗口中切换 ChatGPT 与 Responses API 服务商。侧边栏显示服务器、数据目录、当前配置和切换进度。切换后可继续使用原聊天，消息正文和线程 ID 保留。

服务商在此扩展中单独添加，无需运行 Easy Switch 桌面版。Windows 桌面版的加密配置不会导入。

## 安装

1. 使用 VS Code 的 Remote-SSH 连接 Linux 服务器，并打开服务器上的项目。
2. 在扩展页面确认官方 Codex（`openai.chatgpt`）已安装在 “SSH: 你的服务器”，且能正常打开。
3. 在这个远程窗口运行命令 “Extensions: Install from VSIX...”，选择 `easy-switch-remote-0.1.3.vsix`。安装后确认 Easy Switch 也位于 “SSH: 你的服务器” 中。
4. 打开活动栏中的 Easy Switch，核对目标服务器和 Codex 数据目录。不要在目录识别错误时继续操作。

扩展包包含 Linux x64 和 arm64 工具；服务器无需安装 Rust、Node 构建工具或桌面版 Easy Switch。支持 Linux Remote-SSH，以及官方 Codex `26.1021.22027`、`26.1007.21434`、`26.1002.51308`、`26.51021.22027`。检查以服务器实际安装的版本为准，其他版本会停止切换。各版本的测试范围见 [兼容性说明](COMPATIBILITY.md)。

从 0.1.2 升级时，直接安装新 VSIX 并重载窗口即可。服务商配置和密钥保留，已安装的只读管理员检查器无需重装。

### 受保护的系统进程

部分 Ubuntu 服务（例如用户级 systemd、PAM、ssh-agent、VMware 工具）虽然属于当前用户，仍不允许普通用户读取其可执行文件和文件占用。若提示需要只读管理员检查，在远端终端运行一次（以 0.1.3 为例）：

```sh
sudo sh ~/.vscode-server/extensions/easy-switch.easy-switch-remote-0.1.3/scripts/install-proc-audit.sh
```

脚本要求 Python 3.9+ 和 sudo，安装 root 所有的 `/usr/local/lib/easy-switch/proc-audit.py`，并在 `/etc/sudoers.d/easy-switch-proc-audit-你的UID` 中仅允许当前用户免密运行这一固定检查命令。Python 使用隔离模式并禁止写入字节码；检查器只读取 `/proc` 中当前用户的进程身份与文件占用，不读取进程环境、API 密钥或聊天内容，不发送信号、不写入 Codex 文件。插件和切换工具本身保持普通用户权限。

检查器发现 Codex 进程、目标目录可写文件占用、身份变化或未知结构时仍会拒绝切换。没有安装或授权失效时也会停止，不会自动跳过受限进程。升级检查器源码后应重新运行安装脚本；密码仅在服务器终端输入。

## 使用

点击侧边栏的“添加服务商”，依次填写名称、API Base URL、实际模型 ID 和密钥。Base URL 是服务商的接口前缀，例如 `https://api.example.com/v1`，不是 `/responses` 请求地址。首版只支持 Responses，不提供 Chat Completions 转换、聚合路由或模型自动发现。

添加或编辑后，服务商配置会先保存。等服务器上的 Codex 任务结束，再点击目标服务商的 “切换并重载”。扩展会停止当前窗口的 Codex，备份并迁移配置和历史索引，校验完成后重载窗口。

如果原官方配置没有指定模型，第一次切换前会要求填写 “官方恢复模型”。请在 Codex 的模型选择器查看实际模型 ID 后填写；确认后立即按当前服务器和 Codex Home 保存到扩展持久存储，切换失败后重试或重载窗口无需再次填写。可以点击“官方 ChatGPT”行的编辑按钮修改这个备用模型；官方配置中明确指定的模型仍优先。这个 ID 用于恢复旧线程的当前模型，切回官方时仍保留原配置中 `model` 字段缺省的形态。取消输入会保留当前状态，不会停止后端。

切回 “官方 ChatGPT” 后继续使用原官方登录；未登录时请在官方 Codex 界面完成登录。扩展不收集 ChatGPT 密码，也不会替换或导出你的登录令牌。

当前 Codex Home 的配置是共享的。同一服务器用户、同一目录下的 CLI 和其他 VS Code 窗口也会在重新启动后采用该服务商。如果检测到其他相关进程仍在运行，先结束相应任务、关闭对应 Codex 客户端，再重试；扩展不会批量强杀其他窗口的进程。

同一个 Codex Home 会绑定首次切换使用的 VS Code 用户资料及其扩展数据目录。更换资料或服务器数据目录后，插件会拒绝用另一套备份密钥操作该 Home；请回到原资料完成切换或中断恢复。

本地受管配置中指定的服务商、模型或认证要求与切换冲突时，会停止操作。插件无法据此验证组织的云端策略；账号是否有权使用所选模型，需要发送消息确认。

## 密钥、历史与恢复

- 服务商密钥和备份加密密钥通过 VS Code SecretStorage 加密保存在客户端，按远端环境隔离。换一台电脑连接服务器时，可能需要重新填写密钥。
- 当前启用的 API Key 必须以明文写入服务器的 Codex 配置文件，文件限制为当前用户访问。服务器管理员仍可能读取它；不要把配置提交到 Git 或作为普通附件分享。切回官方会移除扩展管理的活动 API 密钥。
- 切换保留消息正文、工具调用、线程 ID 和项目关联，同步更新会话供应商及分页索引。服务商不接受跨账号或跨上游的加密历史时，仍可能出现 `invalid_encrypted_content`。此时保留原聊天并切回原服务商，不要删除历史片段来强行继续。
- 为防止 Codex 搜索或重建索引时恢复成旧模型，切换会在记录尾部追加官方 `thread_settings_applied` 当前设置事件，分页序号顺接；已有消息、`turn_context` 和工具记录保持原字节。
- 扩展要求 `state_5.sqlite` 的完整索引，且历史回填（backfill）已完成。具有模型历史的线程还必须包含完整、属于该线程的官方设置快照。缺少这些资料时会在停止后端前拒绝切换；请先用官方 Codex 打开或初始化对应聊天。扩展不会生成缺失的权限设置。
- 超长历史切换到更小的上下文窗口，或压缩配置摘要发生变化时，官方 Codex 可能先通过新服务商请求旧模型进行历史压缩。新服务商不支持该旧模型时仍可能报错；请保留历史并切回原服务商，不能保证所有长历史都能跨任意模型继续。
- 中断操作会保留加密备份和操作清单。重新打开窗口后，先使用 “Easy Switch: 恢复中断的切换”，再发起新切换。请使用原客户端的 VS Code 用户资料恢复，避免丢失对应 SecretStorage 密钥。
- 外部修改、忙碌的数据库、未知历史结构、无法确认进程身份或无法确认停写时，扩展会拒绝覆盖。不要手动替换仍在运行的 Codex 数据库。
- 配置已经提交但窗口尚未重载时，使用 “Easy Switch: 重载窗口”。重载后发送一条消息，确认服务商连接正常。

## 从源码构建

本地构建仅操作源码和隔离测试数据；依赖下载需要联网。`npm run package` 会检查两个真实 Linux ELF 工具、包内文件白名单和可执行权限，缺少任一架构时直接失败，不生成占位安装包。

在分别对应 x64 和 arm64 的 Linux 构建机安装 Rust stable、目标 musl 标准库和 musl C 编译器后，从仓库根目录执行对应构建：

```bash
# x64 构建机
rustup target add x86_64-unknown-linux-musl
env CC_x86_64_unknown_linux_musl=musl-gcc cargo build -p easy-switch-helper --release --target x86_64-unknown-linux-musl --locked

# arm64 构建机
rustup target add aarch64-unknown-linux-musl
env CC_aarch64_unknown_linux_musl=musl-gcc cargo build -p easy-switch-helper --release --target aarch64-unknown-linux-musl --locked
```

将两个构建产物分别放到 `extensions/vscode/bin/linux-x64/easy-switch-helper` 和 `extensions/vscode/bin/linux-arm64/easy-switch-helper`。然后使用 Node.js 22 或更新版本打包：

```bash
cd extensions/vscode
npm ci
npm test
node --test scripts/package.test.mjs
npm run package
```

安装包和 SHA-256 审计报告生成在 `extensions/vscode/dist/`。仓库的 **VS Code Extension** 工作流在两个 Linux 架构上执行测试和构建，再生成同样的 VSIX；该工作流只上传构建产物，不发布 Marketplace 或 GitHub Release。

## 验证边界

2026-10-29 已完成：TypeScript / Node 测试 27 项，Windows 核心回归 132 项通过、2 项原有测试忽略，Windows helper 默认 12 项通过、1 项集成测试忽略；Linux x64 和 arm64 均在隔离 QEMU Linux 中各通过 12 项核心及 14 项 helper 测试，均有 1 项需要外部 CLI 的集成测试忽略。两套 Linux 测试使用内存文件系统和合成聊天，未连接网络、真实账号或服务器。另在精确版本的 Windows 本机 Codex app-server 单独运行该集成测试，1 项通过，验证了实际搜索重建后的模型与服务商，未发送聊天。打包审计测试 4 项、QEMU 镜像生成自检 2 项通过。详细覆盖范围、实际日志和未完成项目见 [验证记录](VALIDATION.md)。

正式使用前，请在独立 Codex Home 中验证“官方 → API A → API B → 官方”、原线程恢复与继续发送、分页历史、SSH 中断恢复及其他客户端占用。各版本的隔离测试和真实服务器验证结果见验证记录。

## 许可证

本仓库从桌面版 1.4.0 起采用 [GNU AGPL-3.0](LICENSE)，扩展的版本编号仍独立管理。
