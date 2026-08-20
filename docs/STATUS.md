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

- Background Runs remain `Running` until remote exit metadata is reconciled; launcher success is not command success.
- Run history snapshots its command and ownership so editing or deleting an Action does not rewrite or erase old records.
- SFTP resume verifies the existing partial prefix byte-for-byte before seeking.
- Herdr bridge shutdown is PID-owned and readiness-checked rather than pattern-killing unrelated processes.
- SSH host-key trust is bound to the logical HostId across LAN, Tailscale, and public fallback addresses (schema v10), with legacy address records retained for compatibility.
- Reconnect retry/backoff is owned by the native layer and guarded single-flight per HostId.
- Unknown Action commands require review confirmation by default; only clearly observational commands are classified Safe.

## Verification policy

Every pull request must pass formatting, Clippy with warnings denied, the full Rust workspace tests, and frontend lint/tests/build. Repository secret scanning and push protection are GitHub security settings, not a CI job; contributors must run the documented local pattern scan before staging. The current local verification includes Rust workspace tests, strict Clippy, frontend tests/lint/build, diff checks, and changed-file secret-pattern scanning on August 20, 2026. Platform or network behavior is marked verified only when it has been exercised in that environment. See [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md) for current evidence and explicit gaps.

## Known distribution limits

- The client contains updater signature verification support, but public updater hosting, a reachable manifest, and release-specific asset/signature probes are not configured as a public service yet. Do not claim automatic updates are available until those probes pass.
- Public MSI builds are not Authenticode signed until a trusted commercial certificate is configured in the release environment. Updater signatures and Authenticode are separate trust layers.
- Native macOS and Linux packaging, signing, and GUI validation remain future work; their core portability checks do not make them released desktop targets.
- WSL availability depends on the local Windows installation and installed distributions.

Security issues should be reported using [SECURITY.md](../SECURITY.md), not a public issue.
