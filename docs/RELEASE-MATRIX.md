# Kodework release matrix

This document is the release contract for the three desktop targets.  It is
deliberately stricter than “the compiler produced a binary”: a release is
usable only when the package, native integration and update trust chain all
pass on the target operating system.

## Target artifacts

| Operating system | Architectures | Installer / archive | Sidecar policy |
|---|---|---|---|
| Windows | x86_64 | MSI (primary), NSIS/portable ZIP (optional) | bundled `tailscale.exe` and `tailscaled.exe` for EmbeddedUserspace |
| macOS | arm64, x86_64 | DMG + ZIP | matching Mach-O sidecars; system daemon remains supported |
| Linux | x86_64 | AppImage + deb + rpm | matching ELF sidecars; system daemon remains supported |
| Linux | arm64 | AppImage/deb when native CI is available | matching arm64 sidecars |

Every release also publishes:

- SHA-256 checksums for every installer/archive and updater manifest;
- Tauri updater signature files generated outside the repository;
- `TAILSCALE-LICENSE.txt` and a third-party notices document;
- a release note stating which targets were actually tested and which features
  require a system Tailscale daemon.

## Updater manifest shape

The static endpoint in `tauri.conf.json` can serve one manifest for every
target.  The `platforms` keys must match Tauri's updater lookup, including the
bundle type when one is available:

```json
{
  "version": "0.3.0",
  "notes": "...",
  "pub_date": "2026-08-17T00:00:00Z",
  "platforms": {
    "windows-x86_64-msi": { "url": "...", "signature": "..." },
    "darwin-aarch64-app": { "url": "...", "signature": "..." },
    "darwin-x86_64-app": { "url": "...", "signature": "..." },
    "linux-x86_64-appimage": { "url": "...", "signature": "..." },
    "linux-x86_64-deb": { "url": "...", "signature": "..." },
    "linux-x86_64-rpm": { "url": "...", "signature": "..." }
  }
}
```

The signature values are the contents of the corresponding `.sig` files, not
their filenames.  A release job must validate that every URL is reachable and
that every signature verifies against the public key embedded in the app.

## Build rules

1. Run `npm ci` from the lockfile; never use a mutable dependency install in a
   release job.
2. Build the frontend and run Rust/frontend quality gates before bundling.
3. Build the Tailscale sidecar for the exact target triple, verify its version,
   and fail if the binary is not present in the expected Tauri external-bin
   location.
4. Keep updater private keys, Apple signing/notarization secrets and
   Authenticode certificates in CI secret storage only.
5. Verify generated artifacts after bundling, not just the build command's exit
   code.

## Required smoke tests

Each native runner must perform these checks before publishing an asset:

- install/launch/uninstall (or portable launch) without a second instance;
- create/open a local database in the platform app-data directory;
- read/write one non-sensitive preference;
- open the native keyring adapter and verify a short-lived test secret is
  removed afterward;
- copy/paste text and, where supported, image/PDF/file clipboard data;
- start with system-daemon Tailscale discovery disabled and report a typed
  “not available” state rather than spawning a visible shell;
- connect to the fake SSH server, render PTY output, resize, disconnect and
  reconnect;
- upload/download a fixture through SFTP and verify byte equality;
- verify the updater manifest and signature without installing an unsigned
  artifact.

Real Tailscale/Herdr tests remain a separate protected job because they need a
test network and must never place an auth key in logs or artifacts.

## Current status (2026-08-17)

- Windows development and MSI flow are the validated product baseline.
- Portable Rust adapters and target-aware Tailscale path resolution are now in
  the codebase.
- macOS/Linux desktop bundles are **not** declared released until native CI
  jobs and the smoke tests above exist and are green.
