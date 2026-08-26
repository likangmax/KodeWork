# KodeWork → Claude Code 完整交接文档（历史快照）

> **重要：本文档已归档，不代表当前 checkout。** 当前仓库已经合并 PR #8/#18，分支是 `main`，请改读 [`HANDOFF-CODEX.zh-CN.md`](HANDOFF-CODEX.zh-CN.md)。本文保留仅用于追溯 Claude Code 当时的执行上下文。

> 快照时间：2026-08-23（Asia/Shanghai）
> 工作目录：`D:\OneDrive\AAA_KK\MYCODE\redock-windows`
> 当前阶段：PR #8 合并前的 reliability/security/CI 最终收口
> 重要状态：代码已在本地完成并通过门禁，但尚未 commit、push、merge 或 release

本文档不是产品宣传稿，而是给下一位执行者的操作级交接。接手后应先完整阅读，再运行只读检查；不要根据旧 PR 评论重新推倒当前实现。

---

## 1. 当前授权边界

当前任务此前明确限定为本地工作树修改，因此在得到仓库所有者新的明确授权前：

- 不要 commit；
- 不要 push 或 force-push；
- 不要 merge PR #8；
- 不要创建、覆盖或删除 GitHub Release；
- 不要修改远端 tag、分支保护、Secrets 或 release assets；
- 不要运行 `git reset --hard`、`git checkout -- <file>`、`git clean`，也不要擅自 stash；
- 不要把现有脏工作树当作垃圾清理。

允许且推荐的动作：阅读、审查、运行测试、修复确认存在的问题、更新本交接文档，以及向用户报告证据。

如果用户之后明确要求提交或推送，仍应先完成第 13 节的发布前核对。

---

## 2. Git 与 GitHub 精确快照

### 2.1 本地 Git

```text
repository:   https://github.com/likangmax/KodeWork.git
branch:       published-main
HEAD:         08877a88de3b7c146ffe8cdba208e9c7bf1e7486
HEAD subject: harden native recovery, run reconciliation and SSH ownership
origin/main:  db4f325 (Fix Windows Release publishing)
tag v0.2.3:  3012cdf
```

`published-main` 相对 `origin/main` 的提交序列包括：

```text
08877a8 harden native recovery, run reconciliation and SSH ownership
eceb2ed harden transfer identity runs and release gates
6ca4c63 fix final merge blockers for runs transfers and conpty
9e17b48 chore: align release toolchain and security docs
0576f6a fix reconciliation timestamps and address fallback
93d661c test: make SFTP lease regression portable
5111163 reliability/security: harden runs, transfers and toolchain
a869bdd tighten run privacy and reconnect boundaries
a85dfc2 harden action trust, host keys, transfers, and reconnects
ee43dfe fix action safety and reconnect edge cases
fd4312e fix reliability and native reconnect supervision
...更早的 UI、文档和 v0.2.3 提交
```

当前实现还包含一层位于 `08877a8` 之上的未提交修改。不要只看 `git log` 就认为工作已经完成。

### 2.2 当前工作树

交接文档创建前，本地状态为 23 个 tracked 文件修改、1 个新增测试文件：

```text
 M .github/workflows/ci.yml
 M .github/workflows/release.yml
 M SECURITY.md
 M crates/kodework-core/src/session.rs
 M crates/kodework-core/tests/herdr_bridge.rs
 M crates/kodework-core/tests/session_manager.rs
 M crates/kodework-domain/src/lib.rs
 M crates/kodework-local-pty/src/lib.rs
 M crates/kodework-ssh/src/connection.rs
 M crates/kodework-ssh/src/lib.rs
 M crates/kodework-ssh/tests/fake_server.rs
 M crates/kodework-storage/src/repositories.rs
 M crates/kodework-testkit/src/fake_ssh.rs
 M docs/ARCHITECTURE.md
 M docs/STATUS.md
 M src-tauri/src/commands.rs
 M src-tauri/src/lib.rs
 M src/App.tsx
 M src/api.ts
 M src/files/FilesPanel.tsx
 M src/files/VirtualFileList.tsx
 M src/i18n.ts
 M src/runtime/RuntimePanel.tsx
?? src/i18n.test.ts
```

当时 `git diff --stat` 约为：

```text
23 files changed, 1889 insertions(+), 574 deletions(-)
```

加入本交接文件后数字会自然增加。接手时以新的 `git status --short` 和 `git diff --stat` 为准。

### 2.3 GitHub PR #8

公开 GitHub 页面在 2026-08-23 显示：

```text
PR:          #8
URL:         https://github.com/likangmax/KodeWork/pull/8
state:       Open
title:       ui/docs: polish KodeWork v0.2.3 release
base:        main
head:        published-main
remote head: 08877a8
commits:     17
files:       70
diff:        +5274 / -1075
```

PR 标题已经明显落后于实际内容。若用户授权修改 GitHub，建议标题改为：

```text
reliability/security: harden runs, transfers, SSH identity and native recovery
```

不要在未授权时自行调用 `gh pr edit`。

### 2.4 远端 CI 状态

最新远端运行是：

```text
workflow:    CI
run:         #29
run id:      32570501557
head:        08877a8
created:     2026-08-22 11:31 UTC
conclusion:  failure
failed job:  rust
failed step: cargo test --locked --workspace --all-features
exit code:   101
```

链接：

```text
https://github.com/likangmax/KodeWork/actions/runs/32570501557
```

必须理解两点：

1. 这个红灯对应远端 `08877a8`，不包含当前本地未提交修复。
2. 当前本地完整 workspace test 已经通过，包括真实 Windows ConPTY 回归测试。

因此接手者不能声称“GitHub CI 已绿”，也不能因为旧红灯就删除当前实现。正确流程是：授权后提交并推送当前工作树，让 GitHub 对新 SHA 重新跑 CI。

### 2.5 GitHub 访问环境

本次交接时：

- `gh` CLI 未登录，会提示 `gh auth login`；
- 本机 `git ls-remote` 曾因本地 `127.0.0.1` 代理不可用而无法连接 GitHub；
- GitHub 公共网页仍可读取，并确认 PR 和远端分支状态；
- 本地 `origin/published-main` 已指向 `08877a8`。

接手时不要假设 GitHub CLI 已授权。先运行：

```powershell
gh auth status
git remote -v
git status --short
```

若认证不可用，只能继续本地工作并明确报告，不能声称已经推送或发布。

---

## 3. 为什么会有这一批修改

PR #8 经历了多轮严格评审。早期 blocker 包括：

- HostKey 数据库错误被当作未知主机；
- shell substitution 绕过 Safe Action；
- SFTP 同目标并发和传输中源文件变化；
- reconnect 名义三次、实际只跑一次；
- Interactive Run 永久 Unknown；
- stale generation 的 terminal 数据混进新连接；
- Windows PowerShell ConPTY CI race；
- SFTP `~` 在 fake backend 里工作、真实协议语义不明确；
- Background Run 混合 Windows/Linux 两台机器时钟；
- Quick Action transport 丢失被假判为 Failed；
- React 仍然拥有 reconnect lifecycle；
- Herdr 远端 `socat` 采用 detached PID ownership；
- release job 权限、签名和不可变资产验证不足。

当前工作树的目标不是继续无边界扩张功能，而是把这些 correctness/security/reliability 问题闭环到适合最终 merge gate 的状态。

---

## 4. 已完成实现：Run 生命周期与对账

### 4.1 持久化状态

- 新 Run 在 dispatch 前先以 `Queued` 持久化，避免桌面进程在提交远端命令前后崩溃时完全丢失用户意图。
- `Queued -> Succeeded` 被允许，因为很快完成的 Quick 命令可能没有必要单独持久化中间 `Running`。
- Interactive Action 不创建持久化 Run。PTY shell 的真实命令边界、后续输入和 exit code 无法可靠观察，制造永久 `Unknown` 反而误导。
- 启动时遗留的 Quick `Queued/Running` 不再伪造为 `Interrupted`，而是恢复为 `Unknown` 并等待远端证据。
- storage API 已从 `interrupt_orphaned_quick_runs` 改名为 `recover_orphaned_quick_runs`。

### 4.2 Quick 的 dispatch 证据

这是最后一轮 hostile review 新发现并修复的点。

SSH 层新增 `run_command_tracked()` 和 `CommandExecutionError`，保留：

```text
请求是否可能已经 dispatch
底层 typed SshError
```

语义是：

```text
连接/开 channel/发送请求之前失败
或 server 明确返回 ChannelMsg::Failure
→ dispatched = false
→ Run = Failed

exec 请求已交给 transport，且没有明确 rejection
随后 timeout/channel/transport 丢失
→ dispatched = true
→ Run = Unknown
```

不要把 `Unknown` 改回 `Failed`。远端程序可能仍在运行或已经成功，只是客户端没有得到最终 exit status。

新增回归测试：

```text
tracked_command_distinguishes_pre_dispatch_rejection
tracked_command_marks_timeout_after_exec_ack_as_dispatched
```

注意：`dispatched` 表示“可能已到达远端且未被明确拒绝”，不是业务成功证明。

### 4.3 timeout 文案

Quick timeout 表示客户端停止等待，不表示 Linux 进程被杀死。UI/Run diagnostic 使用：

```text
等待远端结果超时；远端进程可能仍在运行。
```

### 4.4 Background

- Background 使用 tmux detached session。
- tmux launcher 返回 0 只得到 `BackgroundStarted/Running`，不能记作成功。
- remote wrapper 在 `~/.cache/kodework/runs/<run-id>/` 原子写入：
  - `started_at_s`
  - `finished_at_s`
  - `exit_code`
- exit marker 是 terminal 证据。
- tmux session 当前存在是 Background 的 live 证据。
- started marker 单独存在不是 live 证据，结果是 `Unknown`。

### 4.5 reconciliation

- 每个 Host 同时只允许一个 reconciliation，使用 per-host singleflight。
- 最多读取 100 个 reconcilable Run。
- 每 32 个 Run 合并成一个 SSH probe，而不是每个 Run 开一个 exec channel。
- 单个 batch 探测失败不会把整批业务状态伪造成失败。
- SQLite 更新使用 batch transaction。
- local storage 成功写入 terminal 结果后，远端 metadata cleanup 仅 best-effort。
- `Unknown` 不写假的 `finished_at_ms`，保持之后可继续对账。

### 4.6 跨机器时钟

不能组合：

```text
Windows local started_at + Linux remote finished_at
```

当前做法：

```text
remote_duration = remote_finished - remote_started
local_finished = local_started + remote_duration
```

如果远端时间字段不完整，则使用本地 observation time，不直接把远端 epoch 放进本地 timeline。

回归测试：

```text
reconciliation_anchors_remote_duration_to_local_start
```

### 4.7 Run output 隐私

- stdout/stderr preview 只用于当前活动结果展示。
- repository create/finish 写入 SQLite 时 preview 始终为空字符串。
- schema v11 清理旧版本已经落库的 preview。
- Action command/env/snippet 仍属于用户工作区文本，不是 secret store；文档不能声称任意 Action 配置都是秘密。

---

## 5. 已完成实现：连接状态与 native recovery

### 5.1 ConnectionStateController

- lifecycle state 和 generation 由同一个 authoritative controller 管理。
- generation 使用 `reserve_generation()` 原子递增。
- production code 不应直接写 state/generation。
- 非法 transition 返回 `StateTransitionError`，关键连接路径不再静默丢弃错误。
- stale generation event 在进入 subscriber 或 pending replay 之前被 drop。

重要允许路径包括：

```text
Connecting -> Ready
```

真实 SSH 库可能在内部完成 host-key/auth，而不向外暴露每个中间阶段。

以及：

```text
Failed -> Reconnecting
```

用于 native supervisor 对 transient failure 重新取得恢复 ownership。

### 5.2 typed ConnectError

当前错误类别：

```text
Network
Timeout
Tailscale
Authentication
CredentialRequired
HostKey
InvalidConfiguration
Cancelled
Protocol
Internal
```

重试 policy 只看 type，不解析本地化错误字符串。

### 5.3 DNS fallback

- SSH direct connect 显式执行 hostname resolution。
- 新增 `SshError::NameResolution`。
- NameResolution、Timeout、ConnectionRefused、Unreachable 才允许尝试同一 Host 的下一个 address candidate。
- HostKeyChanged、authentication、configuration、cancelled、protocol 不得被 address fallback 隐藏。

回归测试：

```text
candidate_dns_failure_tries_next_address
```

### 5.4 CredentialRequired

- encrypted private key 没有 passphrase 时返回 typed `CredentialRequired`。
- keyboard-interactive 不允许 hidden unattended reconnect。
- 前端使用 `asConnectError()`，不再通过 `contains("encrypted")`、`passphrase` regex 决策。

### 5.5 native reconnect supervisor

- React 的 3 秒 connection lifecycle polling 已删除。
- renderer 通过 Tauri Channel 订阅 `ConnectionRuntimeSnapshot`。
- Channel 每 5 秒发送低频 heartbeat，使 native sender 能发现 renderer 已注销并退出。
- reconnect retry 是 per-host singleflight。
- backoff 为：

```text
1.2s, 2.4s, 4.8s, 9.6s, 19.2s（上限）
```

- transient failure 后保持 `Reconnecting` ownership，下一次 backoff 到期继续尝试。
- fatal/auth/credential failure 不进入自动重试循环。
- OS resume、window focus、tray open、second-instance focus 会递增 `reconnect_wake_epoch`，清除等待中的 backoff 并尽快重试。
- 旧的 public `reconnect_host` Tauri command/API 已删除，避免两套 policy。

当前实现仍是 native 全局 750ms scan + per-host schedule/singleflight，不是真正独立的 per-host actor/`Notify`。文档不得声称完整 actor 架构已经实现。

---

## 6. 已完成实现：HostKey 与信任迁移

- trust database/SQLite lock/query 错误向上传播为 `HostKeyStoreUnavailable`，必须 fail closed。
- “没有历史 key”可以询问用户；“无法读取历史 key”必须阻止连接。
- Host identity 按逻辑 `HostId` 跨 LAN/Tailscale/public address 共享。
- legacy `hostname:port` key exact match 后自动 promote 到 HostId scope，无需再次弹窗。
- host key schema 已支持 `(host_id, algorithm)`，同一 Host 可以保存多算法 key。
- 同算法换 key 仍是 hard failure。

关键测试：

```text
known_hosts_store_error_blocks_without_prompt
matching_legacy_key_is_promoted_to_host_scope
host_scoped_identity_keeps_algorithms_separate
candidate_dns_failure_tries_next_address
```

---

## 7. 已完成实现：SFTP 与传输完整性

### 7.1 `~` 路径

- SFTP 不是 shell，不能把 `~/foo` 原样传给 server 并假设会展开。
- 当前真实 `RusshSftpBackend` 建立 session 后通过 server `expand_path("~")` 得到 canonical home。
- `~` 和 `~/...` 在 backend I/O/identity 前统一转换。
- 测试 `real_backend_expands_tilde_paths_before_sftp_requests` 验证真实 adapter 的 SFTP 请求不携带裸 `~`。

必须诚实区分：这个测试覆盖真实 adapter + 模拟 SFTP transport，不等于已经在一台外部 OpenSSH server 上完成端到端验收。真实 Linux/OpenSSH integration 仍建议在 merge 前或后续专门环境执行。

### 7.2 destination lease

- 同一 local/remote destination 同时只有一个 transfer owner。
- `~` 和 expanded home alias 会归一到同一个 remote lease identity。
- collision 返回 `DestinationBusy`。
- local lease 的大小写处理按 OS 语义区分，不能在 Linux 上无条件 lowercase。

### 7.3 source mutation

Upload 在开始与 final rename 前比较：

```text
file handle metadata
path metadata
length
modified time
transferred == total
```

变化时返回 `SourceChanged`，不提交 final rename。

Download 在替换本地目标前重新 stat remote source；变化时保留用户原文件。

### 7.4 resume

- `.part` 长度相同不代表内容一致。
- 当前使用安全的 byte-by-byte prefix verification。
- 不一致时丢弃错误 partial 并安全重建。
- hash-based resume 尚未实现；不要把它写成已完成。

### 7.5 retry

当前已避免对 `SourceChanged`、`DestinationBusy`、`DiskFull`、`SourceNotFound`、`Cancelled` 自动重试。

SFTP backend 错误类型仍可进一步细分为 PermissionDenied、ConnectionLost、SessionClosed、Unsupported、RemoteIo；这是后续债务，不应继续无边界塞进当前 PR。

---

## 8. 已完成实现：Herdr bridge ownership

旧模型：

```text
nohup setsid socat ... &
记录 remote PID
后续 kill PID
```

当前模型：

```text
BridgeId
+ SSH-owned exec socat
+ local tunnel
+ in-memory ActiveBridge registry
```

关键行为：

- 不使用 `nohup`、`setsid`、`pkill -f`。
- `socat` 生命周期属于 SSH exec channel。
- tunnel 建立失败会关闭 socat owner。
- 只通过 `BridgeId` stop，旧 `remote_pid` API/字段完全删除。
- remote port 使用 16 个候选端口的 bounded collision range。
- `SshConnection::exec_owned()` 的输出不会污染 terminal event stream。
- `SshExec::ensure_running(250ms)` 捕获启动即退出及 bounded stderr diagnostic。
- 普通 `exec()` 保留原广播行为，避免破坏既有 terminal tests。

回归测试：

```text
bridge_rejects_socat_that_exits_during_startup
bridge_without_socket_reports_clear_error
bridge_probes_socket_and_starts_socat
```

---

## 9. 已完成实现：Windows ConPTY CI 稳定性

旧 PowerShell 测试：

```text
固定 sleep 750ms
提前写 ESC[1;1R
猜测 PowerShell/PSReadLine ready
```

这在 busy GitHub runner 上存在 race。

当前测试：

- 使用 sentinel 驱动 readiness；
- 持续读取输出；
- 收到 `ESC[6n` DSR 查询后才回复 `ESC[1;1R`；
- 不依赖 prompt 形状；
- 不依赖固定 startup sleep；
- 保留真实 PowerShell ConPTY round trip。

本机该测试曾连续运行 5 次，5/5 通过；之后也多次随完整 workspace test 通过。

远端 CI #29 仍是旧 SHA 的红灯，必须推送新 SHA 后才能验证 GitHub runner 是否真正恢复。

---

## 10. 已完成实现：CI、release 与供应链

### 10.1 CI

`.github/workflows/ci.yml` 当前变化：

- Rust 使用 `--locked`；
- GitHub Actions 引用 pin 到 commit SHA；
- 新增 dependency-security job；
- `npm ci --ignore-scripts` 后审计 production dependencies；
- 安装固定 `cargo-audit 0.21.2 --locked`；
- 正确执行 `cargo audit`，不能写不存在语义的 `cargo audit --locked`；
- 新增 tracked secret pattern gate；
- private key regex 已修正。

### 10.2 Release least privilege

`.github/workflows/release.yml`：

- build job：`contents: read`；
- publish job：`contents: write`；
- publish job 不运行 `npm ci` 或第三方 build graph；
- Actions 全部 SHA pin；
- Rust toolchain pin 到 1.98.0。

### 10.3 Stable signing

stable release 必须同时具备：

```text
TAURI_SIGNING_PRIVATE_KEY
KODEWORK_CERT_THUMBPRINT
```

缺少 updater signing key 或 Authenticode certificate thumbprint 都 hard fail。

可选：

```text
TAURI_SIGNING_PRIVATE_KEY_PASSWORD
KODEWORK_TIMESTAMP_URL
```

unsigned preview/developer build 应使用独立 workflow，不能让 stable channel 出现两种 trust level。

临时 updater key/pass 文件在 build 后通过 `if: always()` 清理。

### 10.4 Release lineage/version gate

workflow 验证：

```powershell
git merge-base --is-ancestor HEAD origin/main
```

含义是 tag commit 必须已经包含在 `main` 中。不要把参数方向反过来。

同时强制：

```text
tag vX.Y.Z
package.json version
src-tauri/tauri.conf.json version
所有本地 Cargo workspace package version
```

完全一致。

当前 feature branch HEAD 不在 `origin/main`，本地 lineage check 返回 1 是预期行为；此时不应 release。

### 10.5 Release assets

workflow 强制恰好一个 MSI，并要求：

```text
<name>.msi
<name>.msi.sig
<name>.msi.sha256
```

已有同 tag release 时逐个下载并进行 byte-identical SHA-256 比较：

```text
完全相同 → no-op
缺失或不同 → fail，不覆盖 immutable release
```

后续可继续增加 SBOM、provenance/attestation 和严格“禁止额外未知资产”的 manifest gate；当前没有实现，不要声称已有。

---

## 11. 前端与 i18n 状态

已完成：

- `translationCatalogs` 导出；
- `src/i18n.test.ts` 检查 zh/en key parity；
- 英文 credential/connect error 不含中文；
- FilesPanel、VirtualFileList、RuntimePanel 的主要 SFTP/Herdr/tmux 标签接入 Translator；
- structured ConnectError 在前端本地化；
- React reconnect polling 已移除，改为 native runtime subscription；
- Herdr bridge UI 只保存/调用 `BridgeId`。

未完成：

- `src/App.tsx` 仍有大量中文 literal；
- terminal、dialogs、HostEditor、host-key/action/run/settings banner 仍有未迁移文案；
- agent status labels 中仍可能出现英文内部状态；
- 不能声称 English 主流程已完全无中文。

前端 i18n 的后续修改规则：新增 key 必须同时更新 zh-CN/en-US，并保持 `src/i18n.test.ts` 通过。

---

## 12. 已验证证据

### 12.1 工具版本

本机最后记录：

```text
rustc 1.98.0
cargo 1.98.0
node v24.15.0
npm 11.7.0
git 2.53.0.windows.3
```

CI 使用 Node 22 和 Rust 1.98.0。Node 本地版本比 CI 新，因此最终仍需 GitHub CI 验证 Node 22 环境。

### 12.2 最终通过的门禁

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
npm run lint
npm run test:frontend
npm run build
npm audit --registry=https://registry.npmjs.org --omit=dev --audit-level=high
cargo audit --no-fetch
git diff --check
```

结果：

- 完整 Rust workspace test：PASS；
- strict Clippy：PASS；
- Rust fmt：PASS；
- frontend lint：PASS；
- Vitest：2 files / 7 tests PASS；
- production build：PASS；
- npm audit：0 vulnerabilities；
- cargo audit：0 vulnerabilities，17 条 allowed warnings；
- workflow YAML 使用 PyYAML 解析：PASS；
- tracked secret pattern scan：PASS；
- `git diff --check`：无 whitespace error；仅 Windows `core.autocrlf` 的 LF→CRLF warning。

Cargo audit 的 17 条 warning 主要来自 GTK3 bindings、`proc-macro-error`、`unic-*` 的 unmaintained 项，以及 `glib 0.18.5` 的 allowed unsound warning。它们不是本次发现的新 vulnerability，但仍属于依赖升级债务。

### 12.3 hostile pattern scan

最后扫描确认未发现：

```text
remote_pid
公开 reconnect_host command/API
pkill
nohup
setsid
credential/passphrase 字符串 retry policy
静默 let _ = transition(...)
```

内部函数名 `reconnect_host_inner` 可以存在；它不是 renderer 可调用的第二套 reconnect command。

### 12.4 不能声称已经验证的事项

- 当前 dirty tree 尚未在 GitHub Actions 上运行；
- 未在外部真实 OpenSSH server 上完成完整 SFTP `~` upload/download acceptance；
- 未做真实断网→恢复、Windows sleep/resume、tray 隐藏时长时间 recovery 真机验收；
- 未生成或发布 stable MSI；
- 未使用真实 Authenticode/updater secrets 验证 release workflow；
- 未完成完整英文 UI walkthrough；
- 未完成 macOS/Linux GUI packaging；portable Rust check 不等于桌面 release。

---

## 13. Claude Code 推荐接手顺序

### 阶段 A：只读确认

```powershell
Set-Location 'D:\OneDrive\AAA_KK\MYCODE\redock-windows'
git status --short
git branch --show-current
git rev-parse HEAD
git remote -v
git diff --stat
git diff --check
```

确认：

- branch 是 `published-main`；
- HEAD 是 `08877a8...`；
- 大量本地修改仍在；
- 没有陌生 binary、secret、database、log 或 build artifact 被纳入 diff。

### 阶段 B：理解实现

优先阅读：

```text
CLAUDE.md
docs/HANDOFF-CLAUDE-CODE.zh-CN.md
docs/ARCHITECTURE.md
docs/STATUS.md
crates/kodework-core/src/session.rs
crates/kodework-ssh/src/connection.rs
src-tauri/src/commands.rs
.github/workflows/ci.yml
.github/workflows/release.yml
```

然后按功能阅读对应测试，不要只读 production code。

### 阶段 C：重新运行门禁

至少运行：

```powershell
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
npm run lint
npm run test:frontend
npm run build
cargo audit --no-fetch
npm audit --registry=https://registry.npmjs.org --omit=dev --audit-level=high
git diff --check
```

如果 `cargo audit` 联网 fetch 因 GitHub advisory DB 网络失败，可先记录网络错误，再用已有缓存 `cargo audit --no-fetch`；不能把网络失败描述成依赖漏洞。

### 阶段 D：最终代码审查

重点核对：

1. `run_command_tracked` 的 explicit rejection 会把 `dispatched` 复位为 false。
2. Quick/Background 只有 `was_dispatched()` 的错误才进入 `Unknown`。
3. `Unknown` 没有假的 `finished_at_ms`。
4. remote duration 只用于计算 local timeline duration。
5. stale generation 在 subscriber/replay 前被 drop。
6. reconnect 只有一套 native policy。
7. bridge stop 只使用 `BridgeId`。
8. SFTP `~` 在 backend 请求前 canonicalize。
9. database lock、registry lock 不跨无关网络 await。
10. release lineage 的 ancestor 方向没有反转。
11. stable signing 缺失时 hard fail。
12. CI/Release Actions 仍然 SHA pinned。

### 阶段 E：只有得到用户明确授权后才提交/推送

建议先展示：

```powershell
git status --short
git diff --stat
git diff --name-only
```

建议 commit subject：

```text
reliability/security: finish native recovery and run integrity hardening
```

提交前不要盲目 `git add .`。逐项确认文件确实属于本次工作，再 stage。禁止 force push。

授权后可能的流程：

```powershell
git add <reviewed file list>
git commit -m "reliability/security: finish native recovery and run integrity hardening"
git push origin published-main
```

然后：

```powershell
gh pr checks 8 --watch
```

如果 `gh` 未登录，应停止并让用户完成认证，不要寻找或读取本机 token。

### 阶段 F：GitHub final merge gate

新 SHA 的所有 checks 必须绿：

```text
frontend
rust (Windows)
dependency-security
portable Rust (Linux)
portable Rust (macOS)
```

尤其确认 Windows `powershell_round_trip_uses_real_conpty` 不再依赖 rerun 碰运气。

CI 全绿后再做一次：

- PR title/description 与实际范围一致；
- Files changed 无 secret/build output；
- GitHub diff 与本地 commit 一致；
- 无未解决的 merge blocker；
- 用户明确批准 merge。

没有明确批准时，不要 merge。

---

## 14. 后续 PR 建议，不要继续膨胀 PR #8

### PR #9：Connection Supervisor v2

- 真正 per-host actor/`Notify`，替换 750ms global scan；
- `CancellationToken`；
- network-change/wake source integration；
- structured connection telemetry；
- 更明确的 state subscription lifecycle。

### PR #10：Transfer v2

- 外部真实 OpenSSH integration environment；
- hash-based resume；
- typed SFTP permission/session/connection errors；
- filesystem identity-aware leases；
- crash-safe Windows replacement；
- large transfer performance benchmark。

### PR #11：UI/i18n 与模块拆分

- 完整 zh/en literal migration；
- App.tsx、commands.rs、session.rs 拆分；
- Host editor、credential、host-key、run、settings dialogs；
- English end-to-end UI acceptance。

### PR #12：Release hardening

- SBOM；
- provenance/attestation；
- exact release manifest，拒绝额外未知 assets；
- dependency warning reduction；
- stable updater endpoint/reachable manifest probes。

### 独立安全任务

- clipboard private cache + TTL；
- remote run metadata GC；
- long-lived resource registry observability；
- real crash/restart recovery matrix。

---

## 15. 文件职责地图

```text
crates/kodework-domain
  稳定模型、状态机、验证、Action danger classification

crates/kodework-core/src/session.rs
  SessionManager、generation、Run orchestration、reconciliation、Herdr bridge

crates/kodework-ssh/src/connection.rs
  russh connection、PTY、exec、tracked dispatch、owned exec、SFTP channel

crates/kodework-storage/src/repositories.rs
  Run/Host/Project/Action 持久化、batch reconciliation、startup recovery

crates/kodework-local-pty/src/lib.rs
  Windows ConPTY、PowerShell/CMD/WSL 本机终端

crates/kodework-testkit/src/fake_ssh.rs
  fake SSH fault model、persistent owned exec simulation

src-tauri/src/commands.rs
  IPC translation、native reconnect supervisor、Run persistence coordination

src-tauri/src/lib.rs
  AppState、startup recovery、Tauri lifecycle/wake wiring、command registration

src/App.tsx
  renderer orchestration；仍偏大，避免继续塞入业务 policy

src/api.ts
  typed Tauri invoke/channel boundary

src/i18n.ts / src/i18n.test.ts
  中英文 catalog 与 parity regression

.github/workflows/ci.yml
  Windows/front-end/portable/security gates

.github/workflows/release.yml
  Windows stable MSI build、签名、immutable publish
```

---

## 16. 不可破坏的安全与正确性约束

1. HostKey store error 必须 fail closed。
2. HostKeyChanged/auth failure 不允许通过 address fallback 隐藏。
3. Safe Action 只是 UX guardrail，不是 sandbox；shell composition 至少进入 Review。
4. renderer 输入的 Action 内容不可信，database stored Action 才是执行来源。
5. credential bytes 不进入 SQLite、renderer、日志或 reconnect supervisor。
6. terminal output 不进入 durable Run history。
7. timeout 不等于 remote process killed。
8. transport lost 不等于 command failed。
9. Unknown 不能伪造 finished time。
10. remote clock 和 local clock 不能直接混用。
11. stale generation data 不能进入 subscriber/replay。
12. 同 destination transfer 不能并发写。
13. source changed 时不能 final commit。
14. `.part` length 不能作为 resume 完整性证明。
15. SFTP `~` 必须显式 expand/canonicalize。
16. remote bridge 必须由资源 handle/BridgeId ownership 管理，不能 pattern-kill。
17. stable release 缺签名必须失败。
18. build dependency graph 不应持有 `contents: write`。
19. release 同 tag asset 不允许静默覆盖。
20. 未真实验证的平台/网络/签名场景必须明确写 `not tested`。

---

## 17. 最终交接结论

当前本地代码已经从“修明显 bug”进入了较成熟的系统语义阶段：

- Run 能表达未知结果而不是假失败；
- reconnect ownership 已从 React 移到 native typed supervisor；
- HostKey、generation、SFTP、Herdr 都有明确 ownership/integrity boundary；
- release pipeline 已具备更合理的权限和签名失败策略；
- 本地完整 Rust/前端/审计门禁均通过。

但是当前还不能宣称 PR #8 已经可以直接 merge，唯一可靠的下一道外部门禁是：

```text
在用户授权后提交并 push 当前 dirty tree
→ 让 GitHub 对新 SHA 重跑全部 checks
→ 全绿
→ 最终 diff/PR title/merge approval
```

在此之前，最准确的状态是：

> 本地实现和测试已完成；GitHub 仍停留在旧 SHA 的红色 CI；尚未提交、推送、合并或发布。
