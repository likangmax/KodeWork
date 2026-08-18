# Goal：Kodework Windows —— Rust 远程编码工作台

> 文档版本：1.0.0
>
> 日期：2026-08-14
>
> 状态：可执行工程规范（Execution Contract）
>
> 目标读者：负责实际编码、测试、构建和交付的执行模型/工程师

---

## 0. 给执行模型的总指令

你要实现的是一个 Windows 原生桌面软件，不是静态网页，也不是简单 SSH 封装。软件名称暂定为 **Kodework Windows**。它的核心价值是：让用户从 Windows 电脑安全、快速、稳定地连接没有公网 IP 的 Linux 工作站，在远端继续运行 coding agent、终端、tmux/Herdr workspace、开发服务和文件传输。

执行本 Goal 时必须遵守以下规则：

1. **先读文档，再改代码。** 不得直接在现有 Demo UI 上继续堆按钮。先建立 Rust workspace、领域模型、错误模型和测试基础。
2. **Rust 是核心。** SSH、PTY、SFTP、Tailscale、Herdr、状态机、持久化、凭据、重连和后台任务不得由 React 实现。
3. **Tauri 只是薄壳。** `src-tauri` 只负责窗口、托盘、插件和 typed IPC command/channel；业务逻辑必须位于不依赖 Tauri 的 Rust crates。
4. **不复制参考项目代码。** `references/` 下的仓库只用于研究；任何复制的第三方代码必须有明确的许可证记录、来源 commit、原始文件和修改说明。默认策略是重新实现。
5. **不伪造成功。** 未连接真实服务器时只能显示 Demo/未配置状态；不能把 Demo 输出当作 SSH 成功。每个功能必须有测试证据。
6. **安全优先。** 私钥、密码、passphrase、token 不得进入 React state、localStorage、普通 SQLite 字段、日志、崩溃报告或命令行参数。
7. **所有网络工作可取消。** SSH 会话、SFTP 传输、端口转发、Herdr 订阅和重连任务都必须有 cancellation token、超时和明确终止路径。
8. **后台任务不得依赖 UI 生命周期。** 用户关闭窗口默认只隐藏到托盘；远端任务必须由 tmux 或 Herdr 持久化。明确点击 Quit 才允许终止本地进程。
9. **先做 M0/M1 的稳定底座，再扩展功能。** 不得在底座不稳定时加入 AI 面板、RDP/VNC、Docker 监控等非核心功能。
10. **每一个阶段都必须可验证。** 完成标准必须包含命令、测试、结果和已知限制。

如果当前仓库与本 Goal 冲突，以本 Goal 和实际测试结果为准。可以删除或重写原来的 Electron Demo，但不得删除 `references/` 研究资料或用户已有的无关文件。

---

## 1. 产品定义

### 1.1 一句话定义

Kodework Windows 是一个低资源、高可靠、键盘优先的 Windows 远程编码工作台：通过 LAN、Tailscale、公网地址或 SSH 跳板机连接远程 Linux 主机，以 Project 为工作上下文，以 SSH/PTY 为数据通道，以 tmux/Herdr 为持久化运行时，以 Action/Run/Transfer/Web Preview 为可恢复的工作流。

### 1.2 用户场景

典型场景如下：

```text
远程 Linux 工作站（无公网 IP）
        │ Tailscale
        │ 或公网 VPS / SSH Jump Host
        ▼
Windows Kodework
        │
        ├─ SSH 终端 / PTY
        ├─ tmux 或 Herdr 持久化 workspace
        ├─ Claude Code / Codex / OpenCode 等 coding agent
        ├─ Action / Quick Run / Background Run
        ├─ SFTP 文件浏览、上传、下载、断点续传
        ├─ 截图、图片、PDF、日志拖拽上传
        ├─ 远端文件预览和编辑
        └─ SSH local forwarding 的 Web Preview
```

### 1.3 目标用户

- 使用远程 Linux/GPU 工作站的开发者和研究者
- 在家庭网络、校园网、公司内网中没有公网 IP 的机器所有者
- 使用 Tailscale 或 SSH 跳板机连接多台服务器的人
- 运行 Claude Code、Codex、OpenCode、Cursor Agent、Herdr 等工具的用户
- 需要从 Windows 持续观察和恢复远端任务的人

### 1.4 明确非目标

以下内容不是 M1 核心，不得阻塞核心交付：

- 自己实现 VPN、NAT 穿透或 Tailscale 控制平面
- 自己实现完整终端模拟器
- 云端账户、云同步、厂商中继服务器
- RDP/VNC、FTP、SMB、Kubernetes、S3 等泛基础设施面板
- 远端 GPU/CPU 监控大屏
- 内置 AI 模型或代替 Claude/Codex/OpenCode
- 强制安装 Herdr、Tailscale 或远端 agent

这些能力可以通过后续 Adapter/Plugin 增加，但不能污染第一版核心。

---

## 2. 研究结论与证据边界

### 2.1 官方产品方向

Kodework 的公开产品方向围绕 Host/Project/Action/Run/Session 组织，而不是把“SSH 连接”作为唯一顶层对象。公开能力包括 Actions、Projects、tmux continuity、文件管理、Snippets、语音输入、Web Preview、图片上传、备用地址、Yazi 和 Workspace Controls。Windows 版应复刻的是这个远程编码工作流，而不是普通终端软件的所有运维功能。

### 2.2 下载的参考项目

所有参考仓库位于 `references/`，只读研究，不参与 Kodework 的 build：

| 项目 | 本地目录 | 许可证 | 主要吸收点 | 不直接采用 |
|---|---|---|---|---|
| `GOODBOY008/r-shell` | `references/r-shell` | MIT | Tauri 2、Rust、PTY、SFTP、分屏、传输队列、托盘、恢复 | localhost WebSocket 作为高频终端数据通道 |
| `zouwei/termex` | `references/termex` | MIT | Rust Core/Bridge 拆分、AI-native SSH 工作流、加密配置 | 未经分析的旧 Tauri/Flutter bridge |
| `wilsonglasser/oryxis` | `references/oryxis` | AGPL-3.0-or-later | 原生 Rust SSH、vault、多跳、key import、主机密钥 | 任何实现代码；闭源发行前必须进行许可证审查 |
| `wrolp/wrolp` | `references/wrolp` | MIT | Tauri command 分层、russh/russh-sftp、SFTP、session recording、stale-task guard | 前端状态直接拥有网络生命周期 |
| `veeso/termscp` | `references/termscp` | MIT | Rust 文件传输、队列、暂停/恢复、书签、系统 vault、性能取向 | 将其 TUI 直接嵌入桌面 UI |
| `h3nock/remux` | `references/remux` | MIT | tmux-first workspace、快捷命令、附件、图片 markup、Web preview 思路 | iOS-specific gesture/terminal core |
| `Eugeny/tabby` | `references/tabby` | MIT | 成熟 SSH UX、profile、jump host、agent forwarding、快捷键、主题 | Electron 架构和高内存常驻模式 |
| `wavetermdev/waveterm` | `references/waveterm` | Apache-2.0 | durable SSH、workspace/block、远端编辑/预览、command blocks、wsh 思路 | Go/Electron runtime |
| `xtermjs/xterm.js` | `references/xtermjs` | MIT | VT buffer、CJK/IME、WebGL renderer、search/fit/serialize addon | 自己重写 VT parser |
| `warp-tech/russh` | `references/russh` | MIT | Tokio SSH、PTY、SFTP、forwarding、keepalive、认证和错误边界 | 直接暴露 russh 类型到 UI |
| `herdrdev/herdr` | `references/herdr` | Apache-2.0 | 常驻 server/client、workspace/tab/pane、agent 状态、CLI/socket API、远程 attach | 复制 Herdr server；Kodework 只做可选适配器 |

### 2.3 许可证政策

- `MIT`、`Apache-2.0` 可在遵守 notice/版权要求的前提下参考或复用，但默认仍重新实现。
- `AGPL-3.0-or-later` 的 Oryxis 只能作为产品/架构参考，不能把实现代码或不可分离的派生代码混入本项目，除非未来明确决定采用 AGPL。
- `references/` 目录禁止被 workspace 的 Cargo/npm build 依赖。
- 若未来确实复制代码，新增 `docs/THIRD-PARTY-NOTICES.md`，记录仓库、commit SHA、文件、许可证、修改内容和发布义务。

---

## 3. 非功能需求（必须验收）

以下是目标门槛，不代表当前已经达标。执行模型必须建立基线并记录实测值。

### 3.1 性能

| 指标 | 目标 | 测量方法 |
|---|---:|---|
| 冷启动 p50 | ≤ 1.2 s | 10 次 release build 启动到可交互 |
| 冷启动 p95 | ≤ 2.0 s | 同上，排除首次安装/Windows Defender 扫描 |
| 空闲 RSS | 目标 ≤ 120 MB | Windows Task Manager/工作集采样；记录 WebView 版本 |
| UI 主线程卡顿 | 无持续 >50 ms stall | 高频输入、窗口 resize、终端输出同时进行 |
| 终端吞吐 | 10 MB 连续输出不丢字节、不死锁 | 远端 `yes`/生成 fixture，比较 checksum/计数 |
| 终端 IPC | 不逐字符调用 IPC | 8–16 ms 或 4–32 KB 批量聚合 |
| SFTP 大文件 | 10 GB 内存 O(buffer) | RSS 不随文件大小线性增长 |
| 并发传输 | 默认 2，最多 4 | 可配置但有硬上限 |
| scrollback | 每 session 固定上限 | 默认 50,000 行或 16 MB，超限丢弃最旧内容 |
| reconnect | 5 次 bounded exponential backoff | 500 ms 起步，最大 30 s；可取消 |

### 3.2 可靠性

- 远端 tmux/Herdr 中的进程不依赖 Kodework UI 是否打开。
- 窗口关闭默认 hide-to-tray，不终止会话和传输。
- 重启后恢复 Host/Project/Workspace 元数据，并尝试重新 attach tmux/Herdr。
- 网络断开进入明确 `Reconnecting`，不得假装 `Connected`。
- Tailscale daemon 未运行、远端主机不可达、认证失败、host key 变化必须分别显示。
- 传输中途取消、暂停、重试必须幂等，不产生半成品覆盖。

### 3.3 安全

- 首次 host key 必须由用户确认并显示 fingerprint。
- host key 变化是硬失败，不允许“一键继续”。
- 所有网络流量使用 SSH/Tailscale/HTTPS 等明确加密通道。
- 密码、私钥、passphrase、API token 不进入 UI persistence、SQLite 普通字段、日志或 crash dump。
- 本地命令使用 argv/结构化参数，禁止把未经验证的用户输入拼接到 PowerShell 命令字符串。
- Action 有 `DangerLevel` 和 `ConfirmationPolicy`；危险命令默认二次确认。
- Tauri capabilities 采用最小权限；renderer 不得直接访问文件系统、进程或网络。
- 更新包必须签名校验；没有签名验证不得启用自动更新。

### 3.4 可维护性

- `kodework-core` 不依赖 Tauri、React、WebView。
- 每个状态机使用显式 enum，不使用互相矛盾的 boolean 组合。
- 所有公共错误具有稳定 code、可读 message 和 source chain。
- 数据库使用版本化 migration；禁止手工改生产数据库。
- 每个网络 Adapter 都有 fake implementation，供离线测试。
- CI 必须运行 fmt、clippy、unit、integration、frontend typecheck/build。

---

## 4. 总体架构

```text
┌──────────────────────────────────────────────────────────────┐
│                    Kodework Windows UI                         │
│ React + TypeScript + xterm.js + WebGL/DOM fallback           │
│ Host Rail · Project Explorer · Terminal · Files · Runs       │
└───────────────────────────┬──────────────────────────────────┘
                            │ typed commands + Channel streams
┌───────────────────────────▼──────────────────────────────────┐
│                         src-tauri                            │
│ thin window/tray/single-instance/updater shell               │
│ command validation · DTO mapping · event/channel forwarding  │
└───────────────────────────┬──────────────────────────────────┘
                            │
┌───────────────────────────▼──────────────────────────────────┐
│                         kodework-core                           │
│ use cases · managers · orchestration · state machines         │
└───────┬──────────┬──────────┬──────────┬──────────┬──────────┘
        │          │          │          │          │
 kodework-ssh  kodework-sftp  kodework-storage kodework-secrets kodework-network
        │          │          │          │          │
      russh     SFTP      SQLite       Win Credential   resolver
      PTY       queue     migrations   Manager/DPAPI    fallback
        │
  kodework-tailscale ── tailscale.exe status --json / LocalAPI later
  kodework-herdr ────── remote herdr CLI/socket bridge (optional)
  kodework-platform-win ─ tray/autostart/notifications/DPAPI
```

### 4.1 Repository structure to implement

```text
kodework-windows/
├─ crates/
│  ├─ kodework-domain/          # serde models, IDs, enums, invariants
│  ├─ kodework-core/            # use cases, managers, state machines
│  ├─ kodework-ssh/             # russh, PTY, auth, host key, forwarding
│  ├─ kodework-sftp/            # remote FS and TransferManager
│  ├─ kodework-storage/         # SQLite pool, migrations, repositories
│  ├─ kodework-secrets/         # Credential Manager + DPAPI adapters
│  ├─ kodework-network/         # AddressProvider and candidate resolver
│  ├─ kodework-tailscale/       # CLI/LocalAPI adapter and tolerant parser
│  ├─ kodework-herdr/           # Herdr detection, CLI/socket/schema bridge
│  ├─ kodework-platform-win/    # tray, startup, notifications, file dialogs
│  └─ kodework-testkit/         # fake SSH/SFTP/Tailscale/Herdr fixtures
├─ src-tauri/
│  ├─ src/commands/            # thin typed command handlers only
│  ├─ src/ipc/                 # DTOs/channels and frontend mapping
│  ├─ capabilities/            # least-privilege Tauri capabilities
│  └─ tauri.conf.json
├─ src/
│  ├─ app/                     # routing, shell, global keyboard router
│  ├─ terminal/                # xterm lifecycle, renderer, search, IME
│  ├─ hosts/                   # host/address/auth UI
│  ├─ projects/                # project explorer and actions
│  ├─ files/                   # dual-pane/file preview/transfer UI
│  ├─ runs/                    # quick/background run history
│  ├─ herdr/                   # workspace/tab/pane/agent UI
│  ├─ settings/                # preferences and integrations
│  └─ styles/                  # restrained desktop design tokens
├─ migrations/
├─ docs/
├─ references/                 # read-only upstream research
└─ Cargo.toml                  # workspace root
```

### 4.2 Dependency direction

Allowed:

```text
domain ← core ← adapters/platform ← src-tauri
domain ← storage/secrets/network/ssh/sftp/herdr
frontend DTOs ← src-tauri
```

Forbidden:

- `kodework-domain` importing Tauri, Tokio runtime, React or filesystem code
- `kodework-core` importing `tauri::AppHandle`
- frontend importing russh, filesystem paths or secret values
- `kodework-herdr` changing SSH state machine internals
- `kodework-tailscale` deciding authentication or host-key policy

---

## 5. 领域模型

所有 ID 使用不可预测的 UUID/ULID；所有时间使用 UTC RFC3339/Unix milliseconds；所有数据库外键显式声明。

### 5.1 Host

```rust
struct Host {
    id: HostId,
    label: String,
    username: String,
    port: u16,
    auth_ref: CredentialRef,
    addresses: Vec<Address>,
    host_key_policy: HostKeyPolicy,
    default_project_id: Option<ProjectId>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}
```

`Host` 是逻辑机器；`Address` 是访问路径。一个 Host 可有 LAN、Tailscale、Public、Manual 多个地址。

### 5.2 Address

```rust
enum AddressKind { Lan, Tailscale, Public, JumpHost, Manual }

struct Address {
    id: AddressId,
    host_id: HostId,
    kind: AddressKind,
    hostname_or_ip: String,
    port: u16,
    priority: i32,
    enabled: bool,
    last_success_at: Option<DateTime<Utc>>,
    last_failure: Option<FailureCode>,
}
```

不要把 Tailscale 特判写进 SSH；通过 `AddressProvider` 生成候选地址。

### 5.3 Project

```rust
struct Project {
    id: ProjectId,
    host_id: HostId,
    name: String,
    remote_cwd: RemotePath,
    preferred_runtime: RuntimeKind, // Tmux, Herdr, PlainShell
    action_ids: Vec<ActionId>,
    snippet_ids: Vec<SnippetId>,
    preview_ports: Vec<u16>,
}
```

### 5.4 Action / Run

```rust
enum ActionMode { Interactive, Quick, Background }
enum DangerLevel { Safe, Review, Dangerous }
enum ConfirmationPolicy { Never, OnDangerous, Always }

struct Action {
    id: ActionId,
    project_id: ProjectId,
    name: String,
    command: String,
    mode: ActionMode,
    cwd: Option<RemotePath>,
    timeout_ms: Option<u64>,
    danger_level: DangerLevel,
    confirmation: ConfirmationPolicy,
    env: BTreeMap<String, String>,
}

struct Run {
    id: RunId,
    action_id: ActionId,
    status: RunStatus,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    exit_code: Option<i32>,
    remote_session_ref: Option<String>,
    output_bytes: u64,
    error_code: Option<ErrorCode>,
}
```

`Interactive` 必须占用 PTY；`Quick` 使用 SSH exec 并有 timeout；`Background` 必须运行在 tmux/Herdr 持久化上下文中。

### 5.5 Session

```rust
struct Session {
    id: SessionId,
    host_id: HostId,
    project_id: Option<ProjectId>,
    runtime: RuntimeKind,
    target: SessionTarget, // tmux session/window/pane or Herdr workspace/tab/pane
    status: SessionStatus,
    last_attached_at: Option<DateTime<Utc>>,
    scrollback_bytes: u64,
}
```

### 5.6 Transfer / Tunnel / Snippet

```rust
enum TransferStatus { Queued, Hashing, Transferring, Paused, Retrying, Completed, Cancelled, Failed }
struct Transfer { id, host_id, local_path, remote_path, direction, size, transferred, status, checksum, error_code }

struct Tunnel { id, host_id, remote_host, remote_port, local_port, status, session_id }
struct Snippet { id, project_id, name, text, insert_mode, sort_order }
```

截图、图片、PDF、日志和普通文件都必须复用 `TransferManager`，不能单独再实现一套上传协议。

---

## 6. 状态机与不变量

### 6.1 HostConnectionState

```text
Disconnected
  → ResolvingAddress
  → Connecting
  → VerifyingHostKey
  → Authenticating
  → Ready
  → Reconnecting
  → Ready / Failed / Disconnected
```

不变量：

- 只有 `Ready` 才能创建 PTY、SFTP 或 Tunnel。
- `VerifyingHostKey` 不允许自动绕过。
- `AuthenticationFailed` 不触发无意义重试。
- 网络错误可进入 `Reconnecting`；host key changed、权限拒绝、配置无效直接 `Failed`。
- 每次重连使用递增 `connection_generation`；旧任务的输出不得写入新连接。

### 6.2 SessionState

```text
Detached → Attaching → Attached → Suspended → Reattaching
                         └──────→ Closed / Failed
```

tmux/Herdr server 是远端持久性来源；Kodework 关闭不等于 session close。

### 6.3 RunState

```text
Created → Confirming → Queued → Running → Succeeded
                                  ├────→ Failed
                                  ├────→ Cancelled
                                  └────→ TimedOut
```

### 6.4 TransferState

必须支持暂停、取消、重试和恢复；状态转换必须幂等。成功后先完成 fsync/close，再原子 rename `.part` 文件。

### 6.5 HerdrAgentState

```text
Unknown | Idle | Working | Blocked | Done
```

`Blocked` 只表示需要用户输入/批准，不得自动向 agent 发送答案。

---

## 7. SSH、PTY 和终端数据平面

### 7.1 技术选择

- Tokio async runtime
- `russh` 负责 SSH client/channel/PTY/forwarding
- `russh-sftp` 负责 SFTP
- xterm.js 负责 VT 渲染、CJK、IME、tmux、vim、curses 和 WebGL/DOM fallback

Rust 管理字节流和会话生命周期，xterm.js 只管理显示模型。

### 7.2 数据平面

```text
xterm.onData
  → typed IPC command send_input(bytes)
  → bounded tokio mpsc
  → SessionManager
  → russh Channel
  → remote PTY/tmux/herdr pane
  → output aggregator (8–16ms / 4–32KB)
  → Tauri Channel<TerminalFrame>
  → xterm.write(Uint8Array)
```

禁止：

- 每个字符一个 IPC event
- unbounded channel
- `emit("terminal-output")` 高频传输
- 把 localhost WebSocket 作为进程内必经层
- 在 React 中做 ANSI 解析或重连判断

### 7.3 PTY 规则

- 请求 `xterm-256color` 和 truecolor。
- resize 使用 debounce，不能在拖动窗口时每个 pixel 都发远端请求。
- 终端输入必须支持 CJK IME、bracketed paste、鼠标报告和 Ctrl/Alt/Meta 修饰键。
- 记录 `TERM`, `COLORTERM`, shell 类型和远端 OS，显示在诊断面板。
- 终端输出采用固定 scrollback 上限。
- 终端 flood 测试必须验证字节计数，不只看 UI 是否“看起来正常”。

### 7.4 Host key

首次连接显示算法和 fingerprint，提供：`Trust once`、`Trust and save`、`Cancel`。

之后同 fingerprint 自动通过；变化必须硬失败并提供删除旧 key/重新确认的显式流程。绝不实现 `Ok(true)` 的无条件接受回调。

### 7.5 认证

支持顺序：SSH agent → public key → keyboard-interactive/password（按用户设置）。私钥路径可保存，私钥内容不进数据库。Windows SSH agent 的使用必须有可解释的错误。

---

## 8. Tailscale 与地址解析

### 8.1 原则

Tailscale 是网络路径/发现提供者，不是 Kodework 的认证层。认证仍通过标准 SSH 和 host key policy。

### 8.2 Adapter

```rust
trait AddressProvider {
    async fn candidates(&self, host: &Host) -> Result<Vec<AddressCandidate>, NetworkError>;
}

struct TailscaleProvider {
    executable: PathBuf, // tailscale.exe
    timeout: Duration,
}
```

第一版使用 `tailscale.exe status --json` 作为外部 CLI adapter；解析器必须 `Option<T>`、忽略未知字段，并记录 CLI version。不要绑定未经版本承诺的内部 JSON 字段。

未来可增加 LocalAPI adapter，但外层 `AddressProvider` 不变。

### 8.3 候选地址排序

默认优先级：用户手动指定 > 最近成功的 Tailscale > LAN > Public > JumpHost fallback。实际排序必须考虑 enabled、last_success、latency 和用户固定优先级。

只对 DNS failure、timeout、connection refused、Tailscale offline 进行下一个地址尝试；认证失败和 host key changed 不得盲目切换地址掩盖问题。

---

## 9. Herdr 集成（必须支持，但保持可选）

### 9.1 产品定位

Herdr 是远程 coding-agent runtime：后台 server 持有 terminal/workspace，客户端可以 detach/reattach；它提供 workspace、tab、pane、agent lifecycle、CLI 和 socket API。Kodework 不替代 Herdr server，也不复制 Herdr 的内部实现；Kodework 提供一个原生集成层。

### 9.2 支持级别

**M1：Herdr CLI adapter**

- 检测远端 `herdr --version`。
- 运行 `herdr status`、`herdr status server`、`herdr api schema --json` 做能力探测。
- 调用 `herdr workspace ...`、`herdr tab ...`、`herdr pane ...`、`herdr agent ...`、`herdr wait ...` 的 JSON 输出。
- 所有命令使用远端 SSH exec channel，不把 shell 文本拼接到本地 PowerShell。
- 捕获 stdout/stderr/exit code/timeout，解析失败显示原始诊断但不泄露凭据。

**M2：Herdr session/socket bridge**

- 取得 API schema 后进行 protocol version/capability negotiation。
- 优先使用 `terminal session observe` 做只读 ANSI 流。
- 需要输入/resize/scroll/control 时使用 `terminal session control` 或 schema 对应的方法。
- socket bridge 运行在远端，Kodework 只通过受保护的 SSH channel/forwarding 访问。
- 不把 Herdr socket 暴露到 Tailscale 全网或公网监听地址。

**M3：Herdr agent workflow**

- 在 Project 中显示 Herdr workspace/tab/pane 树。
- 显示 agent `idle/working/blocked/done/unknown` 状态。
- 支持创建 workspace/tab/pane、split、focus、rename、read、send text/keys、wait。
- `agent prompt --wait`、`agent wait` 等等待必须有 timeout 和 cancellation。
- 支持 Codex、Claude、OpenCode 等已探测 agent；未识别的 agent 仍作为普通 terminal 运行。

### 9.3 Herdr 与 tmux 的关系

运行时选择：

```text
Project.preferred_runtime = Herdr
    → 若远端已安装且 schema/capability 可用，使用 Herdr
    → 否则提示用户选择安装/降级到 tmux

Project.preferred_runtime = Tmux
    → 使用 tmux attach/new，不调用 Herdr

Project.preferred_runtime = PlainShell
    → 仅临时 PTY，不承诺进程持久化
```

不得自动把一个正在运行的 tmux session 转换成 Herdr session，也不得在未确认时覆盖远端 Herdr server。

### 9.4 远端安装策略

- 默认只检测，不安装。
- 用户点击“Install Herdr on remote”后显示版本、下载源、SHA-256 和目标路径。
- 下载 manifest/asset 必须校验 HTTPS、SHA-256；签名能力可用时必须校验签名。
- 安装失败不影响现有 SSH/tmux 工作区。
- Windows 远端和 Linux 远端能力必须通过 capability 表显示，不能假设 Unix PTY 行为在 Windows ConPTY 上完全相同。

### 9.5 Herdr 数据映射

| Herdr | Kodework |
|---|---|
| session | WorkspaceRuntime |
| workspace | ProjectRuntime |
| tab | SessionTab |
| pane | TerminalPane |
| agent | AgentStatus |
| CLI/socket event | Kodework domain event |
| `pane_id`/workspace id | opaque external id，原样保存 |

ID 只能使用 Herdr 返回值，不能由客户端预测。protocol mismatch、server_not_running、agent_not_ready、agent_blocked、timeout 等错误必须映射为稳定 Kodework error code。

---

## 10. SFTP 与文件/截图传输

### 10.1 TransferManager

```rust
trait TransferManager {
    async fn enqueue(&self, request: TransferRequest) -> Result<TransferId>;
    async fn pause(&self, id: TransferId) -> Result<()>;
    async fn resume(&self, id: TransferId) -> Result<()>;
    async fn cancel(&self, id: TransferId) -> Result<()>;
    async fn retry(&self, id: TransferId) -> Result<()>;
}
```

默认并发 2，最大 4。每个任务使用流式 read/write，禁止 `read_to_end()` 处理大文件。

### 10.2 上传/下载流程

上传：

```text
validate local path
→ determine size/hash if configured
→ remote dst.part
→ streaming chunks
→ flush/close
→ optional remote checksum
→ atomic rename dst.part → dst
```

下载同理，写本地 `.part`，成功后原子 rename。失败保留可诊断的 `.part` 或按用户设置清理，不能覆盖原文件。

### 10.3 图片和截图

剪贴板截图、拖入图片、PDF、日志、普通文件都统一成 `LocalAsset`，进入 TransferManager，再把远程路径作为 staged input 插入终端/Herdr pane。不要单独写第二套 `screenshot_upload.rs` 协议。

### 10.4 文件预览

第一版只读预览：文本、Markdown、JSON、图片、PDF、CSV、目录。大文件只取前 N MB 并显示“truncated”；二进制默认 hex/metadata，不直接当 UTF-8 解码。远端编辑必须先下载到临时文件、备份、保存并原子替换。

---

## 11. Action、Run、Snippet 和 Web Preview

### 11.1 三种执行模式

| 模式 | 通道 | 场景 |
|---|---|---|
| Interactive | PTY | Claude/Codex/OpenCode、vim、htop、shell |
| Quick | SSH exec | git status、test、health check，有 timeout |
| Background | tmux/Herdr | build、train、server、long-running agent |

### 11.2 Danger policy

危险命令默认只允许显式确认：`rm -rf`、`git reset --hard`、生产部署、批量删除、修改防火墙/用户/密钥等。Action 编辑界面必须显示模式、cwd、超时、危险级别和是否需要确认。

### 11.3 Run 记录

Run 记录 metadata 和 bounded output preview；大输出写分块文件/压缩日志并设置保留策略。日志不得包含密码和完整 secret prompt。

### 11.4 Web Preview

Web Preview 使用 SSH local port forwarding：

```text
remote 127.0.0.1:3000
        ↓ SSH direct-tcpip
local 127.0.0.1:<random-port>
        ↓
Tauri webview/browser preview
```

Tunnel 与 Session/Connection 绑定；连接断开进入 Suspended，重连后 Rebind。随机本地端口，禁止默认监听 `0.0.0.0`。

---

## 12. 持久化、数据库和秘密

### 12.1 SQLite

使用 SQLite + WAL；数据库只保存元数据，不保存 secret。启动时依次执行 migration、完整性检查和 schema version 检查。写操作短事务、幂等、可回滚。

核心表：

```text
hosts
addresses
projects
actions
snippets
runs
sessions
transfers
tunnels
host_keys
workspace_snapshots
settings
schema_migrations
```

每张表都要有主键、created_at、updated_at（适用时）、foreign key 和合理 index。

### 12.2 Secrets

- 密码/passphrase/token：Windows Credential Manager，SQLite 只存 `credential_ref`。
- 私钥文件：保留文件路径；需要复制到应用目录时使用 DPAPI 加密，并限制 ACL。
- 解密 secret 只在调用栈需要时存在，使用可清零 buffer，绝不 `Debug` 输出。
- 测试使用 fake in-memory secret store，不能把真实凭据写入 fixture。

### 12.3 配置迁移

旧 Electron Demo 的 localStorage/JSON 只允许作为一次性导入源，导入时逐字段校验、显示预览、成功后不再继续读取。禁止在运行时同时维护两套 source of truth。

---

## 13. Windows 生命周期、托盘和后台

### 13.1 标准行为

| 用户行为 | 必须行为 |
|---|---|
| Windows 登录 | 按设置自动启动；默认启动到托盘 |
| 双击第二次启动 | single-instance，聚焦已有窗口 |
| 点击窗口 X | 默认隐藏到托盘，不退出进程 |
| 托盘 Open | 显示已有窗口 |
| 托盘 Quit | 显示有运行任务时的确认，再真正退出 |
| UI 崩溃/重启 | 远端 tmux/Herdr 继续；下次恢复 metadata 并 reattach |
| 网络断开 | Connection → Reconnecting；任务状态可观察 |
| 更新 | 下载、签名校验、在无活动危险操作时安装 |

### 13.2 进程边界

M1 不做 Windows Service。远程持久性由 tmux/Herdr 保证，本地 Kodework 用 tray 保持生命周期。

M2 可增加可选 `kodework-agent.exe`：

```text
kodework.exe (UI)
   │ authenticated named pipe
   ▼
kodework-agent.exe (optional local worker)
   │
   ├─ long SFTP transfer
   ├─ SSH tunnel
   └─ scheduled local action
```

agent 必须使用 per-user named pipe ACL、协议版本、心跳、取消和单实例；第一版不得把它变成不受控常驻服务。

---

## 14. UI/UX 规范

### 14.1 视觉定位

定位为 Windows developer workbench，参考 VS Code、Windows Terminal、JetBrains 和 Linear 的克制层级，不做 SaaS dashboard。

禁止：

- 巨大圆角 KPI 卡片
- 满屏渐变和装饰性阴影
- 每个按钮都配彩色图标背景
- 过度留白导致终端变小
- “AI 生成式”模板化 dashboard

必须：

- 终端为视觉主角
- 边框和层级表达布局，颜色只表达状态
- connected/reconnecting/offline/blocked/done 颜色稳定
- 密度适合长时间编码
- 高对比、可键盘操作、CJK/IME 正确

### 14.2 布局

```text
┌─────────────────────────────────────────────────────────┐
│ Project · Command Palette · Connection · Window actions │
├──────┬──────────────────┬────────────────────────────────┤
│ Rail │ Host/Project     │ Terminal / split panes         │
│      │ Explorer         │                                │
│      │                  ├────────────────────────────────┤
│      │                  │ Activity / Transfer / Problems │
├──────┴──────────────────┴────────────────────────────────┤
│ SSH ●  Tailscale ●  project:path  tmux/herdr:runtime     │
└─────────────────────────────────────────────────────────┘
```

### 14.3 Terminal renderer 生命周期

- active pane：xterm + WebGL（不可用时 DOM/canvas fallback）。
- visible background：保留 model，按性能策略决定 renderer。
- suspended tab：session 保持，renderer detach/pause，scrollback 有上限。
- 不允许 20 个后台终端同时持续 WebGL redraw。

### 14.4 Keyboard Router

```text
KeyboardEvent
  → ShortcutResolver
  → app-global shortcut?
  → pane/workspace shortcut?
  → terminal focused?
  → raw bytes to PTY
```

不得在多个 React 组件中各自 `window.addEventListener('keydown')`。必须集中处理 Ctrl+B、Ctrl+Shift+P、Ctrl+Tab、split、focus、search、paste、IME 和 shell/tmux 原始输入冲突。

---

## 15. IPC 契约

### 15.1 Control plane

所有 command 使用 versioned DTO；成功返回 `{ request_id, data }`，失败返回：

```json
{
  "request_id": "...",
  "error": {
    "code": "host_key_changed",
    "message": "Remote host key changed; connection blocked.",
    "retryable": false,
    "details": {}
  }
}
```

最小 command 集：

```text
host.list/create/update/delete/test_connection
project.list/create/update/delete/open
session.attach/detach/resize/send_input/reconnect/close
terminal.search/copy/export
action.list/create/update/delete/execute
run.list/get/cancel/retry
transfer.enqueue/pause/resume/cancel/retry/list
file.list/read/write/rename/delete/mkdir
tunnel.open/close/list
tailscale.status/discover
herdr.detect/capabilities/workspaces/attach/observe/control
settings.get/update
startup.get/set
diagnostics.collect
```

### 15.2 Data plane

仅使用有序、bounded 的 Tauri Channel：

```rust
enum TerminalFrame { Bytes(Vec<u8>), Exit { code: Option<i32> }, State(SessionStatus) }
enum TransferEvent { Progress { id, done, total, speed }, State { id, status }, Error { id, code } }
enum RunEvent { Output(Vec<u8>), State(RunStatus), Exit(i32) }
```

数据帧必须有 session/run/transfer ID，前端不得把不同任务的输出混在一起。

---

## 16. 测试和质量门槛

### 16.1 Rust 单元测试

必须覆盖：

- domain validation、ID、路径、端口、危险命令判断
- Host key fingerprint 和 changed-key 拒绝
- Address candidate 排序/fallback
- 所有状态机合法/非法转换
- reconnect backoff、取消和 generation guard
- Transfer `.part`、resume、pause、cancel、retry
- Herdr JSON schema/unknown field/protocol mismatch
- SQLite migration、rollback、corrupt DB 诊断
- secret store fake implementation

### 16.2 集成测试

使用 `kodework-testkit` 提供：

- fake russh server：PTY、stdout flood、断线、认证失败、host key 变化
- fake SFTP server：大文件、慢写、断点、磁盘满
- fake `tailscale.exe`：valid JSON、unknown fields、daemon unavailable、schema drift
- fake `herdr` executable：status、schema、workspace/pane/agent、timeout、protocol mismatch
- fake Credential Manager/DPAPI provider

### 16.3 故障测试矩阵

| 故障 | 预期 |
|---|---|
| Wi-Fi 断开时持续输出 | UI 显示 Reconnecting；无死锁；tmux/Herdr 继续 |
| Tailscale 未启动 | 明确 `tailscale_unavailable`，可切换手动地址 |
| 主机 key 被替换 | 硬失败、红色警告、不自动重试 |
| 密码错误 | `authentication_failed`，不切换地址掩盖 |
| tmux session 被删除 | 显示可恢复/创建新 session 的选项 |
| Herdr server 未运行 | 提示启动或降级 tmux |
| Herdr protocol mismatch | 显示版本并禁用不兼容能力 |
| 10 GB 传输中途断网 | 保留 `.part`，可 resume，不从头读入内存 |
| 本地磁盘满 | 传输 Failed，原文件不被覆盖 |
| 远端磁盘满 | 传输 Failed，清晰显示远端错误 |
| JSON 输出包含未知字段 | 正常解析，保留诊断 |
| UI 重启 | 恢复 Project/Session metadata，重新 attach |
| 第二实例启动 | 聚焦第一个实例，不创建第二套后台连接 |
| 快速 resize 100 次 | debounce 后最终尺寸正确，无 channel 堵塞 |
| 20 个终端同时输出 | active pane 流畅，后台 renderer 不持续重绘 |
| UTF-8 字符跨 packet | 输出正确，不出现替换字符 |
| 中文/日文/韩文 IME | composition、候选框和粘贴位置正确 |
| 危险 Action | 根据 policy 弹确认，不直接执行 |

### 16.4 Frontend 测试

- DTO decode/encode 和 error mapping
- terminal renderer attach/detach/resize/search
- keyboard router 优先级
- host/project/action/run/transfer UI state reducer
- reconnect/blocked/done 状态视觉和可访问性
- 文件拖拽只产生 LocalAsset，不直接写远程路径

### 16.5 Release gate

以下任一项失败都不能称为稳定版：

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm run lint
npm run build
```

Windows release 还必须验证：MSI/installer、签名、升级回滚、托盘、开机启动、single-instance、卸载、无管理员权限安装。

---

## 17. 实施阶段与验收标准

### M0：工程重构（必须先完成）

- Cargo workspace 建好。
- `kodework-domain`、`kodework-core` 与 Tauri 完全解耦。
- 现有 Demo UI 标记为临时，不能再作为业务 source of truth。
- ADR、研究许可证矩阵、CI 骨架完成。
- 通过 fmt/clippy/test。

### M1：可长期自用的核心版本

- Host/Address/Project CRUD 和 SQLite migration
- Windows Credential Manager/DPAPI abstraction
- 真 SSH PTY、xterm.js、CJK/IME、host key
- tmux attach/new、session restore、reconnect
- Action/Run 三种模式和危险确认
- SFTP 文件列表、上传、下载、队列、取消、重试、截图
- Tailscale status/discovery/fallback
- Herdr CLI detection、workspace/pane/agent 基本查看和控制
- tray/autostart/single-instance

**M1 通过标准：** 一台没有公网 IP、安装 Tailscale 的 Linux 主机可以从 Windows 完成连接、进入 Project、attach tmux/Herdr、运行 agent、断网恢复、上传截图并打开远程项目。

### M2：高级工作区

- split pane/tab/group
- SSH config/PuTTY/WinSCP/Termius 等只读导入
- jump host chain、agent forwarding、local port forwarding
- Web Preview
- SFTP resume、目录树、远端预览和安全编辑
- Herdr socket bridge、event subscription、agent wait
- workspace snapshot 和迁移

### M3：Kodework parity+

- Voice input（Windows Speech API 或本地 provider，默认不上传音频）
- Snippets 和快捷命令组
- Yazi integration（检测远端 `yazi`，不可用时普通文件浏览）
- Workspace Controls
- 更丰富的图片 markup/preview
- 可选 `kodework-agent.exe`
- signed updater、crash diagnostics、性能仪表盘

### 明确延期

- Mosh
- RDP/VNC/FTP/SMB/K8s/S3
- 内置云同步
- 内置 AI 模型
- 全套 Docker/GPU 运维 dashboard

---

## 18. 执行顺序（禁止跳步）

1. 备份/保留现有 Demo，建立 Cargo workspace。
2. 建立 `kodework-domain` 类型和 serde DTO，先写状态机测试。
3. 建立 `kodework-storage` migration/repository，导入 Demo 配置只做一次。
4. 建立 `kodework-secrets` trait 与 Windows/fake 实现。
5. 建立 `kodework-ssh`：host key、认证、PTY、bounded I/O、cancel、generation guard。
6. 建立 `kodework-sftp`：流式 transfer、queue、progress、resume、atomic rename。
7. 建立 `kodework-network`/`kodework-tailscale`：候选地址、CLI parser、fallback policy。
8. 建立 `kodework-herdr`：detect → schema → capability → CLI → optional socket bridge。
9. 建立 `kodework-core` managers，用例测试和 fake adapters。
10. 建立 Tauri 2 thin shell、typed command、Tauri Channel、capabilities。
11. 将 frontend 改为 Host/Project/Terminal/Files/Runs/Herdr workspace UI。
12. 加入 tray/autostart/single-instance/updater。
13. 加入 fault tests、性能基线和 Windows packaging。
14. 只有所有 gate 通过后才清理旧 Electron 文件，并更新 README/CHANGELOG。

每一步完成时更新 `docs/STATUS.md`，写明：日期、commit、完成内容、测试命令、实际结果、已知限制、下一步。

---

## 19. 代码规范

- Rust 采用 `rustfmt`、`clippy -D warnings`、`thiserror`、结构化 tracing。
- 核心 crate 禁止 `unwrap/expect/panic`；边界错误必须返回类型化 error。
- 异步任务必须命名、可取消、持有 generation/token；禁止 detached 无主 task。
- 共享状态使用明确的 actor/manager 或 `Arc<RwLock>`，不允许全局可变单例。
- 前端 TypeScript 开启 strict；禁止 `any` 扩散；DTO 与 Rust schema 版本化。
- UI 组件不直接执行命令；所有副作用通过 service/query hook。
- 所有日志采用 `trace/debug/info/warn/error`，默认不记录命令参数中的 secret。
- 所有用户可见错误包含下一步建议，但不显示私钥、密码或完整远端环境变量。
- 路径必须使用类型包装（LocalPath/RemotePath），不能混用 Windows `\` 和 Unix `/`。

---

## 20. 交付物清单

执行完成后仓库必须包含：

- 可构建的 Cargo workspace
- 可运行的 Tauri Windows 应用
- `README.md` 安装、开发、故障排查说明
- `docs/ARCHITECTURE.md` 或本 Goal 的实现链接
- `docs/ADR-*.md`
- `docs/THIRD-PARTY-NOTICES.md`
- `docs/STATUS.md`
- SQLite migrations
- fake adapter/testkit
- unit/integration/fault tests
- Windows installer 和签名/更新说明
- 性能基线报告（冷启动、RSS、终端 flood、SFTP 大文件）
- 安全检查报告（secret scan、capability audit、host key tests）

---

## 21. 最终验收场景

### 场景 A：Tailscale 远端 Linux + tmux

1. Windows 登录后 Kodework 自动在托盘启动。
2. 用户选择 Host，Kodework 通过 Tailscale 地址并验证 host key。
3. 打开 Project，创建/attach tmux session。
4. 启动 Codex/Claude/OpenCode。
5. 关闭窗口，远端 agent 继续运行。
6. 断开网络 30 秒，再恢复；Kodework 进入 Reconnecting 后重新 attach。
7. 上传截图到 Project cwd，远端 agent 能读取 staged path。

### 场景 B：Herdr 远端 workspace

1. Kodework 检测远端 `herdr` 和 API schema。
2. 显示 workspace/tab/pane 和 agent 状态。
3. 用户打开指定 workspace，不创建重复 workspace。
4. `Working/Blocked/Idle/Done` 状态正确映射。
5. 用户发送 prompt 或 keys；操作有 timeout/cancel。
6. Kodework 重启后通过外部 ID 恢复，不预测或覆盖 Herdr IDs。

### 场景 C：备用地址

1. Host 同时配置 LAN、Tailscale、Public。
2. LAN unreachable 时自动尝试 Tailscale。
3. 密码错误时不切换地址掩盖认证问题。
4. Tailscale daemon 关闭时显示原因，并允许用户手动地址。

### 场景 D：大文件和后台可靠性

1. 上传 10 GB 文件，内存不随文件大小增长。
2. 73% 时断网，状态变为 Retrying/Paused，恢复后从 offset 继续。
3. 点击 X 后仍可从托盘观察进度。
4. 明确 Quit 时先取消/等待任务，再退出并保存状态。

---

## 22. 不得出现的错误实现

- 用 Electron 代替 Tauri/Rust 核心。
- 把所有业务写进 `src-tauri/src/commands.rs`。
- 前端 `localStorage` 保存 password/private key/API key。
- `check_server_key` 永远返回 true。
- 每字符 `emit` 或无界 channel。
- `read_to_end` 处理 GB/10 GB 文件。
- 关闭窗口直接杀 SSH/transfer 进程。
- 以为“开机启动”就等于“远程任务持久化”。
- 让 Kodework 自己实现 VPN。
- 未经用户确认自动安装/升级远端 Herdr。
- 把 Herdr、tmux、PlainShell 混成不可观察的“terminal mode”。
- 用布尔变量堆出连接状态，而不使用显式 enum。
- 没有 fault test 就宣称稳定或无 bug。

---

## 23. 完成定义（Definition of Done）

一个功能只有同时满足以下条件才算完成：

1. 领域模型和错误码已定义。
2. Rust core 有单元测试。
3. 有 fake adapter 的集成测试。
4. 前端只通过 typed IPC 使用它。
5. 断线、取消、超时、重复调用行为已验证。
6. 日志、secret、权限边界已审查。
7. Windows 实机或 CI runner 上完成验证。
8. README、Status、迁移/配置文档已同步。
9. 性能影响有基线或解释。
10. 不存在未处理的 `unwrap/expect`、未命名后台任务或 silently ignored error。

最终交付时必须报告：已完成、未完成、测试通过、测试未覆盖、性能实测、已知风险。禁止用“应该没问题”代替证据。

