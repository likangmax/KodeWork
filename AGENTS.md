# KodeWork repository instructions for coding agents

This file is the durable entry point for Codex and other coding agents working in this repository. It applies to the whole repository unless a more specific `AGENTS.md` appears deeper in the tree.

## Product truth

KodeWork is a local-first Windows workbench for durable coding sessions on private Linux hosts. The currently distributed desktop artifact is a Windows 10/11 x64 MSI. Portable Rust crates are checked on Windows, Linux, and macOS, but native macOS/Linux desktop releases are not published yet.

Tailscale supplies a network path; SSH still owns authentication and host-key verification. Herdr/tmux supply remote session continuity; they do not make the local Windows process immortal.

Never turn an implementation detail, green unit test, or saved configuration into a stronger product claim than the evidence supports.

## Read before editing

Use this order when orienting yourself:

1. `README.md` (or `README.zh-CN.md`)
2. `docs/README.md` for the current documentation map and source-of-truth boundaries
3. `docs/AGENT-GUIDE.md`
4. `docs/ARCHITECTURE.md` and relevant files under `docs/adr/`
5. `docs/STATUS.md`, `docs/TEST-MATRIX-WINDOWS.md`, and `docs/RELEASE-MATRIX.md`
6. The owning crate/component and its tests

`docs/AGENT-GUIDE.md` is the detailed operational contract. This file intentionally stays shorter and more durable.

Files listed under **Historical planning and handoffs** in `docs/README.md` are traceability records only. They can contain old commit SHAs, dependency versions, test counts, paths, and plans. Do not use them as the current checkout truth unless the task explicitly asks you to reconstruct historical state.

## Repository boundaries

- `crates/`: Rust domain, core, transport, storage, and platform adapters
- `src-tauri/`: thin Tauri shell, typed IPC, plugins, and native resources
- `src/`: React workspace, terminal, files, runtime, and settings UI
- `docs/`: architecture, ADRs, user/maintainer documentation, release evidence
- `scripts/`: reproducible build, sidecar, and verification helpers
- `.github/`: CI, issue forms, pull-request templates, dependency automation

Keep domain/state-machine logic independent from Tauri. Keep Tauri commands thin. Do not move unrelated code while fixing a focused issue.

## Security and privacy invariants

These are non-negotiable:

- Unknown SSH host keys require an explicit trust decision; changed keys are hard failures.
- Passwords, private-key material/passphrases, Tailscale auth keys, updater private keys, real hostnames, and private files must not enter committed source, fixtures, logs, screenshots, issue text, or PR text.
- Renderer persistence must not become a secret store.
- Clipboard reads require an explicit user paste action; OSC 52 remains bounded and write-only.
- SFTP writes must remain streamed/staged safely; do not replace them with whole-file buffering for convenience.
- Dangerous remote Actions must still be classified/enforced by the Rust side; the UI cannot be the final authority.
- Fail closed at trust and identity boundaries instead of silently guessing.

Use documentation-range IPs and synthetic users in examples and tests.

## Change workflow

For a normal code change:

1. Reproduce or define the observable failure/behavior.
2. Add the smallest regression test that fails before the fix when practical.
3. Change the owning module only.
4. Run focused tests first, then the repository gates below.
5. For UI/native behavior, distinguish automated evidence from real Windows GUI/network evidence.
6. Update English/Chinese docs only for behavior that is actually verified.
7. Scan the diff for secrets, machine-specific paths, generated artifacts, and accidental product-claim inflation.
8. Prepare a focused PR with what changed, what was tested, and what remains unverified.

Do not force-push protected history, publish a release, rotate signing material, or modify release assets unless the task explicitly authorizes it.

## Required verification

From the repository root, use the relevant commands and make a best effort to run the complete set before declaring a code change ready:

```powershell
npm ci
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
npm run lint
npm run test:frontend
npm run build
npm audit --omit=dev
```

Also run the repository secret scan and `git diff --check` when available in the working environment.

A passing compiler or unit test does not prove native GUI behavior, real SSH/Tailscale connectivity, installer behavior, sleep/resume behavior, or release readiness. Report those as `not tested` or `blocked` when appropriate.

## Documentation and release claims

Be precise with the words `available`, `verified`, `supported`, and `released`.

- Windows x64 is the currently distributed desktop target.
- Cross-platform Rust CI is not a macOS/Linux desktop release.
- Updater signature verification support is not the same as public automatic-update availability.
- Tauri updater signatures are not Authenticode signatures.
- Do not claim a network mode works from mocked tests or a form-save path alone.

Follow `docs/RELEASE-MATRIX.md` before changing release language.

## Pull-request handoff

A useful PR description should include:

- the user-visible or maintainer-visible problem;
- the smallest relevant implementation summary;
- tests/checks run and their result;
- native/real-network checks performed, if any;
- security/privacy considerations;
- known unverified areas or follow-up work.

When in doubt, prefer a smaller truthful change with explicit evidence over a broader unverified claim.
