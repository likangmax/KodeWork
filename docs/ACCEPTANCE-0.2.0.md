# Kodework Windows 0.2.0 验收记录

验收日期：2026-08-16  
工作区：`D:\OneDrive\AAA_KK\MYCODE\redock-windows`

## 已完成并在本机验证

- `App.tsx` 已从 1600+ 行降至约 1300 行，终端、文件、运行时、设置、主机编辑、工作区头部均为独立 React 模块。
- 文件面板只有一个滚动视口；远程文件列表按固定行高、overscan 和 ResizeObserver 进行窗口化渲染，适用于超大目录。
- SSH 认证支持密码、自定义 OpenSSH 私钥、Windows OpenSSH Agent/Pageant、keyboard-interactive/MFA。
- 自定义 Ed25519 私钥已通过 fake SSH server 的真实 russh 认证测试。
- keyboard-interactive broker 已覆盖请求、响应、超时和清理。
- 认证模式切换不会复用旧模式的凭据；服务端以持久化 Host 为权威，前端不能伪造凭据引用。
- 20 个并发终端 pane 已通过隔离和上限测试，21 个会被拒绝。
- 远程会话断线、窗口 focus、网络 online、可见性恢复均触发合并后的状态恢复探测。
- SFTP 大文件流式传输、暂停、恢复、重试、取消和 `.part` 原子完成路径已有自动化覆盖；512 MiB release 流式往返测试通过。
- updater manifest 生成脚本、静态托管 Caddy 配置、发布校验脚本和 Authenticode 构建参数已加入仓库。

## 自动化门禁证据

以下命令在本次交付前均通过：

```text
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features --quiet
npm run lint
npm run build
npm audit --audit-level=high --registry=https://registry.npmjs.org
cargo audit
```

`cargo audit` 仅报告 17 个已知传递依赖的维护/历史 soundness 警告（GTK3、glib、unic 等），未报告漏洞项；它们来自 Tauri/GTK 依赖链，已记录为后续升级跟踪项。

本地浏览器走查结果：启动页、主机编辑、密码/私钥/Agent/MFA 四种认证模式切换无控制台错误；文件面板的滚动容器结构已修正。

## 0.2.0 交付包

- MSI：[Kodework Windows_0.2.0_x64_en-US.msi](../target/release/bundle/msi/Kodework%20Windows_0.2.0_x64_en-US.msi)
- SHA-256：`4CDA55FCC0568598A92664F4A5D26F3497794DF0E63F114FEC5C6057D23E3C8B`
- 大小：24,764,416 bytes
- Tauri updater `.sig`：已生成，428 bytes
- 本地 updater channel：已用 `scripts/publish-update.ps1` 生成并校验 `latest.json`；产物位于用户临时目录的 `kodework-channel-020-final\stable`。

## 尚不能声称已经完成的外部事项

这些事项需要用户或发行环境提供外部资源，本机没有伪造完成：

1. `https://updates.kodework.dev` 当前没有可用的在线 origin/DNS，真实 URL 探测失败；仓库只提供可直接部署的静态 channel 和 Caddy 配置。
2. 本机没有商业 Authenticode 证书或 HSM。当前 MSI 的 `Get-AuthenticodeSignature` 状态为 `NotSigned`。设置 `KODEWORK_CERT_THUMBPRINT` 并准备时间戳服务后，发布脚本会强制签名并验证。
3. 本机 Windows OpenSSH Agent 服务处于 stopped/disabled，无法声称真实 Agent 服务器认证已成功；Agent 代码和 fake/协议路径已完成，需启用服务并加载 identity 后做真机验收。
4. 睡眠恢复、持续断网、多小时传输和 20 终端长时间并发的物理矩阵已提供脚本与记录模板，但本次没有虚构“连续数小时已运行”的结果。执行入口为 `scripts/run-soak-matrix.ps1`，矩阵定义见 `docs/TEST-MATRIX-WINDOWS.md`。

## 发布前必需的外部输入

- updater：DNS、HTTPS 证书、静态存储或 S3 凭据；部署 `deploy/updater/Caddyfile`，再运行 `scripts/verify-release.ps1`。
- Authenticode：商业代码签名证书 thumbprint（或 CI/HSM 签名实现）、时间戳 URL；在干净 Windows VM 上验证 SmartScreen/升级安装。
- 真机压力：启用 Windows SSH Agent，准备测试 tailnet/SSH 主机，执行睡眠、断网、20 pane、>RAM 文件传输和 8 小时混合负载矩阵，并把结果追加到 `docs/TEST-MATRIX-WINDOWS.md`。

