# ADR-0003: Cross-platform desktop targets and native boundaries

## Status

Accepted — 2026-08-17

## Context

Kodework's product behavior is not Windows-specific: SSH/PTY, SFTP,
Tailscale address discovery, Herdr/tmux sessions, Actions, transfers and
workspace state should be shared by Windows, macOS and Linux.  The first
release, however, was a Windows product and the desktop shell still had
several Windows-only assumptions:

- Win32 Credential Manager/DPAPI imports were unconditional.
- Clipboard commands imported a crate named `kodework-platform-win` directly.
- Tailscale discovery used `.exe`, `ProgramFiles` and `LOCALAPPDATA` in shared
  code.
- The release script produced only a Windows MSI.

Treating a successful Windows build as proof of three-platform support would
hide compile, packaging, keyring, clipboard and process-lifecycle failures.

## Decision

### 1. Keep the core portable

`kodework-domain`, `kodework-core`, `kodework-ssh`, `kodework-sftp`,
`kodework-storage`, `kodework-network`, `kodework-tailscale` and
`kodework-herdr` must not depend on Tauri or Win32 APIs.  The Tauri crate is a
thin shell and may only translate DTOs, manage windows/plugins and forward
typed streams.

### 2. Make native integrations explicit

- Windows uses the existing `kodework-secrets-win` Credential Manager adapter
  for backwards-compatible records.  Its Win32 module is compiled only on
  Windows.
- macOS and Linux use `kodework-secrets-native` and the `NativeKeyring`
  provider (`Keychain` on macOS, Secret Service on Linux).
- The clipboard implementation is exposed through the platform adapter alias;
  its validated text/image/PDF/file workflow is shared where the native
  clipboard backend supports it.  Platform-specific limitations must be
  reported as typed errors, never as a fabricated upload.
- Tailscale executable resolution uses the current target's executable suffix,
  target sidecar directory and platform search paths.  Shared code never
  assumes `.exe`.

### 3. Release on native runners

Cross-compilation from Windows is not the release proof.  GitHub Actions must
build and test on native runners:

| Target | Required artifacts | Required trust checks |
|---|---|---|
| Windows x64 | MSI and/or NSIS installer, updater signature | Authenticode + Tauri updater signature |
| macOS arm64/x64 | DMG and ZIP | Developer ID signing + notarization + updater signature |
| Linux x64 | AppImage, deb, rpm | updater signature, checksums, package smoke test |
| Linux arm64 | AppImage/deb when a native runner is available | checksums and smoke test |

Each target bundles a matching `tailscale`/`tailscaled` sidecar or clearly
declares system-daemon-only mode.  A release job must fail if a required
sidecar, license notice, updater artifact or checksum is missing.

### 4. Preserve Windows upgrade identity

The existing Windows identifier remains stable while the cross-platform work
lands, so a new Windows installer upgrades the current installation instead
of creating a second app.  A future product-name change requires an explicit
installer migration test.

### 5. Define the proof boundary

The repository may say “cross-platform architecture” after portable core and
target-specific compile checks pass.  It may say “macOS/Linux release” only
after native-runner packaging, installation, launch, keyring, clipboard,
Tailscale system-daemon discovery, SSH PTY, SFTP and updater smoke tests pass
on that operating system.

## Consequences

### Positive

- A UI rewrite cannot silently reintroduce Win32 dependencies into the core.
- Credentials are stored by the host operating system rather than a plaintext
  fallback database.
- Tailscale sidecars and release artifacts are target-specific and auditable.
- The project can publish the three-platform asset layout users expect from a
  mature GitHub desktop project.

### Negative

- Native CI runners and signing credentials are required for release quality.
- Linux keyring availability depends on the user's Secret Service session.
- Clipboard file/image support must be verified separately on X11, Wayland,
  macOS and Windows.
- The current local Windows machine cannot prove macOS notarization or Linux
  desktop integration by itself.

## Verification

Portable checks run locally:

```powershell
cargo check -p kodework-domain --target x86_64-unknown-linux-gnu
cargo check -p kodework-core --target x86_64-unknown-linux-gnu
cargo check -p kodework-secrets-native --target x86_64-unknown-linux-gnu
cargo check -p kodework-secrets-win --target x86_64-unknown-linux-gnu
cargo check -p kodework-platform-win --target x86_64-unknown-linux-gnu
```

The complete Tauri shell and bundle checks belong to native GitHub runners;
the CI workflow must not mark a cross-platform release green from a Windows
only build.
