param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$BaseUrl = $env:KODEWORK_UPDATE_BASE_URL,
  [string]$Destination = (Join-Path $PSScriptRoot '..\release-channel'),
  [string]$Notes = 'Stability, performance and security improvements.',
  [switch]$RequireUpload
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($BaseUrl)) { throw 'BaseUrl or KODEWORK_UPDATE_BASE_URL is required.' }
if ($BaseUrl -notmatch '^https://') { throw 'Updater BaseUrl must use HTTPS.' }
if ($Version -notmatch '^\d+\.\d+\.\d+$') { throw 'Updater version must be stable X.Y.Z.' }
$msiDirectory = Join-Path $root 'target\release\bundle\msi'
$msi = Get-ChildItem -LiteralPath $msiDirectory -Filter "*_${Version}_x64_en-US.msi" | Select-Object -First 1
if (-not $msi) { throw "MSI for version $Version was not found in $msiDirectory" }
$signaturePath = $msi.FullName + '.sig'
if (-not (Test-Path -LiteralPath $signaturePath)) { throw "Updater signature missing: $signaturePath" }

# Bind the published version to the application metadata instead of trusting the
# -Version argument on its own. The publisher always runs from the repository
# that produced the MSI, so a divergence here is a mistake, not a use case.
$configVersion = (Get-Content -Raw -LiteralPath (Join-Path $root 'src-tauri\tauri.conf.json') | ConvertFrom-Json).version
if ($configVersion -ne $Version) {
  throw "Requested version $Version does not match src-tauri/tauri.conf.json ($configVersion)."
}

$signature = [System.IO.File]::ReadAllText($signaturePath).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) { throw 'Updater signature is empty.' }
# Verify the authentic MSI and signed version before copying or publishing.
$verification = & node (Join-Path $PSScriptRoot 'verify-updater-signature.mjs') $msi.FullName $signaturePath $Version 2>&1
if ($LASTEXITCODE -ne 0) { throw 'Updater MSI signature/version verification failed.' }
if (($verification -join "`n") -notmatch 'Updater artifact signature and authenticated version verified\.') {
  throw 'Updater verification exited without confirming a verified signature; refusing to publish.'
}
$channel = Join-Path $Destination 'stable'
New-Item -ItemType Directory -Force -Path $channel | Out-Null
$artifactName = "kodework-windows-$Version-x86_64.msi"
$publishedPath = Join-Path $channel $artifactName
# Never replace already-published bytes for the same version: a silent swap would
# leave the announced version pointing at a different installer.
if (Test-Path -LiteralPath $publishedPath) {
  $existingHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $publishedPath).Hash
  $candidateHash = (Get-FileHash -Algorithm SHA256 -LiteralPath $msi.FullName).Hash
  if ($existingHash -ne $candidateHash) {
    throw "Update channel already holds a different $artifactName; refusing to overwrite an immutable published artifact."
  }
}
Copy-Item -LiteralPath $msi.FullName -Destination $publishedPath -Force
$pubDate = [DateTimeOffset]::UtcNow.ToString('o')
$base = $BaseUrl.TrimEnd('/')
$manifest = [ordered]@{
  version = $Version
  notes = $Notes
  pub_date = $pubDate
  platforms = [ordered]@{
    'windows-x86_64' = [ordered]@{
      signature = $signature
      url = "$base/stable/$artifactName"
    }
  }
}
$manifestPath = Join-Path $channel 'latest.json'
$manifest | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $manifestPath -Encoding utf8NoBOM

$parsed = Get-Content -Raw -LiteralPath $manifestPath | ConvertFrom-Json
if ($parsed.version -ne $Version -or [string]::IsNullOrWhiteSpace($parsed.platforms.'windows-x86_64'.signature)) {
  throw 'Generated latest.json failed validation.'
}
Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $channel $artifactName), $manifestPath |
  Format-Table Path, Hash -AutoSize

if (-not [string]::IsNullOrWhiteSpace($env:KODEWORK_UPDATE_S3_URI)) {
  $aws = Get-Command aws -ErrorAction Stop
  & $aws.Source s3 sync $channel ($env:KODEWORK_UPDATE_S3_URI.TrimEnd('/') + '/stable') --cache-control 'public,max-age=300'
  if ($LASTEXITCODE -ne 0) { throw "aws s3 sync failed with exit code $LASTEXITCODE" }
  Write-Host "Update channel uploaded from $channel"
} elseif ($RequireUpload) {
  # Uploading is the whole point of the runbook when it is driven by automation.
  throw 'KODEWORK_UPDATE_S3_URI is not set; refusing to report success while the public update channel was not uploaded.'
} else {
  Write-Warning "KODEWORK_UPDATE_S3_URI is not set: the manifest was written to $channel but NOT uploaded. The public updater endpoint is unchanged."
}
