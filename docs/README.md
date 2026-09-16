# KodeWork documentation

This page is the documentation map for users, contributors, and maintainers. It also defines the current sources of truth for support, release, architecture, testing, and project direction.

## Start here by goal

| Goal | Start with |
| --- | --- |
| Install and use KodeWork | [English user guide](USER-GUIDE.md) / [中文使用指南](USER-GUIDE.zh-CN.md) |
| Troubleshoot a problem | [Troubleshooting](TROUBLESHOOTING.md) and [Support](../SUPPORT.md) |
| Understand what is supported/released | [Project status](STATUS.md) and [Release matrix](RELEASE-MATRIX.md) |
| Understand architecture/security boundaries | [Architecture](ARCHITECTURE.md), [ADRs](adr/), and [Security policy](../SECURITY.md) |
| Contribute code or documentation | [Contributing](../CONTRIBUTING.md) |
| Maintain or release the project | [Release matrix](RELEASE-MATRIX.md), [Windows test matrix](TEST-MATRIX-WINDOWS.md), and [Project status](STATUS.md) |
| See project direction | [Public roadmap](../ROADMAP.md) |
| See release history | [Changelog](CHANGELOG.md) |

Project home:

- [README (English)](../README.md)
- [README（简体中文）](../README.zh-CN.md)

## Source-of-truth hierarchy

When two documents appear to disagree, use the more specific current source below:

1. **Installable distribution:** [GitHub Releases](https://github.com/likangmax/KodeWork/releases)
2. **Current capability and known limits:** [STATUS.md](STATUS.md)
3. **Platform and release evidence contract:** [RELEASE-MATRIX.md](RELEASE-MATRIX.md)
4. **Architecture and security invariants:** [ARCHITECTURE.md](ARCHITECTURE.md), current ADRs, and [SECURITY.md](../SECURITY.md)
5. **Current public direction:** [ROADMAP.md](../ROADMAP.md)
6. **Release history:** [CHANGELOG.md](CHANGELOG.md)

A version field in `main` or a successful compile is not a substitute for published release evidence.

## User documentation

- [English user guide](USER-GUIDE.md) — installation, Linux preparation, connection modes, daily workflows, verification, and troubleshooting.
- [中文使用指南](USER-GUIDE.zh-CN.md) — 安装、Linux 准备、连接方式、日常使用、验收与排障。
- [Troubleshooting](TROUBLESHOOTING.md) — connection, authentication, terminal, transfer, and installation problems.
- [Support policy](../SUPPORT.md) — where to ask questions, what is supported, and what information is safe to share.
- [Third-party notices](THIRD-PARTY-NOTICES.md) — bundled components and licenses.

## Engineering references

- [Project status](STATUS.md) — what is implemented, usable, verified, released, and still limited.
- [Release matrix](RELEASE-MATRIX.md) — packaging, signing, installation, and platform evidence requirements.
- [Windows test matrix](TEST-MATRIX-WINDOWS.md) — automated/native evidence and explicit gaps.
- [Architecture](ARCHITECTURE.md) — runtime boundaries, data planes, security invariants, and performance rules.
- [Architecture decisions](adr/) — canonical ADRs for significant design decisions.
- [Cross-platform roadmap](CROSS-PLATFORM-ROADMAP.md) — technical exploration for macOS/Linux and longer-term platform work; not a release claim.
- [Changelog](CHANGELOG.md) — versioned user-visible changes and current unreleased work.

## Contribution and maintenance

- [Contributing](../CONTRIBUTING.md) — setup, workflow, supply-chain rules, verification, and PR expectations.
- [Security policy](../SECURITY.md) — supported versions, private vulnerability reporting, and security-sensitive contribution rules.
- [Code of conduct](../CODE_OF_CONDUCT.md) — participation and enforcement expectations.
- [Support policy](../SUPPORT.md) — user-support boundaries and public-data hygiene.
- [Roadmap](../ROADMAP.md) — public project direction and planned areas of work.

Repository history remains available through Git and GitHub. Transient handoff notes, AI-tool-specific instructions, and superseded planning snapshots are intentionally not kept in the current documentation tree.

## Documentation maintenance rules

- Use synthetic users, documentation-range IPs, and fake paths/credentials in public examples.
- Never add real infrastructure, secret-bearing logs, private terminal output, or signing material.
- Keep English and Chinese user-facing documentation aligned when behavior changes.
- Distinguish `configured`, `tested`, `verified`, `supported`, and `released`; do not silently strengthen a claim.
- Avoid transient local state, branch-specific commit SHAs, and time-sensitive test counts in durable instructions.
- Preserve historical dates in ADRs and versioned changelog entries; update only documents that claim to describe current state.

Files remain at the repository root when GitHub, Cargo, npm, Vite, Tauri, or community-health conventions expect them there.
