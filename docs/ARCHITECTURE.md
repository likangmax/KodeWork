# KodeWork architecture

> The implementation baseline for the current `0.x` Windows release. This document describes boundaries that are enforced by the workspace and tested by CI; it is not a future-state mock-up.

## System view

```mermaid
flowchart TB
    subgraph Desktop[Desktop process]
        UI[React + TypeScript
workspace UI / xterm.js]
        Shell[Tauri 2 shell
window / tray / single instance / updater]
        IPC[Typed commands + bounded Channels]
        Core[kodework-core
session / tunnel / run / transfer orchestration]
        Domain[kodework-domain
models / validation / danger policy]
        UI --> IPC --> Core
        Shell --> IPC
        Core --> Domain
    end

    subgraph Adapters[Platform-independent Rust adapters]
        SSH[kodework-ssh
russh / PTY / host keys / jump hosts]
        SFTP[kodework-sftp
streaming / resume / retry / cancel]
        Net[kodework-network
candidate ranking / fallback]
        TS[kodework-tailscale
system + embedded userspace adapter]
        Herdr[kodework-herdr
CLI + socket bridge]
        Store[kodework-storage
SQLite migrations / repositories]
        Secret[kodework-secrets*
opaque refs / native secure storage]
        Local[kodework-local-pty
PowerShell / CMD / WSL]
    end

    Core --> SSH
    Core --> SFTP
    Core --> Net
    Core --> TS
    Core --> Herdr
    Core --> Store
    Core --> Secret
    Core --> Local

    SSH --> Remote[Private Linux host
SSH / SFTP / Herdr / tmux]
    SFTP --> Remote
    TS --> Path[Tailscale path]
    Net --> Path
    Path --> Remote
```

## Runtime boundaries

| Boundary | Owns | Must not own |
| --- | --- | --- |
| `src/` | view state, layout, keyboard routing, renderer lifecycle | credentials, SSH sockets, filesystem secrets, connection truth |
| `src-tauri/` | command/DTO translation, native reconnect supervision, desktop plugins, tray and window lifecycle | SSH/SFTP implementation, renderer-owned lifecycle policy |
| `kodework-core` | connection generations, session lifecycle, actions/runs, tunnels, transfer orchestration | Tauri types or React state |
| `kodework-domain` | stable models, validation, explicit state machines, danger classification | I/O, platform APIs, persistence |
| adapter crates | one integration boundary each (SSH, SFTP, Tailscale, Herdr, local PTY, storage, secrets) | cross-cutting UI policy |

The dependency direction is deliberately one-way:

```text
UI -> Tauri shell -> core -> domain
                         -> adapters
```

The core and domain crates are Tauri-independent so the same connection and transfer logic can be reused by a future macOS/Linux shell or a background agent.

## Data planes

### Control plane

Low-frequency typed commands create and control resources: connect, disconnect, resize, run an Action, list a directory, enqueue a transfer, and update a pinned folder. Commands return typed results or typed errors.

### Data plane

High-frequency streams never use one IPC call per byte. Rust aggregates terminal output into bounded frames and sends ordered `TerminalFrame` values through a Tauri Channel. Transfer and Run output use the same pattern, with cancellation and backpressure. The native event pump checks the connection generation before subscriber delivery or replay, so output from an obsolete connection cannot overwrite a newer one.

```text
keyboard input -> bounded input queue -> SSH/local PTY
SSH/local PTY output -> 8–16 ms or 4–32 KiB frame -> xterm.js
file bytes -> streaming SFTP -> .part -> atomic rename -> completion event
```

## Durable remote work

KodeWork is a client, not the process supervisor for the remote coding task. Herdr or tmux runs on the Linux host. A Windows restart, tray exit, sleep cycle, or network flap therefore leaves the remote job alive; the next connection re-attaches to the same durable session. Local PTY sessions are intentionally separate and are terminated with the desktop process.

Quick and Background Actions are persisted separately from their mutable Action
definition. Each Run stores the host, project, command, mode, and working-folder
snapshot captured at launch. A detached Background Run starts as `Running`; the
tmux launcher returning zero is not treated as command success. The remote
wrapper atomically writes an exit-code marker under
`~/.cache/kodework/runs/<run-id>/`, and batched `run_reconcile` compares that
marker and the owned tmux session after reconnect. For Quick, a started marker
without an exit marker is `Unknown`; for Background, only a currently owned
tmux session proves `Running`. If neither source is authoritative, the UI shows
`Unknown` instead of guessing failure or success. Historical Run rows retain
their snapshots when mutable Actions or Projects are deleted.

Quick Actions apply their configured timeout to the observed SSH command. A
local observer timeout records an unresolved/`Unknown` result unless remote
termination is proven; it does not claim that the remote process was killed.
Interactive and Background
Actions have no local command deadline: Interactive is dispatched to the PTY,
while Background execution is owned by the remote tmux wrapper. During startup,
queued or running Quick Runs left by a terminated desktop process are marked
`Unknown` and remain reconcilable; detached Background Runs are also left
available for reconciliation.

Run history stores lifecycle metadata, command snapshots, and byte counts only.
Stdout/stderr previews are kept in memory for the active result view and are
never persisted to SQLite; migration 11 also clears previews written by older
versions.

## Address and authentication flow

1. Resolve a host into ordered candidates (manual, LAN, Tailscale, public, or jump-host route).
2. Try candidates with bounded timeouts and typed failure classification.
3. Verify the SSH host-key fingerprint before accepting the session. Trust is bound to the logical `HostId`, so LAN, Tailscale, and public fallback paths for one workstation must present the same identity; legacy address-scoped records remain a compatibility fallback. A changed key is a hard failure.
4. Authenticate with password, private key, Windows SSH Agent/Pageant, or keyboard-interactive prompts.
5. Create independent PTY, exec, SFTP, and tunnel channels under one connection generation.
6. The `ConnectionStateController` is the only production state writer. Event
   pumps reject stale transport generations before forwarding data or adding it
   to the bounded pane replay buffer, so reconnects cannot mix old output into
   the new session.

Tailscale supplies a network path or address discovery. It does not replace SSH authentication or host-key verification.

## Security boundary

- SQLite stores host/project metadata and opaque credential references, never
  managed credentials or persisted command output. User-authored Actions,
  environment values, and snippets are workspace text, so users must not use
  those fields as a credential store.
- Windows credentials use native protected storage; private-key material is handled by the platform adapter.
- Auth keys are short-lived inputs and are never written to README files, fixtures, logs, or ordinary renderer persistence.
- Remote paths are validated before clipboard-asset upload; uploads are staged and atomically renamed.
- Dangerous Actions are reclassified in Rust. Clearly observational commands
  are Safe, known destructive commands are Dangerous, and unknown shell
  constructs are Review; a UI flag cannot downgrade the server decision.
- Herdr socket bridges are owned by an SSH exec channel and identified locally
  by a `BridgeId`; stopping one closes only its tunnel and channel. They never
  use detached processes or broad `pkill -f` matching.
- Host-key identities were introduced per logical Host in schema v10 and are
  stored per algorithm since schema v12, preventing an address fallback from
  silently becoming a different trusted workstation. Schema v11 removes
  persisted run output and schema v13 stores jump-host authentication
  independently.
- Production CSP is restrictive; loopback frames are allowed only for an explicit SSH Web Preview tunnel.

See the numbered decisions in [`adr/`](adr/) for the rationale behind these boundaries.

## Performance invariants

- Bounded queues and bounded terminal replay buffers.
- No `read_to_end` for large transfers; a 512 MiB fixture is covered by the test suite.
- Resume never trusts `.part` length alone: the existing prefix is compared
  byte-for-byte with the current source, and a mismatch restarts safely. The
  real SFTP backend canonicalizes `~` through the server's expand-path API;
  hash-only resume is deferred until the SSH/SFTP boundary can provide a
  trusted remote digest capability.
- At most 20 local/remote terminal sessions per workspace, with inactive renderers detached or paused.
- Fixed-row virtual windows for large remote directories.
- Single-flight reconnect, host-key, and keyboard-interactive polling.
- Reconnect attempts are single-flight per logical `HostId`, use typed failure
  kinds, and are launched by the native process supervisor. The renderer
  subscribes to native runtime snapshots and does not own retry counters or
  backoff decisions.
- Generation guards on reconnect and explicit cleanup of dead subscribers.

## Verification

The repository's release gates are listed in [`STATUS.md`](STATUS.md) and the test evidence is maintained in [`TEST-MATRIX-WINDOWS.md`](TEST-MATRIX-WINDOWS.md). A green compile is not treated as proof of real Tailscale, Herdr, sleep/wake, or installer behavior; those environments are labeled separately.
