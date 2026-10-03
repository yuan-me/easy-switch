# 架构与数据边界

`crates/core` 承担供应商、配置、DPAPI、操作日志、会话与数据库变更、桌面适配、协议和 Runtime。`src-tauri` 是命令、进度事件和更新生命周期；`src` 仅展示与交互，通过 Tauri IPC 调用业务层，不直接读取凭证、SQLite 或启动进程。

## 事务

写入由应用内 busy guard 与跨进程文件锁串行化。配置在退出 Codex 前预检查，确认桌面进程及其子进程全部退出后再扫描文件。SQLite 在退出后 checkpoint、使用 backup API 创建副本、修改副本并执行 integrity_check。JSONL 只改 session_meta 中的目标字段，其余行按原字节保留。

Journal 为每个目标保留加密 preimage、加密暂存、前后 SHA-256 摘要。状态为 Preparing → Applying → Complete；失败恢复为 Restored。启动后未完成的日志会阻止下一次业务写入。显式恢复先记录 Restoring，再验证所有备份和文件，恢复期间再校验每次写入。源文件已外部修改或数据库仍有 WAL/journal 时拒绝覆盖。

Windows 路径统一扩展路径、UNC 和分隔符比较；拒绝设备命名空间、目录越界、链接与重解析点。SQLite 只修改已知字段，对不支持的 schema 停止写入；远程 host 目录行不变。项目迁移同步 cwd 与 project_id。

## 代理

独立 `easy-switch.exe --runtime --store <目录>` 只监听 127.0.0.1，使用随机 DPAPI 保存的本地 Bearer 凭证，拒绝带 Origin 的浏览器请求，限制并发与请求体大小。Responses 原样转发工具与返回正文，仅按成员覆盖模型；Chat 适配明确拒绝无法表达的能力。

有状态或含工具的请求不做自动跨成员重放。仅无状态、无工具请求遇到明确 429/503 时允许故障转移；传输错误可能发生在服务端已接收之后，不重放。运行窗口关闭后 Runtime 可继续服务，退出前必须排空请求。

## 凭证与诊断

API Key 和自定义请求头值在供应商文件中通过 DPAPI 加密。列表只返回 key 是否存在与请求头名称；空值保留已保存值。Codex 自己需要的 auth.json/config.toml 仍按其原生格式写入，不能把这两个文件当作可公开材料。日志不记录正文或凭证。诊断发送合成内容，不执行返回的工具。

## 更新

使用官方 Tauri updater 校验更新包与签名内版本。检查/下载与安装分别受更新锁和业务 busy guard 保护。安装检查 Codex 写入者与 Runtime 活动数，随后调用 Runtime 原子排空接口。签名失败不进入 ready，下载失败可重试，安装启动失败保留下载内容。安装包默认按当前用户安装，检查 WebView2；用户数据目录独立于程序目录。

正式与测试通道使用不同 identifier、安装入口、数据目录和清单 URL。测试通道只接收 test-channel prerelease，正式通道只读取 latest release。真实安装回滚仍需 VM 验收。

## Token 用量（1.1.0）

新增只读 IPC `usage_report({query:{bucketStarts:number[],end:number,model:string|null}})`。
时间参数为 Unix 秒，分组左闭右开；前端用本机时区产生小时／日边界，周一作为周起点。最多 32 个分组，最大范围 32 天。

核心按实际 JSONL 文件扫描，不调用会话详情的继承历史回放；用量缓存按文件长度和修改时间失效。累计计数转增量，计数重置时使用当次用量；引用历史仅用于建立累计基准。可识别的复制事件先归入较早的原始历史，缺失稳定标识的情况不臆测关联。模型从 turn_context 获取，供应商配置不代替历史模型。

输出包含 input/output/cached、各时间桶、模型分布、会话排行及警告。总量是输入与输出之和；缓存属于输入，未知缓存返回 null。扫描到未完成写入、无效时间或缺失基准时告知统计不完整，不修改会话、索引或供应商。首次汇总需要读取现有历史，后续复用未变化文件；当前未引入持久化统计数据库。
