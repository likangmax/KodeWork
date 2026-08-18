# Kodework Windows release build (MSI + signed updater artifacts).
# The updater signing key lives OUTSIDE the repository so it is never
# committed; this script wires it into the tauri build environment.
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$keyFile = Join-Path $env:USERPROFILE '.tauri\kodework.key'
$passFile = Join-Path $env:USERPROFILE '.tauri\kodework.pass'
if (-not (Test-Path $keyFile)) {
  Write-Error "Updater signing key not found at $keyFile. Generate one with: npx tauri signer generate --ci -w $keyFile -p <password>"
}
# tauri build's signing step reads the key from the TAURI_SIGNING_PRIVATE_KEY
# content variable (TAURI_SIGNING_PRIVATE_KEY_PATH is only honored by the
# signer sign subcommand), so load the file contents here.
$env:TAURI_SIGNING_PRIVATE_KEY = [System.IO.File]::ReadAllText($keyFile).Trim()
if (Test-Path $passFile) {
  $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = [System.IO.File]::ReadAllText($passFile).Trim()
}
Push-Location $root
try {
  & (Join-Path $PSScriptRoot 'prepare-tailscale.ps1')
  if ($LASTEXITCODE -ne 0) { throw "Tailscale sidecar build failed with exit code $LASTEXITCODE" }
  $tauriArguments = @('run', 'tauri', '--', 'build')
  $signingConfig = $null
  if (-not [string]::IsNullOrWhiteSpace($env:KODEWORK_CERT_THUMBPRINT)) {
    $timestampUrl = if ([string]::IsNullOrWhiteSpace($env:KODEWORK_TIMESTAMP_URL)) { 'http://timestamp.digicert.com' } else { $env:KODEWORK_TIMESTAMP_URL }
    $signingConfig = Join-Path ([System.IO.Path]::GetTempPath()) ("kodework-tauri-signing-" + [Guid]::NewGuid().ToString('N') + '.json')
    $overlay = [ordered]@{ bundle = [ordered]@{ windows = [ordered]@{
      certificateThumbprint = $env:KODEWORK_CERT_THUMBPRINT
      digestAlgorithm = 'sha256'
      timestampUrl = $timestampUrl
      tsp = $true
    } } }
    $overlay | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $signingConfig -Encoding utf8NoBOM
    $tauriArguments += @('--config', $signingConfig)
  } elseif ($env:KODEWORK_REQUIRE_AUTHENTICODE -eq '1') {
    throw 'KODEWORK_REQUIRE_AUTHENTICODE=1 but KODEWORK_CERT_THUMBPRINT is not configured.'
  } else {
    Write-Warning 'Building without Authenticode. Public releases must set KODEWORK_CERT_THUMBPRINT.'
  }
  & npm @tauriArguments
  if ($LASTEXITCODE -ne 0) { throw "tauri build failed with exit code $LASTEXITCODE" }
  $version = (Get-Content -Raw -LiteralPath (Join-Path $root 'package.json') | ConvertFrom-Json).version
  $msi = Get-ChildItem -LiteralPath (Join-Path $root 'target\release\bundle\msi') -Filter "*_${version}_x64_en-US.msi" | Select-Object -First 1
  if (-not $msi) { throw "release MSI for version $version was not generated" }
  $signature = Get-AuthenticodeSignature -LiteralPath $msi.FullName
  if (-not [string]::IsNullOrWhiteSpace($env:KODEWORK_CERT_THUMBPRINT) -and $signature.Status -ne 'Valid') {
    throw "Authenticode verification failed for $($msi.FullName): $($signature.Status)"
  }
} finally {
  if ($signingConfig -and (Test-Path -LiteralPath $signingConfig)) { Remove-Item -LiteralPath $signingConfig -Force }
  Pop-Location
}
Write-Host "Release build complete. Artifacts in target/release/bundle/msi/"
