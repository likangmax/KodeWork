# KodeWork project status

KodeWork is an actively developed `0.x` project. The currently released desktop target is **Windows 10/11 x64**. Selected portable Rust crates are checked on Windows, Linux, and macOS, but native macOS/Linux desktop packages are not released.

Installable distribution is determined by [GitHub Releases](https://github.com/likangmax/KodeWork/releases), not by the version field in the `main` branch. The source tree may contain unreleased version metadata and changes.

## Status vocabulary

KodeWork documentation uses these terms deliberately:

- **configured** — a code/configuration path exists;
- **tested** — a relevant automated or manual test was executed;
- **verified** — the required evidence for the stated environment/claim was observed;
- **supported** — the project is willing to treat the capability/platform as part of the supported product surface;
- **released** — an installable artifact was published through the official release channel.

These states are not interchangeable. Compilation or CI success alone does not make a desktop platform released.

## Current release scope

- SSH PTY sessions, multiple terminal tabs, and split layouts
- Password, public-key, SSH Agent, and keyboard-interactive authentication
- Strict SSH host-key verification and jump hosts
- SFTP browsing and streaming transfers with pause, resume, retry, and cancel
- Clipboard text, image, and PDF handling for an active remote terminal
- Herdr and tmux discovery and attach workflows
- Embedded-userspace or system-daemon Tailscale address paths
- Local PowerShell, Command Prompt, and WSL terminals through Windows ConPTY
- Projects, Actions, Runs, SSH tunnels, and loopback Web Preview
- Tray, autostart, single-instance behavior, themes, and updater-signature verification support
- English and Simplified Chinese application/user documentation surfaces

## Reliability and security hardening present in the current source tree

- Background Runs remain `Running` only while their owned tmux session is observable; launcher success is not command success. Started-only evidence is `Unknown`.
- Quick Actions enforce their configured local observation deadline; a transport timeout is not proof that the remote process was terminated, so unresolved results remain `Unknown` and reconcilable.
- On startup, queued/running Quick Runs left by a terminated desktop process become `Unknown`, never an invented terminal result.
- Run history snapshots command/ownership lifecycle metadata while keeping terminal output and credentials out of durable history.
- SFTP resume verifies the existing partial prefix byte-for-byte before seeking, and destination leases reject concurrent writes to the same target.
- Transfers revalidate source metadata before final commit; real SFTP `~` paths are expanded through the server API before identity/IO operations.
- Herdr bridges are SSH-channel-owned and stopped by a scoped `BridgeId`; cleanup does not rely on detached pattern-kill behavior.
- SSH host-key trust is bound to logical HostId across LAN/Tailscale/public fallback addresses, and trust-store read failures block verification.
- Reconnect attempts are native, typed, and single-flight per host; the renderer observes native runtime state rather than owning connection truth.
- Unknown Action commands require review confirmation by default; only clearly observational commands are classified Safe.
- Modal dialogs expose accessible names with a focused regression test.

## Verification baseline

The repository CI has five substantive check families:

1. frontend install/lint/tests/production build;
2. Windows Rust sidecar preparation, formatting, locked Clippy with warnings denied, and full locked workspace tests;
3. dependency/security policy including production npm audit, RustSec audit, and tracked-secret pattern rejection;
4. portable Rust checks/tests on Linux;
5. portable Rust checks/tests on macOS.

The existing required `rust` status context is an aggregate gate: it reports success only when both the Windows Rust job and the dependency/security policy job succeed. This preserves the branch-protection context while ensuring the security policy cannot be skipped by a merge that requires `rust`.

Workflow configuration is not a substitute for all repository settings. Branch protection/rulesets still determine which top-level status contexts and review rules GitHub enforces.

Platform or network behavior is marked verified only when exercised in that environment. See [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md) for native evidence expectations.

## Release and distribution boundaries

- Stable Windows release publication validates tag lineage/version consistency and expects immutable MSI, updater signature, and SHA-256 assets.
- The stable release path fails closed when the required Tauri updater signing key or trusted Authenticode certificate configuration is unavailable.
- Tauri updater signatures and Windows Authenticode are separate trust layers.
- Updater signature-verification support in the application is **not** the same as a public automatic-update service. Public updater hosting/manifest reachability must be independently verified before that capability is claimed.
- Native macOS and Linux packaging, signing/notarization, GUI validation, and release assets remain future work.
- WSL availability depends on the local Windows installation and installed distributions.

## Known follow-up work

Current tracked follow-ups include:

- sanitized real product screenshots for README presentation ([#26](https://github.com/likangmax/KodeWork/issues/26));
- repository administration/settings work that cannot be completed through ordinary source changes ([#44](https://github.com/likangmax/KodeWork/issues/44)).

Planned work remains listed here until it is completed and verified.

## Reference documents

- installable artifacts: [GitHub Releases](https://github.com/likangmax/KodeWork/releases)
- user-visible history: [CHANGELOG.md](CHANGELOG.md)
- platform/release contract: [RELEASE-MATRIX.md](RELEASE-MATRIX.md)
- Windows native evidence: [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md)
- architecture/security boundaries: [ARCHITECTURE.md](ARCHITECTURE.md) and [SECURITY.md](../SECURITY.md)
- public direction: [ROADMAP.md](../ROADMAP.md)

Security issues should be reported privately using [SECURITY.md](../SECURITY.md), not a public issue.
