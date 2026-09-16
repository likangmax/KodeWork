# KodeWork release matrix

This document defines what evidence is required before KodeWork calls a desktop target **released** or **supported**. It is intentionally stricter than “the compiler produced a binary.”

For installable artifacts, [GitHub Releases](https://github.com/likangmax/KodeWork/releases) is the distribution source of truth. The source tree can be ahead of the latest published release.

## Evidence states

A target can progress through these states:

1. **Code exists** — implementation is present.
2. **CI checked** — deterministic/core checks pass for the target/toolchain.
3. **Native verified** — packaging, install/launch, GUI/platform integration, sidecars, and required smoke tests were exercised on the native target.
4. **Released** — the project published the intended artifact through the official release workflow/channel.
5. **Supported** — the project documents the target as part of the supported user-facing product surface.

Do not skip from CI-checked to released/supported without the native and distribution evidence required for that platform.

## Current desktop matrix

| Operating system | Architecture | Portable/core CI | Native desktop package | Public release status |
| --- | --- | --- | --- | --- |
| Windows 10/11 | x86_64 | Yes | MSI | **Released / current product baseline** |
| macOS | arm64 | Yes (portable Rust scope) | Not released | Not supported as desktop target |
| macOS | x86_64 | Core portability work only | Not released | Not supported as desktop target |
| Linux | x86_64 | Yes (portable Rust scope) | Not released | Not supported as desktop target |
| Linux | arm64 | Limited/planned | Not released | Not supported as desktop target |

Cross-platform core CI is engineering evidence, not a packaging/signing/GUI release claim.

## Current Windows stable artifact contract

A stable Windows release is expected to publish immutable assets for one Windows x64 MSI:

- `*.msi` — installer;
- `*.msi.sig` — Tauri updater signature for the MSI;
- `*.msi.sha256` — SHA-256 manifest for the MSI.

The repository also carries [third-party notices](THIRD-PARTY-NOTICES.md). Do not claim that a file is attached to every GitHub Release unless the release workflow actually publishes it as an asset.

Tauri updater signatures and Windows Authenticode serve different trust purposes. A stable release path must not treat one as a substitute for the other.

## Stable Windows publication rules

Before publishing a stable Windows tag, the release path must:

1. resolve an exact stable semver tag (`vX.Y.Z`);
2. verify the tagged commit is contained in `main`;
3. verify Cargo workspace, npm package metadata/lockfile, and Tauri version agree with the tag;
4. install frontend dependencies from the lockfile (`npm ci`);
5. build the pinned Windows/Tailscale sidecar set expected by the Tauri bundle;
6. require the Tauri updater private key needed to produce the updater signature;
7. require the trusted Authenticode certificate configuration for stable publication;
8. build the MSI and verify the updater signature exists;
9. compute and stage a SHA-256 manifest;
10. publish only after artifact verification succeeds;
11. if a release/tag already exists, refuse destructive overwrite unless the existing assets are byte-identical to the verified staged assets.

Secrets and certificate material belong in protected CI/host secret storage, never in source, logs, artifacts, Issues, PRs, or documentation examples.

## Updater manifest contract

A public updater service is **not** considered available merely because the application contains updater verification code. Before claiming automatic updates, the project must verify a reachable manifest, correct platform key, downloadable artifact, and valid signature against the public key embedded in the app.

A future manifest should follow the shape expected by Tauri. Example placeholders are deliberately version-agnostic:

```json
{
  "version": "X.Y.Z",
  "notes": "...",
  "pub_date": "YYYY-MM-DDTHH:MM:SSZ",
  "platforms": {
    "windows-x86_64-msi": {
      "url": "https://example.invalid/KodeWork-X.Y.Z-x64.msi",
      "signature": "<contents of the matching .sig file>"
    }
  }
}
```

`example.invalid` is intentionally non-routable. Do not replace it with a real endpoint in public docs until the service is actually provisioned and verified.

## Required Windows smoke evidence

For a stable Windows desktop release, native acceptance should cover the applicable items below in addition to CI:

- install/launch/uninstall and single-instance behavior;
- expected application data/database location and a non-sensitive preference round-trip;
- native secret-store availability with a short-lived synthetic test secret removed afterward;
- local PowerShell/CMD and available WSL terminal behavior;
- fake/test SSH PTY rendering, resize, disconnect, and reconnect;
- SFTP upload/download fixture byte equality;
- clipboard flows using synthetic content;
- tray/autostart behavior when changed;
- bundled/system Tailscale path behavior when changed;
- updater/signature verification path without installing an unsigned/untrusted artifact.

Real Tailscale/Herdr/infrastructure tests remain protected evidence because they may require non-public test networks. Their secrets and infrastructure details must not be copied into public logs or artifacts.

## Promotion requirements for a new desktop platform

macOS or Linux must not be called supported/released until the target has evidence for:

1. reproducible native package creation;
2. correct sidecars/platform adapters;
3. install/launch/uninstall or equivalent portable-launch smoke tests;
4. native GUI, terminal, keyring/secret-store, filesystem, clipboard, and lifecycle behavior;
5. signing/notarization/trust-chain requirements where applicable;
6. platform-specific updater/release artifact verification;
7. documentation and support expectations for the target;
8. a published release artifact through the official channel.

Planned package formats (for example DMG, AppImage, deb, or rpm) remain design direction until those native release pipelines exist and pass.

## Release evidence rules

- Use lockfiles; do not use mutable dependency installation in release jobs.
- Verify generated artifacts after bundling, not just the build command exit code.
- Third-party Actions should remain pinned to reviewed full commit SHAs.
- Release jobs should use explicit least-privilege permissions and expose signing material only to the jobs/steps that require it.
- Do not execute untrusted contributor code in a context that has release secrets or write-capable publishing credentials.
- A green workflow proves only the checks it actually ran. External certificate provisioning, updater hosting, repository branch protection, and release provenance must be verified separately.

See [STATUS.md](STATUS.md), [TEST-MATRIX-WINDOWS.md](TEST-MATRIX-WINDOWS.md), and [../SECURITY.md](../SECURITY.md) for the complementary current-state and security evidence.
