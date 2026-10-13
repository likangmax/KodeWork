# Kodework updater production runbook

The client uses the static Tauri v2 manifest at `https://updates.kodework.dev/stable/latest.json`.

Required external assets (never commit them):

- DNS control for `updates.kodework.dev`;
- an HTTPS origin or S3-compatible bucket/CDN;
- the existing Tauri updater private key in the release runner;
- a commercial Authenticode certificate installed in the Windows certificate store or exposed by an HSM/token.

Release sequence:

1. Set `KODEWORK_CERT_THUMBPRINT` and optionally `KODEWORK_TIMESTAMP_URL`, then run `scripts/build-release.ps1`.
2. Require `Get-AuthenticodeSignature` to report `Valid` for the MSI before publishing.
3. Set `KODEWORK_UPDATE_BASE_URL=https://updates.kodework.dev` and optionally `KODEWORK_UPDATE_S3_URI=s3://bucket/prefix`, then run `scripts/publish-update.ps1 -Version X.Y.Z`.
4. Deploy this `Caddyfile` (or equivalent CDN headers) with the generated `release-channel` tree mounted at `/srv/kodework-updates`.
5. Run `scripts/verify-release.ps1 -LatestJsonUrl https://updates.kodework.dev/stable/latest.json -ExpectedVersion X.Y.Z` from a clean Windows machine. Authenticode is required by default; pass `-RequireAuthenticode:$false` only for a deliberate unsigned rehearsal. `-ExpectedVersion` is what makes a rollback (a correctly signed *older* release) detectable, because the manifest is otherwise only compared against itself.

The updater minisign signature and Authenticode serve different trust boundaries; both are mandatory for public production releases. Publishing is intentionally refused when the expected MSI or updater signature is absent.

## Rehearsing the whole channel without provisioning anything

The steps above need DNS, TLS and a bucket, which is exactly why the channel is
easy to leave unverified. The same chain can be exercised on one machine,
including real TLS:

0. Optional but recommended, so the transport matches production: create a local
   CA plus a loopback server certificate and trust the CA for the current user
   only (no administrator rights needed). Any local CA works; the point is that
   nothing in the rehearsal skips certificate validation.
1. Build a signed MSI (`scripts/build-release.ps1`, or `npx tauri build` with
   `TAURI_SIGNING_PRIVATE_KEY` set to a rehearsal key).
2. Publish into a local tree. `-AllowInsecureLoopback` is the only way to use an
   `http://` BaseUrl, and it accepts loopback addresses only; an `https://` BaseUrl
   needs no switch:

   ```powershell
   scripts/publish-update.ps1 -Version X.Y.Z -BaseUrl https://127.0.0.1:8443 `
     -Destination $env:TEMP\channel
   ```

3. Serve it, with `--pfx` for TLS or without it for plain loopback HTTP:

   ```
   node scripts/serve-update-channel.mjs --root $env:TEMP\channel --port 8443 `
     --pfx <server.pfx> --pfx-passphrase <secret>
   ```

4. Verify it the way a client does, including rollback detection:

   ```powershell
   scripts/verify-release.ps1 -LatestJsonUrl https://127.0.0.1:8443/stable/latest.json `
     -ExpectedVersion X.Y.Z -RequireAuthenticode:$false
   ```

The manifest embeds the artifact URL, so the port used to serve the tree has to
be the port passed as `-BaseUrl` in step 2. The rehearsal proves the manifest
shape, the artifact download, the minisign signature and the version binding; it
deliberately does not stand in for Authenticode, which needs the commercial
certificate.

The manifest carries the artifact under both `windows-x86_64-msi` and
`windows-x86_64`: `tauri-plugin-updater` searches `{os}-{arch}-{installer}` first
and falls back to `{os}-{arch}`, so publishing both spellings removes the
dependency on that fallback.

`node scripts/check-shipped-graph.mjs` is the companion check for the opposite
question: that a crate carrying a Linux-desktop advisory (for example `glib`) is
not linked into the shipped Windows artifact.

## Verifying the upgrade over an installed copy

Whether an installed client actually migrates is a property of the MSI database,
not of the update channel, and it can be checked against the copy already on the
machine without installing anything:

- the new package's `UpgradeCode` must equal the installed product's, its
  `ProductCode` must differ (that is what makes it a major upgrade rather than a
  re-install), its `ProductVersion` must be greater, the `Upgrade` table must
  cover the installed version, and `RemoveExistingProducts` must be scheduled in
  `InstallExecuteSequence` - WiX's `MajorUpgrade` removes the previous version that
  way, not through the `Upgrade` table's `Remove` column;
- read both sides with the Windows Installer COM API: the installed product's
  cached package via `Installer.ProductInfo(productCode, 'LocalPackage')`, and the
  new package's `Property`, `Upgrade` and `InstallExecuteSequence` tables.
