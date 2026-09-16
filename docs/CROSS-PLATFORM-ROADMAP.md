# KodeWork cross-platform roadmap

> This document describes the technical path toward additional native desktop platforms. It is not a release schedule. For current product availability, use [`STATUS.md`](STATUS.md) and [`RELEASE-MATRIX.md`](RELEASE-MATRIX.md). For project-wide priorities, use [`../ROADMAP.md`](../ROADMAP.md).

## Current evidence

KodeWork currently distributes a **Windows 10/11 x64 MSI**. That is the only native desktop release the project claims today.

The repository also checks a selected portable Rust crate graph on Linux and macOS in CI. Those jobs are useful portability evidence for shared domain, storage, network, SSH/SFTP, Tailscale, Herdr, and related core code, but they do **not** prove that a native macOS or Linux desktop application is packaged, installable, signed, or usable.

No iOS or Android client is currently released or promised on a schedule.

## What “supported” means

A platform is not considered supported because the Rust graph compiles or because the application framework can target that operating system. Before KodeWork claims a new native desktop platform, the target must have evidence for all relevant layers:

1. **Portable core** — the shared Rust graph builds and its relevant tests pass on the target architecture.
2. **Credential storage** — secrets use an appropriate OS-backed secure store with reboot/relaunch and failure-path tests; renderer persistence is not used as a credential vault.
3. **Local terminal** — a native PTY implementation supports shell startup, resize, I/O, close/exit behavior, and resource limits without weakening current safety boundaries.
4. **Desktop shell** — the native Tauri application launches correctly and platform-specific lifecycle, tray/menu, clipboard, dialogs, notifications, and keyboard behavior are exercised where applicable.
5. **Remote-development workflows** — SSH host-key policy, authentication modes, SFTP, tmux/Herdr, Tailscale paths, jump hosts, and reconnect behavior are verified on the native client rather than inferred from shared-core tests.
6. **Packaging** — install, launch, upgrade, and uninstall smoke tests pass for the actual package format distributed to users.
7. **Release trust** — signing, notarization, checksums, updater metadata, and other trust layers required for the chosen distribution path are configured and verified separately.
8. **Documentation** — the user guide, troubleshooting guidance, status page, and release matrix describe the platform without relying on Windows-only instructions or unverified claims.

Until those gates are satisfied, documentation should say `portable core checked`, `under development`, or `not released` rather than `supported` or `production-ready`.

## macOS path

The portable Rust checks are groundwork, not a desktop beta. Native macOS work should proceed in small, reviewable layers:

### Native adapters

- provide an OS-backed credential-store implementation appropriate for macOS;
- implement or adopt a Unix PTY path behind the existing local-terminal boundary;
- audit filesystem, shell, clipboard, process-launch, tray/menu, and path assumptions that currently depend on Windows behavior;
- keep SSH host-key verification and credential policy fail-closed while adapting platform APIs.

### Native acceptance

Before publishing a macOS artifact, verify at minimum:

- clean build on the target architecture;
- application bundle creation and launch on a clean test machine;
- credential persistence/retrieval through the native secure store;
- local shell startup, resize, Unicode/CJK input, clipboard behavior, and clean close/exit;
- real SSH/SFTP plus the supported private-network/jump-host paths;
- sleep/resume and network-change recovery;
- install/update/uninstall behavior for the selected distribution route;
- code signing and notarization requirements for that route.

The project should decide Intel support separately from Apple Silicon support based on test capacity and user demand; CI architecture coverage must not be silently generalized to untested hardware.

## Linux path

Linux support has a wider environment matrix, so the first native target should be deliberately narrow instead of claiming every distribution or desktop environment.

### Native adapters

- use an OS-appropriate secure credential backend with an explicit failure policy when a secure store is unavailable;
- share a Unix PTY implementation where the platform behavior is genuinely compatible, while retaining target-specific tests;
- audit desktop integration across the chosen display/session environment rather than assuming Windows tray/lifecycle semantics;
- keep packaging choices limited to formats the project can build, install, upgrade, uninstall, and support reproducibly.

### Native acceptance

For every Linux distribution/package combination that is advertised, collect evidence for:

- package installation, launch, upgrade, and removal;
- secure credential storage in the documented desktop/session environment;
- local terminal behavior and common shell startup;
- SSH/SFTP, Tailscale/jump-host, Herdr/tmux, clipboard/file, and reconnect workflows;
- Wayland/X11 or desktop-environment behavior only where those combinations were actually tested;
- bundled sidecars, runtime libraries, desktop entries, tray integration, and permissions required by the package.

A successful CI build on Ubuntu does not imply support for Fedora, Arch, Debian, GNOME, KDE, Wayland, X11, Flatpak, AppImage, RPM, or any other environment that has not passed its own acceptance path.

## Mobile and web

iOS, Android, ChromeOS-specific, and browser clients are **exploratory topics, not committed roadmap items**. The project does not currently commit to React Native, Flutter, a native rewrite, Tauri mobile, WebAssembly, cloud credential sync, or any delivery date for those targets.

If mobile or web demand becomes material, architecture selection should begin with a written threat model and a small proof of concept covering the hard constraints first:

- SSH transport and host-key verification;
- platform credential storage;
- terminal rendering/input and background/resume limits;
- filesystem and SFTP interaction within the platform sandbox;
- lifecycle/reconnect behavior;
- whether enough of the existing Rust core can be reused without duplicating security-sensitive business logic.

Only after those constraints are measured should the project choose a client framework or publish a delivery plan.

## Sequencing

There are no committed platform dates. A responsible sequence is:

1. keep the portable Rust graph green on target operating systems;
2. isolate remaining Windows-specific code behind explicit platform boundaries;
3. implement one native adapter at a time with focused tests;
4. create native packages in CI without calling them releases;
5. run native GUI, secure-storage, real-network, lifecycle, and packaging acceptance;
6. publish a preview only when its limitations are explicit;
7. promote a platform to supported/released only after the release matrix contains the corresponding evidence.

This order may change when testing capacity, contributor ownership, or user demand changes. A roadmap change should update the repository documents rather than inventing percentage-complete values or calendar promises.

## Contribution areas

Useful cross-platform contributions are intentionally smaller than “port KodeWork to an OS”:

- reduce Windows-only assumptions in shared crates without changing behavior;
- add deterministic platform-boundary tests;
- improve portable CI coverage for shared crates;
- prototype secure-store or PTY adapters behind existing interfaces;
- document a reproducible native build/acceptance result with sanitized evidence;
- identify code that compiles cross-platform but still depends on Windows semantics at runtime.

Open a focused issue before undertaking a broad platform port so scope, evidence, and support claims remain reviewable. See [`../CONTRIBUTING.md`](../CONTRIBUTING.md).

## Non-goals

- no support claims based on compilation alone;
- no percentage-complete estimates without an auditable definition;
- no speculative release dates;
- no framework recommendation presented as a settled architecture decision before a proof of concept;
- no insecure credential fallback added merely to make another platform launch;
- no attempt to support every package format, desktop environment, architecture, or mobile platform at once.

The durable rule is simple: **portable code is groundwork; native support requires native evidence.**
