# Remote-SSH 实际安装记录

文中的主机名、用户名和用户目录已替换为示例名称。

日期：2026-10-30（Asia/Shanghai）。本记录与 2026-10-29 的合成数据测试区分；直接观察和用户实际操作回报分别注明证据来源。

## 已确认

- 已通过用户输入 SSH 密码的 VS Code 窗口连接目标服务器；主机名 `test-server`，Linux x86_64，用户 `test-user`，用户目录 `/home/test-user`。
- 初始只读检查：默认 Codex Home 为 `/home/test-user/.codex`，尚未初始化，没有认证文件，也没有安装官方 Codex。此服务器没有可直接用于验收的既有 Codex 聊天，需要先建立测试聊天。
- Easy Switch `0.1.0` 已通过本机 VSIX 安装至服务器。远端安装日志确认目录 `/home/test-user/.vscode-server/extensions/easy-switch.easy-switch-remote-0.1.0`；VS Code 的 SSH 扩展区域显示已安装、启用。
- 官方 Codex `26.1021.22027` 已安装至 `/home/test-user/.vscode-server/extensions/openai.chatgpt-26.1021.22027`。安装来自 OpenAI 官方 Marketplace Linux x64 原包，没有修改官方扩展或放宽版本白名单。
- 在服务器 Bash 中实际运行该扩展自带的 Linux CLI `--version`，返回 `codex-cli 0.164.0-alpha.11.1`。同一检查确认服务器上的 Easy Switch Linux x64 helper 具有可执行权限。
- 准备窗口中 Easy Switch 已加载中文入口，并在官方扩展被临时禁用时提示先安装和启用官方扩展；该状态下没有执行切换。
- 正常重启并重新连接后，原生侧边栏实际显示 `SSH · test-server`、`/home/test-user/.codex`、`官方 ChatGPT 已配置`，目录提示中的数据库路径为 `/home/test-user/.codex/sqlite`，并显示“添加服务商”和“刷新”按钮。此结果来自扩展实际调用远端 helper 的 `inspect` 返回，证明 Linux 工具可以运行；“已配置”仅表示磁盘配置，不表示账号登录或上游请求通过。
- 代理持久配置及窗口重载后，用户完成官方 ChatGPT 测试并确认收到 `OK`。随后实际检查 SSH 窗口，也可见 Codex 中“只回复OK”的测试输入和助手回复 `OK`。新后端已读取正确代理，TLS 校验保持开启。

## 安装问题及处理

远端直接运行 `code --install-extension openai.chatgpt@26.1021.22027` 失败。Remote-SSH 日志显示，服务器读取官方 CDN manifest 时出现 `self-signed certificate`。该错误发生在下载清单时，不能据此判断版本不存在。

改为本机下载 [OpenAI 官方 Linux x64 VSIX](https://openai.gallerycdn.vsassets.io/extensions/openai/chatgpt/26.1021.22027/1792790312726/Microsoft.VisualStudio.Services.VSIXPackage)，再通过远程窗口的“从 VSIX 安装”传入服务器，安装成功。没有关闭 TLS 校验或更改系统证书信任。

- 官方 VSIX 大小：428,074,098 字节。
- 官方 VSIX SHA-256：`9a5c1aa4b50d0eb9b9c06c3d7ffd18e1e7fa4591da85b1973bd1629e4db75fa9`。
- ZIP 清单确认 `openai.chatgpt`、版本 `26.1021.22027`、平台 `linux-x64`。
- Linux CLI 包版本 `0.164.0-alpha.11.1`，目标 `x86_64-unknown-linux-musl`。
- Linux CLI SHA-256：`e6d386e8822288f6d5f327e966a5a5e1de928de8795bb6149716005a410f38f2`，与此前核查的同平台二进制一致。

初始 VS Code 主进程带有 `--disable-extension openai.chatgpt`，后续新窗口会继承该环境禁用参数。已正常关闭两个安装用窗口，确认 Code 进程退出，再不带禁用参数重新启动远程连接。未终止其他服务器进程。

VS Code 在退出重启时自动从 `1.143.1` 更新至 `1.144.0`（修订 `2c33b50b16c472efe58357d1d2a8a5414bc23fbf`），Remote-SSH 同步了对应服务器组件。用户再次输入 SSH 密码后连接恢复，正常启用后的侧边栏检查通过。

## 尚未验收

- 测试 API 供应商切回官方的往返恢复，以及第二个 API 服务商之间的实际切换。
- 切换后的新聊天、线程 ID 与正文/工具记录逐项核对、分页历史和归档状态。
- 实际 SSH 断连、占用冲突与中断恢复。

## 0.1.2 真实切换回报（2026-10-30）

用户在收到 0.1.2 VSIX 安装、一次性只读管理员检查器安装及切换测试步骤后，明确回复：“已切换，原聊天收到 API OK”。据此记录官方切换到测试 API 供应商及原聊天继续发送通过（用户操作回报）。未独立读取这次服务器安装日志、sudoers 规则、进程审计输出或请求路由；不能仅凭该回复证明这些内部细节或完整验收矩阵均已通过。

未读取或记录账号令牌、API 密钥。上述单向切换成功不代表 API→官方、API A→API B、新聊天及全部历史完整性检查均已完成。
