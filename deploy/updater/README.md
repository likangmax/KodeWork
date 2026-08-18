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
5. Run `scripts/verify-release.ps1 -LatestJsonUrl https://updates.kodework.dev/stable/latest.json -RequireAuthenticode` from a clean Windows machine.

The updater minisign signature and Authenticode serve different trust boundaries; both are mandatory for public production releases. Publishing is intentionally refused when the expected MSI or updater signature is absent.
