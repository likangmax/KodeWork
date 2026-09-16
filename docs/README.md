# KodeWork documentation map

Use this page to find the right document without treating historical handoffs as current project state.

## Use KodeWork

- [English user guide](USER-GUIDE.md) — installation, Linux preparation, connection modes, daily workflows, verification, and troubleshooting.
- [中文零基础使用指南](USER-GUIDE.zh-CN.md) — 安装、Linux 准备、连接方式、日常使用、验收与排障。
- [Troubleshooting](TROUBLESHOOTING.md) — connection, authentication, terminal, transfer, and installation problems.
- [Changelog](CHANGELOG.md) — user-visible changes by release.
- [Third-party notices](THIRD-PARTY-NOTICES.md) — bundled components and licenses.

Project home:

- [README (English)](../README.md)
- [README（简体中文）](../README.zh-CN.md)

## Understand the current project

These files are the durable sources of truth for current capability and project direction:

- [Project status](STATUS.md) — what is implemented/usable today and known distribution limits.
- [Release matrix](RELEASE-MATRIX.md) — packaging, signing, installation, and platform evidence requirements.
- [Public roadmap](../ROADMAP.md) — current/next/later direction without release-date promises.
- [Windows test matrix](TEST-MATRIX-WINDOWS.md) — automated/native evidence and explicit gaps.
- [Architecture](ARCHITECTURE.md) — runtime boundaries, data planes, security invariants, and performance rules.
- [Architecture decisions](adr/) — canonical ADRs for significant design decisions.
- [Cross-platform roadmap](CROSS-PLATFORM-ROADMAP.md) — technical exploration for macOS/Linux and longer-term platform work.

## Contribute or maintain

Start with:

- [Contributing](../CONTRIBUTING.md) — setup, workflow, verification, and PR expectations.
- [AGENTS.md](../AGENTS.md) — durable repository-wide instructions for coding agents and contributors.
- [Agent and maintainer guide](AGENT-GUIDE.md) — detailed operational, security, testing, and release procedures.
- [中文 Agent 与维护者指南](AGENT-GUIDE.zh-CN.md) — 中文执行指南。
- [Security policy](../SECURITY.md) — vulnerability reporting and security boundaries.
- [Code of conduct](../CODE_OF_CONDUCT.md) — participation expectations.

## Historical planning and handoffs

The following files are retained for traceability. They may contain old commit SHAs, test counts, completed tasks, local-machine facts, or plans that have since changed. Do **not** use them as the source of truth for the current checkout.

- [Codex handoff snapshot (中文)](HANDOFF-CODEX.zh-CN.md)
- [Claude Code historical handoff (中文)](HANDOFF-CLAUDE-CODE.zh-CN.md)
- [Legacy next-steps plan (中文)](NEXT-STEPS.md)
- [Legacy improvement roadmap](IMPROVEMENT-ROADMAP.md)

For new planning, use [`../ROADMAP.md`](../ROADMAP.md) and open GitHub issues instead.

## Repository policy

Files remain at the repository root when GitHub, Cargo, npm, Vite, Tauri, or community-health conventions expect them there. Generated output, dependency directories, local research caches, credentials, signing keys, real infrastructure details, and machine-specific fixtures must not be committed.
