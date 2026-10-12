param(
  [Parameter(Mandatory = $true)][string]$LatestJsonUrl,
  [switch]$RequireAuthenticode
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
  # Base64 decode, verify MSI cryptographically, then validate the trusted-comment version.
  & node (Join-Path $PSScriptRoot 'verify-updater-signature.mjs') $msiPath $signaturePath $manifest.version
  if ($LASTEXITCODE -ne 0) { throw 'Updater artifact signature or signed version is invalid.' }
  $authenticode = Get-AuthenticodeSignature -LiteralPath $msiPath
  if ($RequireAuthenticode -and $authenticode.Status -ne 'Valid') { throw "Authenticode is not valid: $($authenticode.Status)" }
  [pscustomobject]@{ Version = $manifest.version; Sha256 = (Get-FileHash -Algorithm SHA256 $msiPath).Hash; Authenticode = $authenticode.Status; Signer = $authenticode.SignerCertificate.Subject }
} finally {
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
