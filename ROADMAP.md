# KodeWork roadmap

KodeWork is an actively developed `0.x` project. This roadmap describes the direction of the public project without turning planned work into release promises.

For the exact capability that is available **today**, use [`docs/STATUS.md`](docs/STATUS.md) and [`docs/RELEASE-MATRIX.md`](docs/RELEASE-MATRIX.md) as the source of truth.

## Current release scope

- **Desktop release:** Windows 10/11 x64 MSI
- **Portable core:** selected Rust crates are continuously checked on Windows, Linux, and macOS
- **Remote targets:** Linux hosts reachable through direct SSH, Tailscale, fallback addresses, or an SSH jump host
- **Durable sessions:** tmux and Herdr integration
- **Files and assets:** SFTP browsing/transfers plus screenshot/image/PDF paste workflows
- **Local terminals:** PowerShell, Command Prompt, and WSL on Windows

Passing cross-platform Rust CI does **not** mean that macOS or Linux desktop packages are released. Native packaging, signing, installation, GUI smoke tests, sidecars, and release assets must pass on each platform before support is claimed.

## Now — reliability and contributor experience

The immediate focus is making the existing Windows remote-development loop easier to trust and easier to contribute to.

### Connection and recovery

- reduce time-to-first-useful connection feedback;
- keep reconnect behavior deterministic after transient network loss;
- improve recovery evidence around sleep/resume and application restart;
- preserve fail-closed SSH host-key and credential behavior while improving diagnostics.

### Terminal and transfer performance

- keep terminal rendering responsive during large output and multi-pane use;
- measure and improve large-file SFTP throughput;
- continue hardening pause/resume/retry/cancel and destination-conflict behavior;
- avoid performance changes that weaken transfer integrity or observability.

### Public OSS surface

- keep issue forms, contribution guidance, agent instructions, and release evidence current;
- keep dependency/security gates current and remediate actionable advisories rather than suppressing them;
- add sanitized product screenshots/demo material that contains no real infrastructure or credentials;
- improve accessibility and keyboard behavior in focused, testable increments;
- maintain a small set of well-scoped contributor tasks instead of a speculative backlog.

Contribution-ready work is tracked in [GitHub Issues](https://github.com/likangmax/KodeWork/issues). Tasks suitable for a first contribution are labeled `good first issue` when such a label applies.

## Next — product polish and verification depth

These items are planned directions, not committed release dates.

- expand native Windows acceptance coverage for installer, upgrade/uninstall, sleep/resume, scaling, CJK/IME, clipboard, and long-running sessions;
- improve keyboard navigation and accessibility semantics across modal and workspace surfaces;
- add targeted regression/integration tests around high-risk state transitions;
- profile startup, terminal, reconnect, and transfer hot paths before optimizing them;
- improve troubleshooting output while keeping secrets and private terminal/file content out of durable diagnostics;
- make release notes and documentation easier to verify against actual code and test evidence.

## Later — distribution and additional platforms

### Windows distribution

- configure a trusted commercial Authenticode signing path for stable public releases;
- publish and verify an updater manifest/service before claiming automatic updates are available;
- keep updater signing and Authenticode as separate trust layers.

### macOS and Linux desktop

KodeWork will only call a desktop platform supported after the native target has evidence for:

1. build/package creation;
2. required sidecars and platform adapters;
3. install/launch/uninstall smoke tests;
4. native GUI and terminal behavior;
5. signing/notarization requirements where applicable;
6. release assets and documentation.

The portable Rust checks are useful groundwork, but are not substitutes for those native acceptance steps. See [`docs/CROSS-PLATFORM-ROADMAP.md`](docs/CROSS-PLATFORM-ROADMAP.md) for the technical exploration.

## How roadmap items become releases

A roadmap item is not considered shipped because code exists or a unit test passes. User-visible claims should be backed by the relevant evidence:

- automated tests for deterministic logic;
- native Windows testing for desktop/GUI behavior;
- protected real-network testing for SSH/Tailscale/jump-host behavior when required;
- release workflow evidence for packaging/signing/distribution claims.

Known gaps should remain visible as `not tested`, `not available`, or `not released` until the corresponding evidence exists.

## Non-goals for the roadmap

- no artificial star/download targets;
- no platform-support claims based only on compilation;
- no security guarantees that exceed the documented threat model;
- no feature dates that the project cannot responsibly commit to.

If you want to propose a change, start with the [feature request form](https://github.com/likangmax/KodeWork/issues/new/choose) or a [GitHub Discussion](https://github.com/likangmax/KodeWork/discussions). For vulnerabilities, follow [`SECURITY.md`](SECURITY.md) instead of opening a public issue.
