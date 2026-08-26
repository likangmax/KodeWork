# Documentation map

Start here instead of browsing every Markdown file. The documents are grouped by audience and purpose.

## I want to use KodeWork

- [English user guide](USER-GUIDE.md) — installation, first-run language, Linux preparation, every connection mode, daily use, verification, and troubleshooting.
- [中文零基础使用指南](USER-GUIDE.zh-CN.md) — 安装、首次语言选择、Linux 准备、全部连接方式、日常操作、验收和故障排查。
- [Troubleshooting guide](TROUBLESHOOTING.md) — diagnosis and fixes for connection, authentication, transfer, terminal, and installation issues.
- [English product overview](../README.md) — features, platform status, downloads, and development quick start.
- [中文项目首页](../README.zh-CN.md) — 功能定位、平台状态、下载与开发入口。
- [Changelog](CHANGELOG.md) — user-visible changes by release.
- [Third-party notices](THIRD-PARTY-NOTICES.md) — bundled components and license notices.

## I am an agent or maintainer

- [Codex current handoff (中文)](HANDOFF-CODEX.zh-CN.md) — current `main` snapshot, verified local/GitHub gates, known gaps, and safe continuation order.
- [Claude Code historical handoff (中文)](HANDOFF-CLAUDE-CODE.zh-CN.md) — archived execution snapshot retained for audit/history; not the current checkout state.
- [Agent and maintainer guide](AGENT-GUIDE.md) — repository orientation, configuration semantics, safety boundaries, validation gates, and release procedure.
- [中文 Agent 与维护者指南](AGENT-GUIDE.zh-CN.md) — 面向中文 Agent 的可执行配置、测试、隐私和发布流程。
- [Architecture](ARCHITECTURE.md) — runtime boundaries, data planes, security invariants, and performance rules.
- [Architecture decisions](adr/) — numbered ADRs; each decision has one canonical file.
- [Windows test matrix](TEST-MATRIX-WINDOWS.md) — automated, native-machine, and explicitly unverified scenarios.
- [Project status](STATUS.md) — current release scope and known distribution limits.
- [Release matrix](RELEASE-MATRIX.md) — packaging, signing, and trust requirements per platform.
- [Cross-platform roadmap](CROSS-PLATFORM-ROADMAP.md) — Windows hardening, macOS/Linux desktop, and iOS/Android strategy.
- [Brand and icon](BRAND-ICON.md) — icon semantics and source assets.
- [Improvement roadmap](IMPROVEMENT-ROADMAP.md) — phased plan for hardening, docs, UX, and distribution.
- [Contributing](../CONTRIBUTING.md) — setup, workflow, coding standards, and PR checklist.

## Repository policy

The repository root contains only files that GitHub, Cargo, npm, Vite, Tauri, or community-health features expect there. Build manifests are not documentation and should not be moved just to shorten the file list. GitHub-recognized files such as `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, `SECURITY.md`, `LICENSE`, and both language READMEs also remain at the root.

Generated output, dependency directories, local upstream research, secrets, signing keys, real infrastructure details, and machine-specific fixtures are ignored and must never be included in the public source tree.
