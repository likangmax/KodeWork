# Contributing

## Before opening a pull request

1. Read `docs/ARCHITECTURE.md`, `docs/TEST-MATRIX-WINDOWS.md` and the relevant ADRs.
2. Keep the Rust core independent from Tauri types. Renderer commands should remain thin and typed.
3. Do not commit credentials, Tailscale auth keys, SSH keys, updater keys, generated MSI files, or files from `references/`.
4. Add a regression test for protocol, state-machine, security, transfer, or parser changes.
5. Run the complete local gates:

```powershell
npm run lint
npm run test:frontend
npm run build
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
```

## Pull request guidance

- Explain the user-visible behavior and the failure mode being addressed.
- Include screenshots or a short recording for UI changes, with hostnames and sensitive data redacted.
- Call out schema, updater, installer, permission, CSP, or backwards-compatibility changes explicitly.
- Keep unrelated formatting or dependency upgrades out of focused fixes.

## Upstream research and licenses

The ignored `references/` directory contains read-only research checkouts. They are not build inputs and must not be copied into production code without reviewing the source commit and license. Prefer a clean reimplementation with a note in `docs/`.
