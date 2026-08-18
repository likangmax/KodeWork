# Kodework Windows 深度审计记录（2026-08-15）

## 审计结论

本轮没有把既有“门禁全绿”等同于“产品无缺陷”。审计覆盖 Rust/Tauri 分层、SSH/SFTP/Action/Tailscale/凭据边界、并发与进程 I/O、React 生命周期、首屏性能和 960×620 最小窗口视觉检查。

当前主链路具备可靠基础，但仍不能声称完整达到 Redock iOS 的内置 Tailscale parity。现有可用实现是系统 Tailscale daemon/CLI 地址发现与普通 SSH；`EmbeddedUserspace` 只有启动规划模型，尚未具备受管 `tailscaled` 生命周期、登录状态机、SOCKS5 到 russh 的数据路径和安全更新机制。该缺口必须在后续里程碑完成并经过真实网络故障测试后，才能对外标记“内置 Tailscale”。

## 本轮确认并修复的问题

1. Action 的 `confirmation=Always` 曾可被非危险命令绕过；现由 Rust core 统一执行确认策略，危险命令即使前端声明安全也必须确认。
2. Action 的 `cwd` 和环境变量曾直接拼接到远程 shell；现校验变量名、拒绝控制字符并使用 POSIX 安全引用。`~/...` 使用 `$HOME` 展开，避免把波浪号错误地单引号化。
3. Background Action 曾与 Quick Action 一样运行在普通 exec channel，断线后不具备持续性；现创建独立 detached tmux session，并返回 `tmux:` 引用。
4. Tauri `run_action` 曾信任 renderer 传入的完整 Action；现以 SQLite 中的 Action/Project 为权威，验证 Action 属于当前 Host 后才执行。
5. RunRepository 曾存在但未接线；现 Quick/Background 执行前写入 Running，结束后更新状态、退出码、远程 session 引用和输出字节数。Interactive Action 不伪造完成记录。
6. Herdr socket 路径曾存在 shell 引用风险，停止 bridge 的 SSH 错误被静默忽略；现安全引用并向 UI 传播失败。
7. Credential Manager/DPAPI 适配器曾未接入桌面壳；现可从 `auth_ref` 读取 Windows Credential Manager 密码，并提供“记住密码”入口。SQLite 只保存 opaque reference，删除 Host 时清理关联密码。
8. 密码曾放在 React state 中；现使用非受控 password input，提交后立即清空，避免出现在 React state/devtools 快照。Rust 端使用 zeroizing 容器。
9. Tailscale CLI stdout/stderr 曾顺序读取且忽略读取错误，存在异常输出下管道阻塞风险；现并发读取、16 MiB 硬上限、读取错误和超限错误均类型化。
10. xterm.js 曾进入首屏主 bundle；现使用 lazy/Suspense 分块。主 JS 从约 526 KiB 降至约 243 KiB，terminal chunk 约 284 KiB，Vite 不再产生 500 KiB 警告。
11. React hook 依赖曾有两条 lint 警告；现为 0 警告。
12. Windows secrets 测试曾输出固定测试凭据文本；现只输出字节长度，不向测试日志暴露内容。

## 新增或强化的回归测试

- Action data fields shell quoting、非法环境变量名、控制字符拒绝、`~/` 路径展开。
- Background Action 必须进入 detached tmux 并返回 session reference。
- Run 完成记录必须保存 remote reference 和 output bytes。
- 既有全量测试继续覆盖 host-key 改变硬失败、SSH flood、Jump Host、断线状态、SFTP 大文件与故障恢复、隧道并发/半关闭、Herdr bridge、Tailscale daemon 回退等。

## 验证结果

- `cargo fmt --all -- --check`：通过。
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`：通过。
- `cargo test --workspace --all-features`：通过。
- `npm run lint`：通过，0 警告。
- `npm run build`：通过，无大 chunk 警告。
- 浏览器预览视觉检查：默认窗口、1024×700、应用最小窗口 960×620 无水平溢出；Host 编辑和密码弹窗的焦点、标签、空状态可用。800×600 会溢出，但 Tauri 明确限制最小宽高为 960×620，因此不属于可达窗口状态。

## 未完成且不得虚报的事项

1. 真正的 Embedded Tailscale userspace backend。
2. 用户提供的真实远端在 Windows 成品中的端到端验证；禁止使用聊天中泄露过的 auth key，必须换用新建、短期、最小权限且可撤销的测试凭据。
3. Wi-Fi/Tailscale 真实断开恢复、CJK/IME、20 个活跃终端、真实 Herdr/tmux、长时间后台运行和 Windows 睡眠恢复矩阵。
4. updater 服务端仍是占位地址；MSI 仍缺 Authenticode 商业证书。
5. 退出应用时对本地活跃 transfer/tunnel 的确认与有界停止流程仍需产品化。
6. Action 运行历史已有持久化，仍需补完整历史列表、筛选、重试/重新附着 UI。

## 后续优先级

P0：Embedded Tailscale 架构与实现、真实端到端故障矩阵、退出生命周期。

P1：Run History UI、凭据管理页（更换/忘记密码、私钥/DPAPI）、20 终端与长时压力测试。

P2：App.tsx 按 workspace/files/activity/settings 拆分、文件列表虚拟滚动、正式更新服务器和 Authenticode。
