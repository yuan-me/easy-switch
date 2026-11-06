# VS Code 扩展验证记录

更新：2026-10-30（Asia/Shanghai）。

## 0.1.3 名称、图标、文案和兼容性

- 显示名称改为 Easy Switch，扩展 ID 保持 `easy-switch.easy-switch-remote`。SVG 使用同一片曲线围绕中心旋转 120° 和 240°；480 像素渲染的笔画重心为 (240.058, 240.015)，随后生成扩展列表 PNG。深浅主题和 24 像素预览均已检查。
- 精简服务商表单、状态提示、错误信息、切换确认和安装说明。保留密钥明文存储、共享目录、加密历史和恢复限制。
- Node 37 项通过，覆盖四个受支持版本的检查、停写、迁移和重载调用顺序，以及未知版本拒绝。QEMU 归档工具自检 2 项通过。
- 新增三个官方版本的 Linux x64/ARM64 隔离测试，共六组通过；每组验证 API A、API B、官方三次迁移、真实搜索重建、恢复原线程及目标模型、原始正文保留和正常退出。版本、程序校验值、日志及测试环境修正见 [兼容性说明](COMPATIBILITY.md)。
- 本轮未替换用户服务器上的 Codex，未使用真实账号或发送生成请求。新增版本的真实 Remote-SSH 聊天验收仍待完成。旧测试日志和旧 SHA-256 文件继续保留作对应版本的基线。

## 0.1.2 受保护进程检查、模型记忆及图标

- 用户批量诊断确认，同用户的 `systemd`、`(sd-pam)`、`ssh-agent`、`vmtoolsd` 也无法由普通用户检查。0.1.1 仅处理 OpenSSH 会话不足以覆盖该服务器。
- 用户明确授权新增只读管理员检查。安装脚本部署 root 所有的固定 Python 检查器和限定命令的 sudoers 规则；只允许无额外参数、隔离且禁用字节码的 Python 调用。检查器把进程 UID 与 sudo 发起者绑定，仅读取 `/proc` 的身份、argv[0]、FD 链接和打开标志，不读取环境变量、密钥或聊天，也不写应用文件或停止进程。
- 扩展和 Rust 工具仅在普通权限不足时调用检查器；发现 Codex、目标目录的可写 FD、独立数据库目录占用、身份变化、无授权、超时和未知协议时仍停止。普通用户执行实际迁移。
- 官方恢复模型在确认输入后立即按远端环境和 Codex Home 保存，失败、刷新或重载后复用；官方行可编辑备用值，服务器明确配置的官方模型优先。
- 侧边栏 SVG 改为 Easy Switch 三片旋转造型的单色线稿；新增扩展列表 PNG 图标，已渲染检查深浅主题预览。
- Node 33 项通过；Python 只读检查器 7 项通过；安装脚本 Bash 语法检查通过。Linux x64/arm64 离线 QEMU helper 回归分别 16 项通过、1 项需要外部 CLI 的测试忽略。日志和校验值见 `scripts/validation/admin-audit-*`；旧版本日志继续保留作基线。
- 真实服务器用户回报：“已切换，原聊天收到 API OK”。官方切换到测试 API 供应商 和原聊天继续发送按用户操作回报记为通过；未独立读取 root 检查器安装日志、sudoers 规则或进程审计输出。切回官方、第二个 API 服务商及其余验收项仍待验证，详见 `REMOTE_SSH_VALIDATION.md`。

## 0.1.1 SSH 会话权限修复

- 用户实际诊断：PID 12035 是 UID 1000 的 `sshd-session: guiyn@notty`，父进程 11997 为 UID/EUID 0 的 `sshd-session: guiyn [priv]`；子进程的 exe 和 fd 归 root 所有，普通用户读链接被拒绝。
- 扩展与 Linux 工具原先都把这种 OpenSSH 会话进程当作无法核实的潜在写入者。现在仅在 exe 访问被拒绝时，核对 SSH 父子进程标题、完整 UID 字段、稳定的 PID/父 PID/启动时间，以及 root 所有的 exe 链接和私有 fd 目录。匹配后排除 SSH 传输进程，其子 shell、Codex、其他写入者和未知进程仍按原保护规则检查。
- Node 28 项测试通过；包括真实 Codex 仍被列出、普通同名进程不获豁免、父进程用户或标题不匹配、额外命令参数、PID 复用及进程消失。
- 最终源码构建 Linux x64/arm64 工具；两架构离线 QEMU 内的 helper 回归分别 15 项通过、1 项依赖外部 CLI 的测试忽略。新增测试验证受保护 SSH 身份规则及错误身份拒绝。
- 本次源码和二进制校验值见 `scripts/validation/ssh-guard-sha256.json`，Linux 实测输出见 `ssh-guard-x64.txt`、`ssh-guard-arm64.txt` 及相应 JSON。下列旧日志及 `source-sha256.json` 保留为 0.1.0 基线，不能当作 0.1.1 的二进制校验值。
- 真实服务器更新、服务商切换及原聊天继续发送仍待此次修复后的验收；用户诊断输出不是已通过切换的证据。

## 0.1.0 已完成基线（2026-10-29）

- `npm test`：TypeScript 严格编译成功，27 项 Node 测试通过。
- 测试覆盖：远端登录 Shell 目录解析、Linux 大小写路径、进程 PID/starttime 身份验证、本窗口后端 Home 不同或无法识别时拒绝停止、其他窗口/CLI 阻断、服务器与 Home 间密钥隔离、凭据保存失败回退、异步保存时保留其他窗口新记录、官方模型及服务商输入校验。
- IPC 测试使用独立模拟子进程，确认密钥经标准输入交付、命令行不带密钥、进度和错误脱敏、协议异常不显示成功。
- 模拟 VS Code 宿主的命令流验证：目标校验先于停止后端；显示 helper 确认的数据库目录并采用配置指纹，实际请求保留原始环境来源，删除配置中的 sqlite_home 不会残留旧目录覆盖；部分提交后刷新恢复状态并阻止自动/手动重载；启动检查中断操作时不主动激活 Codex；未知官方版本不停止进程；重载失败不再次提交。
- 官方扩展 `26.1021.22027` 的本机静态代码已核对：后端退出不会自动重启，标准 Linux 二进制路径为 `bin/linux-x86_64/codex` 或 `bin/linux-aarch64/codex`。这属于代码核查，不等同于 Linux SSH 运行验收。
- 对应 CLI `0.164.0-alpha.11.1`、源码修订 `6905eced` 的搜索/索引重建行为已核对：仅修改 SQLite 当前模型不足以维持切换，历史重建会再次投影模型。因此迁移在记录尾部追加官方 `thread_settings_applied` 设置事件，保留旧消息、`turn_context` 和工具记录的原字节，并顺接分页序号。
- 迁移要求当前 `state_5.sqlite` 索引完整且 backfill 完成；有模型历史的线程必须具有完整、属于该线程的官方设置快照。缺失时在停止后端前拒绝操作并要求先用官方 Codex 打开或初始化，不生成猜测的权限快照。
- 原官方配置未指定模型时，GUI 在首次切换前要求确认官方恢复模型 ID。该 ID 用于恢复线程当前模型，切回官方仍保留原配置的缺省形态。
- 精确 Windows 本机 CLI `0.164.0-alpha.11.1` 实际 app-server 集成测试 1 项通过：在独立合成 Home 初始化官方真实 `threads` 表，执行 API A→API B→官方，每一步故意污染 SQLite 的模型与服务商，再通过真实 `thread/list(searchTerm)` 重建到目标，验证原历史正文保持。此项默认标为 ignored，需要明确指定 CLI 路径单独运行；本轮已显式运行并通过。[实际 app-server 输出](scripts/validation/windows-appserver-projection.txt) 随包提供。测试未使用真实账号或发起聊天请求，属于 Windows 本机的 schema、解析器与索引重建验证。

- Windows 完整核心回归：132 项通过、2 项忽略（原有的真实模型目录/app-server 和性能基准测试）；Windows helper 默认 12 项通过、上述需要显式 CLI 的集成测试 1 项忽略。上述 Node 与 Rust 测试使用合成资料，没有读取真实认证或聊天。
- 最终源码交叉构建了静态 Linux x64 和 arm64 ELF 工具；构建前后核对 [Rust 源码与清单 SHA-256](scripts/validation/source-sha256.json) 一致。
- Linux x64：核心 `remote_core` 12 项、helper 14 项全部通过；需要外部 CLI 的集成测试 1 项忽略。[实际测试输出](scripts/validation/linux-x64.txt)、[退出状态与二进制校验值](scripts/validation/linux-x64.json)。
- Linux arm64：核心 `remote_core` 12 项、helper 14 项全部通过；需要外部 CLI 的集成测试 1 项忽略。[实际测试输出](scripts/validation/linux-arm64.txt)、[退出状态与二进制校验值](scripts/validation/linux-arm64.json)。
- 两架构均实际启动了 QEMU / Alpine Linux 内核，在内存文件系统执行最终 Rust 测试二进制。环境没有网卡、磁盘和主机共享目录，QEMU 均正常关机退出。覆盖官方→API A→API B→官方、正文/线程/模型记录保留与设置事件追加、索引完整性前置检查、独立数据库目录、符号链接拒绝、私有文件权限、占用拒绝、提交与回滚、加密备份损坏检测、恢复绑定的崩溃窗口及不同 VS Code 资料/storeRoot 的持久排他绑定。
- 打包审计自动化测试 4 项通过，QEMU 镜像生成自检 2 项通过。打包脚本逐字节检查显式文件清单，验证 ELF 架构、静态链接及 ZIP 中的 Linux 可执行权限；安装包不包含密钥、聊天数据库或 JSONL 会话文件、构建工具或开发依赖，附带日志仅含合成测试信息。

以上 Linux 日志随扩展一起提供；源工作区的原始结果位于 `artifacts/vscode-linux-test/qemu-x64-final/` 和 `qemu-arm64-final/`。CI 工作流已编写，但本轮没有推送或触发托管 CI。

## 尚未完成

2026-10-30 已完成真实 Remote-SSH 安装、侧边栏及官方 ChatGPT 的 OK 回复验证；用户随后确认官方切换到测试 API 供应商 切换成功，原聊天收到 API OK。工作区记录及证据来源见 `REMOTE_SSH_VALIDATION.md`。切回官方、第二个 API 服务商、新聊天、历史完整性及异常恢复的真实验收仍待完成。不能以编译、模拟宿主或打包成功代替这些验收。

精确 CLI `0.164.0-alpha.11.1` 的历史恢复与压缩路径仍存在上游兼容边界：超长历史切到更小上下文窗口，或 compaction 配置摘要变化时，Codex 可能先在新 API 服务商调用旧模型进行压缩。如果服务商不支持旧模型，会保留原历史并报错；用户可切回原服务商。不能把迁移测试通过解释为所有长历史都能跨任意模型发送。跨服务商加密历史的不兼容限制也仍然存在。

自动停止后端只对 [兼容性说明](COMPATIBILITY.md) 列出的四个版本启用；其他版本明确拒绝写入，需测试后才能加入支持列表。

## 打包开发依赖公告

`npm audit --json` 实际报告 2 项高危：`@vscode/vsce@4.1.0` 的开发依赖 `@azure/identity@4.14.0` 引入 `@azure/msal-node`，受 [GHSA-xcx6-jx8p-x3h5](https://github.com/advisories/GHSA-xcx6-jx8p-x3h5) 影响。`npm audit fix --ignore-scripts` 未能在上游版本范围内修复。没有强制跨主版本覆盖认证库。

这是打包工具的 Azure 授权码认证路径风险。当前只执行本地打包，不进行 Marketplace 发布或 Azure 登录；VSIX 不包含 `node_modules`、vsce 或其他开发依赖，扩展运行代码仅使用 VS Code/Node 内置接口。未来使用该工具链执行发布认证前，应更新到修复版本并重新检查公告。

运行时边界保持明确：插件保存的密钥由客户端 SecretStorage 加密；当前启用的 API Key 仍以 Codex 原生格式明文写入服务器配置。部分上游绑定的加密历史可能无法跨服务商继续，插件不会删除或改写聊天正文来掩盖这种失败。
