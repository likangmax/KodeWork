# Kodework Windows 架构说明

> 状态：架构基线文档。实现以当前 Rust workspace、测试门禁和 ADR 为准。

## 1. 分层

```text
┌──────────────────────────────────────────────┐
│ React + TypeScript UI（src/）                │
│ Host 侧栏 · 项目区 · 终端/文件/活动页签       │
└───────────────┬──────────────────────────────┘
                │ typed IPC invoke / Tauri Channel（M1）
┌───────────────▼──────────────────────────────┐
│ src-tauri（kodework-tauri）：薄壳              │
│ 窗口 · 托盘(M1) · 单实例(M1) · typed commands│
└───────┬──────────┬───────────┬───────────────┘
        │          │           │
┌───────▼───┐ ┌────▼─────┐ ┌───▼─────────────┐
│ kodework-   │ │ kodework-  │ │ kodework-        │
│ storage   │ │ secrets  │ │ platform-win   │
│ SQLite    │ │ CredMan/ │ │ 生命周期策略    │
│ migrations│ │ DPAPI(M1)│ │ 托盘/自启(M1)   │
└───────────┘ └──────────┘ └─────────────────┘
        domain ← core ← adapters（依赖方向）

适配器边界（M1 实现真实后端）：
- kodework-ssh：russh 连接/PTY/host key/forwarding
- kodework-sftp：russh-sftp 流式传输 + TransferManager
- kodework-network：候选地址排序与失败回退
- kodework-tailscale：tailscale.exe status --json 解析与后端计划
- kodework-herdr：远端 herdr CLI/schema 适配（可选）
- kodework-testkit：fake 适配器（离线测试）
```

## 2. 依赖方向（禁止违反）

- 允许：domain ← core ← adapters/platform ← src-tauri；domain ← storage/secrets/network/ssh/sftp/herdr；frontend DTO ← src-tauri。
- 禁止：domain 依赖 Tauri/Tokio/React/文件系统；core 依赖 tauri::AppHandle；前端接触 russh/文件系统路径/secret 值；herdr 修改 SSH 状态机；tailscale 决定认证与 host key 策略。

## 3. 关键决策（ADR 摘要）

| 主题 | 决策 | 文档 |
|---|---|---|
| 桌面架构 | Rust 核心 + Tauri 2 薄壳 + Web UI | ADR-0001 |
| 存储与秘密 | SQLite 只存元数据；凭据只存 {provider, opaque_id} 引用；Windows Credential Manager/DPAPI | ADR-0002 |
| IPC | 小 typed command 控制面 + 有界有序 Tauri Channel 数据面；Rust 侧按 8–16ms/4–32KB 聚合 | ADR-0003 |

## 4. 当前实现状态（M0）

- 领域模型：Host/Address/Project/Action/Run/Session/Transfer/Tunnel/Snippet、状态机、危险命令分类、校验 —— crates/kodework-domain
- 连接状态机与 generation guard、Action 计划 —— crates/kodework-core
- SQLite schema v3（hosts/addresses/projects/actions/runs/sessions/transfers/settings + schema_migrations），幂等迁移 —— crates/kodework-storage
- SecretStore trait + MemorySecretStore（zeroize、Debug 脱敏）—— crates/kodework-secrets
- 候选地址排序与失败分类 —— crates/kodework-network
- SSH/SFTP 边界类型与流式策略常量（russh 未引入）—— crates/kodework-ssh、crates/kodework-sftp
- tailscale status --json 容错解析（Self/Peer 官方结构）+ 系统守护进程/userspace sidecar 启动计划（不含 auth key）—— crates/kodework-tailscale
- herdr CLI argv 构造与 schema 容错解析 —— crates/kodework-herdr
- 生命周期策略（托盘/自启/退避重连）—— crates/kodework-platform-win（src-tauri 复用，无重复实现）
- FakeRemoteHost 故障建模 —— crates/kodework-testkit
- Tauri 薄壳：list_hosts/save_host/delete_host 三个 typed 命令 + %LOCALAPPDATA%/Kodework/kodework.sqlite3 —— src-tauri
- 前端：React 单页壳；浏览器模式 = Preview（不写盘）；桌面模式经 Tauri IPC 读写 SQLite；不伪造连接/上传 —— src/

## 5. 数据流（M1 目标形态）

```text
xterm.onData → send_input(command) → 有界 mpsc → SessionManager → russh Channel
远端输出 → 聚合器(8–16ms/4–32KB) → Tauri Channel<TerminalFrame> → xterm.write()
传输进度 → TransferEvent 帧（id/done/total/speed/status/code）
```

## 6. 安全边界（已在代码中强制）

- workspace lints：unsafe_code = forbid；unwrap/expect/panic = deny
- 凭据不进入 React state / localStorage / SQLite 普通列 / 日志（CredentialRef 只存引用）
- 地址输入拒绝 shell 控制字符；远端命令用 argv/结构化参数，不拼接 PowerShell 字符串
- Host key 变化 = 硬失败（M1 实现时保持；禁止 Ok(true) 无条件接受）
- Tauri capabilities 最小权限（当前 core:default，随 M1 命令最小化扩展）

## 7. 质量门禁（每次提交前）

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm run lint        # oxlint --ignore-pattern references --ignore-pattern target
npm run build
```
