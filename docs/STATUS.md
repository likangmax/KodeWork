# KodeWork project status

KodeWork is under active development. The repository currently targets Windows 10/11 x64. The Rust core is checked on Windows, Linux, and macOS, but native macOS and Linux desktop packages are not yet release targets.

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
- Tray, autostart, single-instance behavior, themes, and signed updater artifacts

## Verification policy

Every pull request must pass formatting, Clippy with warnings denied, the full Rust workspace tests, frontend lint/tests/build, and secret scanning. Platform or network behavior is only marked verified when it has been exercised in that environment. See [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md) for the current evidence and explicit gaps.

## Known distribution limits

- Public updater hosting is not configured yet. The client verifies updater signatures, but a distributor must provide a real endpoint.
- Public MSI builds are not Authenticode signed until a trusted signing certificate is configured in the release environment.
- Native macOS and Linux packaging, signing, and GUI validation remain future work.
- WSL availability depends on the local Windows installation and installed distributions.

Security issues should be reported using [SECURITY.md](../SECURITY.md), not a public issue.
