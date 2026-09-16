<p align="center">
  <img src="assets/branding/kodework-icon-master.png" width="112" alt="KodeWork icon">
</p>

<h1 align="center">KodeWork</h1>

<p align="center"><strong>A local-first Windows workbench for durable coding sessions on private Linux hosts.</strong></p>

<p align="center">
  <a href="https://github.com/likangmax/KodeWork/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/likangmax/KodeWork/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/likangmax/KodeWork/releases/latest"><img alt="Release" src="https://img.shields.io/github/v/release/likangmax/KodeWork?display_name=tag"></a>
  <a href="LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-green.svg"></a>
  <img alt="Windows 10/11 x64" src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D4">
</p>

<p align="center">
  <strong>English</strong> · <a href="README.zh-CN.md">简体中文</a>
</p>

<p align="center">
  <a href="https://github.com/likangmax/KodeWork/releases/latest">Download</a> ·
  <a href="docs/USER-GUIDE.md">User guide</a> ·
  <a href="ROADMAP.md">Roadmap</a> ·
  <a href="CONTRIBUTING.md">Contributing</a> ·
  <a href="SECURITY.md">Security</a>
</p>

KodeWork turns a private Linux machine into a recoverable remote coding workspace without requiring that machine to have a public IP. It combines SSH/PTY, SFTP, Tailscale or jump-host routing, tmux/Herdr session continuity, file and asset transfer, SSH port forwarding, and a native Windows desktop workflow.

> Start work on a remote Linux host, disconnect when you need to, and return to the same durable remote session later.

## Get started

### 1. Install

Download the latest **Windows x64 MSI** from [GitHub Releases](https://github.com/likangmax/KodeWork/releases/latest), install it, and launch KodeWork.

The first launch asks for **English** or **简体中文**. The MSI wizard itself is not localized yet.

### 2. Add a workstation

Select **+** beside Workstations and provide:

- Linux username and address;
- SSH authentication method;
- remote working directory;
- optional Tailscale or SSH jump-host route;
- optional tmux or Herdr runtime for durable sessions.

On the first SSH connection, verify the server host-key fingerprint before trusting it.

### 3. Learn the workflow

The [user guide](docs/USER-GUIDE.md) walks through Linux preparation, all connection modes, files, asset paste, WSL/local terminals, durable sessions, upgrades, and troubleshooting.

## Why KodeWork

Most SSH clients focus on opening a shell. KodeWork treats the remote machine as a persistent development workspace.

| Need | KodeWork approach |
| --- | --- |
| Reach a private host | Direct/LAN addresses, Tailscale discovery, fallback routes, or an SSH jump host |
| Keep remote work alive | Reattach to tmux or Herdr after local disconnects/restarts |
| Work from one desktop surface | Terminal panes, files, transfers, actions, runtime state, and web previews share project context |
| Avoid credential leakage into UI state | Password/private-key material stays behind native secret-handling boundaries |
| Move large files safely | SFTP streaming with pause/resume/retry/cancel and staged completion |
| Preview remote web services | SSH local port forwarding to loopback-only Web Preview |

## Current capability

The currently distributed desktop target is **Windows 10/11 x64**.

| Capability | Windows x64 | macOS desktop | Linux desktop |
| --- | --- | --- | --- |
| Installable KodeWork desktop release | **Available (MSI)** | Not released | Not released |
| Native GUI/install/signing acceptance | Windows release baseline | Not completed | Not completed |
| Portable Rust core CI | Checked | Checked | Checked |

Cross-platform Rust CI is not a desktop-release claim. KodeWork will only call a new desktop platform supported after native packaging, installation, GUI behavior, sidecars, signing/notarization requirements, and release assets are verified. See [Project status](docs/STATUS.md), [Release matrix](docs/RELEASE-MATRIX.md), and the [Roadmap](ROADMAP.md).

## Feature map

| Area | Current functionality |
| --- | --- |
| Network | Direct/fallback addresses, embedded or system Tailscale discovery, SSH jump hosts |
| Remote terminal | Rust SSH/PTY core, xterm.js, split panes, CJK/IME support, reconnect state |
| Durable sessions | tmux and Herdr discovery/attach workflows |
| Authentication | Password, public key, SSH Agent/Pageant, keyboard-interactive/MFA |
| Files | SFTP browsing, streaming transfer, pause/resume/retry/cancel, pinned remote folders |
| Clipboard/assets | Text copy plus explicit screenshot/image/PDF upload into the active remote workspace |
| Local terminals | PowerShell, Command Prompt, and WSL through Windows ConPTY |
| Automation | Interactive, Quick, and Background Actions with Rust-side danger classification |
| Preview | SSH local forwarding and loopback Web Preview |
| Desktop | English/Chinese UI, themes, tray, single instance, autostart, updater-signature verification support |

## Security model

KodeWork handles security-sensitive boundaries explicitly:

- unknown SSH host keys require a fingerprint decision;
- changed known host keys are hard failures;
- passwords, private-key passphrases, and Tailscale auth keys are not ordinary renderer/SQLite/log data;
- dangerous Actions are classified again by Rust, not trusted solely to the UI;
- remote OSC 52 clipboard writes are bounded, while remote clipboard reads are intentionally ignored;
- SFTP uploads remain streamed and staged rather than loading entire files into memory;
- Web Preview is limited to explicit loopback SSH forwarding.

Do not report suspected vulnerabilities in a public issue. Follow [SECURITY.md](SECURITY.md).

## Architecture

```mermaid
flowchart TB
  UI[React + xterm.js] --> IPC[Typed Tauri commands + bounded channels]
  Shell[Tauri 2 desktop shell] --> IPC
  IPC --> Core[kodework-core\nsessions · runs · tunnels · transfers]
  Core --> Domain[kodework-domain\nmodels · validation · danger policy]
  Core --> Adapters[SSH · SFTP · Tailscale · Herdr · local PTY · storage]
  Adapters --> Host[Private Linux host\nSSH / SFTP / tmux / Herdr]
```

Rust owns connection truth, authentication boundaries, reconnect generations, transfer state, and remote-session continuity. React owns presentation and renderer lifecycle. Tailscale can provide the network path; SSH still performs target authentication and host-key verification.

See [Architecture](docs/ARCHITECTURE.md) and the numbered [ADRs](docs/adr/).

## Development

### Prerequisites

- Windows 10/11 x64 for native desktop development
- Rust 1.98.0 with the MSVC toolchain (`rust-toolchain.toml`)
- Node.js 24+ and npm
- Tauri 2 Windows development prerequisites

### Common commands

```powershell
npm ci
npm run dev              # browser-only UI preview; no native SSH/credentials
npm run desktop          # native Tauri development
npm run lint
npm run test:frontend
npm run build
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked --workspace --all-features
```

For repository rules, testing expectations, and PR guidance, read [CONTRIBUTING.md](CONTRIBUTING.md). Coding agents should start with [AGENTS.md](AGENTS.md).

## Repository layout

```text
crates/        Rust domain, core, transport, storage, and platform adapters
src-tauri/     Thin Tauri shell, typed IPC, plugins, native resources
src/           React workspace, terminal, files, runtime, settings UI
docs/          User guides, architecture, ADRs, test/release evidence
scripts/       Reproducible build, sidecar, and verification helpers
.github/       CI, release automation, issue forms, PR templates
```

## Project status and roadmap

KodeWork is usable today but remains an actively developed `0.x` project. Near-term work focuses on connection/recovery reliability, terminal and transfer performance, Windows acceptance evidence, accessibility, and contributor experience.

- [Current project status](docs/STATUS.md)
- [Public roadmap](ROADMAP.md)
- [Windows test matrix](docs/TEST-MATRIX-WINDOWS.md)
- [Release matrix](docs/RELEASE-MATRIX.md)
- [Changelog](docs/CHANGELOG.md)

## Contributing

Focused issues and pull requests are welcome. Start with [CONTRIBUTING.md](CONTRIBUTING.md), use the structured issue forms, and keep real credentials, hostnames, private files, and signing material out of public reports.

Questions and ideas can go to [GitHub Discussions](https://github.com/likangmax/KodeWork/discussions).

## License

KodeWork is licensed under the [MIT License](LICENSE). Bundled Tailscale components retain their upstream BSD-3-Clause license; see [third-party notices](docs/THIRD-PARTY-NOTICES.md).
