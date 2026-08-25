# KodeWork project status

KodeWork is under active development. The currently distributable desktop product is Windows 10/11 x64. The portable Rust core is checked on Windows, Linux, and macOS, but native macOS and Linux desktop packages are not yet released.

## Current release scope

- SSH PTY sessions, multiple terminal tabs, and split layouts
- Password, public-key, SSH Agent, and keyboard-interactive authentication
- Strict SSH host-key verification and jump hosts
- SFTP browsing and streaming transfers with pause, resume, retry, and cancel
- Clipboard text, image, and PDF handling for an active remote terminal
- Herdr and tmux discovery and attach workflows
- Embedded userspace or system-daemon Tailscale address paths
- Local PowerShell, Command Prompt, and WSL terminals through Windows ConPTY
- Projects, Actions, Runs, SSH tunnels, and loopback Web Preview
- Tray, autostart, single-instance behavior, themes, and updater signature verification support

## Reliability hardening in progress

- Background Runs remain `Running` only while their owned tmux session is observable; launcher success is not command success. Started-only evidence is `Unknown`.
- Quick Actions enforce their configured local observation deadline; a transport timeout is not proof that the remote process was terminated, so unresolved results remain `Unknown` and reconcilable.
- On startup, queued or running Quick Runs left by a terminated desktop process become `Unknown`, never an invented terminal `Interrupted`; Quick and Background Runs remain reconcilable from remote metadata.
- Run history snapshots its command and ownership so editing or deleting an Action does not rewrite or erase old records.
- Run history stores lifecycle metadata and byte counts only; stdout/stderr previews are ephemeral, and migration 11 clears previews persisted by older versions.
- SFTP resume verifies the existing partial prefix byte-for-byte before seeking; real SFTP `~` paths are expanded through the server API before identity/IO operations.
- SFTP destination leases reject concurrent writes to the same local or scoped remote target, and transfers verify source metadata again before final commit.
- Herdr bridges are SSH-channel-owned and stopped by a first-class `BridgeId`; no detached process or pattern-kill cleanup is used.
- SSH host-key trust is bound to the logical HostId across LAN, Tailscale, and public fallback addresses (schema v10), with legacy address records retained for compatibility.
- Host-key store read failures block verification instead of being treated as an unknown key; lookups do not mutate trust state.
- Reconnect attempts are native, typed, single-flight per host, and the renderer observes a native runtime Channel rather than polling lifecycle state.
- Unknown Action commands require review confirmation by default; only clearly observational commands are classified Safe.
- Interactive Actions are dispatched to the PTY and intentionally excluded from terminal Run history because the native layer cannot observe their eventual shell exit.

## Verification policy

Every pull request must pass formatting, locked Clippy with warnings denied, the full locked Rust workspace tests, frontend lint/tests/build, dependency audits, and the tracked-secret pattern gate. The repository pins Rust CI to 1.98.0 so toolchain upgrades happen deliberately. Platform or network behavior is marked verified only when it has been exercised in that environment. See [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md) for current evidence and explicit gaps.

## Known distribution limits

- The client contains updater signature verification support, but public updater hosting, a reachable manifest, and release-specific asset/signature probes are not configured as a public service yet. Do not claim automatic updates are available until those probes pass.
- Stable release workflow now hard-fails unless a trusted commercial Authenticode certificate thumbprint is configured; preview/developer builds remain the place for unsigned installers. Updater signatures and Authenticode are separate trust layers.
- Native macOS and Linux packaging, signing, and GUI validation remain future work; their core portability checks do not make them released desktop targets.
- WSL availability depends on the local Windows installation and installed distributions.

Security issues should be reported using [SECURITY.md](../SECURITY.md), not a public issue.
