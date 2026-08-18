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
  $platform = $manifest.platforms.'windows-x86_64'
  if (-not $platform.url -or -not $platform.signature) { throw 'latest.json is missing windows-x86_64 url/signature.' }
  $msiPath = Join-Path $temporary 'kodework.msi'
  Invoke-WebRequest -UseBasicParsing -Uri $platform.url -OutFile $msiPath
  $signaturePath = "$msiPath.sig"
  [System.IO.File]::WriteAllText($signaturePath, [string]$platform.signature)
  $publicKeyEncoded = (Get-Content -Raw (Join-Path $PSScriptRoot '..\src-tauri\tauri.conf.json') | ConvertFrom-Json).plugins.updater.pubkey
  $publicKeyPath = Join-Path $temporary 'updater.pub'
  [System.IO.File]::WriteAllText($publicKeyPath, [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($publicKeyEncoded)))
  $minisign = Get-Command minisign -ErrorAction SilentlyContinue
  if (-not $minisign) { throw 'minisign is required for cryptographic updater verification; install it on the release-validation machine.' }
  & $minisign.Source -Vm $msiPath -x $signaturePath -p $publicKeyPath
  if ($LASTEXITCODE -ne 0) { throw 'Tauri updater signature verification failed.' }
  $authenticode = Get-AuthenticodeSignature -LiteralPath $msiPath
  if ($RequireAuthenticode -and $authenticode.Status -ne 'Valid') { throw "Authenticode is not valid: $($authenticode.Status)" }
  [pscustomobject]@{ Version = $manifest.version; Sha256 = (Get-FileHash -Algorithm SHA256 $msiPath).Hash; Authenticode = $authenticode.Status; Signer = $authenticode.SignerCertificate.Subject }
} finally {
  Remove-Item -LiteralPath $temporary -Recurse -Force -ErrorAction SilentlyContinue
}
