param(
  [Parameter(Mandatory = $true)][string]$LatestJsonUrl,
  # Authenticode is part of the public distribution contract, so it is enforced
  # unless a caller explicitly opts out with -RequireAuthenticode:$false.
  [switch]$RequireAuthenticode = $true,
  # Supply the version this release is expected to be. Without it the manifest is
  # only compared against itself, and a correctly signed OLDER release (a
  # rollback) passes every check below.
  [string]$ExpectedVersion
)
$ErrorActionPreference = 'Stop'
$temporary = Join-Path ([System.IO.Path]::GetTempPath()) ("kodework-release-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
  $manifestPath = Join-Path $temporary 'latest.json'
  Invoke-WebRequest -UseBasicParsing -Uri $LatestJsonUrl -OutFile $manifestPath
  $manifest = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
  $platform = $manifest.platforms.'windows-x86_64-msi'
  if (-not $platform) { $platform = $manifest.platforms.'windows-x86_64' }
  if (-not $platform.url -or -not $platform.signature) { throw 'latest.json is missing windows-x86_64 url/signature.' }
  $msiPath = Join-Path $temporary 'kodework.msi'
  Invoke-WebRequest -UseBasicParsing -Uri $platform.url -OutFile $msiPath
  $signaturePath = "$msiPath.sig"
  [System.IO.File]::WriteAllText($signaturePath, [string]$platform.signature)
  if ($manifest.version -notmatch '^\d+\.\d+\.\d+$') { throw 'Updater version must be stable X.Y.Z.' }
  if ($ExpectedVersion) {
    if ($ExpectedVersion -notmatch '^\d+\.\d+\.\d+$') { throw 'ExpectedVersion must be stable X.Y.Z.' }
    if ($manifest.version -ne $ExpectedVersion) {
      throw "Published manifest version $($manifest.version) does not match the expected version $ExpectedVersion (possible rollback or stale channel)."
    }
  } else {
    Write-Warning 'No -ExpectedVersion supplied: the manifest was only checked against itself, so a rollback to an older signed release would not be detected.'
  }
  # Base64 decode, verify MSI cryptographically, then validate the trusted-comment version.
  $verification = & node (Join-Path $PSScriptRoot 'verify-updater-signature.mjs') $msiPath $signaturePath $manifest.version 2>&1
  if ($LASTEXITCODE -ne 0) { throw 'Updater artifact signature or signed version is invalid.' }
  # Exit status alone is not proof; require the verifier's own confirmation line.
  if (($verification -join "`n") -notmatch 'Updater artifact signature and authenticated version verified\.') {
    throw 'Updater verification exited without confirming a verified signature.'
  }
  $authenticode = Get-AuthenticodeSignature -LiteralPath $msiPath
  if ($RequireAuthenticode -and $authenticode.Status -ne 'Valid') { throw "Authenticode is not valid: $($authenticode.Status)" }
  [pscustomobject]@{ Version = $manifest.version; Sha256 = (Get-FileHash -Algorithm SHA256 $msiPath).Hash; Authenticode = $authenticode.Status; Signer = $authenticode.SignerCertificate.Subject }
} finally {
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
