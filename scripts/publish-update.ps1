param(
  [Parameter(Mandatory = $true)][string]$Version,
  [string]$BaseUrl = $env:KODEWORK_UPDATE_BASE_URL,
  [string]$Destination = (Join-Path $PSScriptRoot '..\release-channel'),
  [string]$Notes = 'Stability, performance and security improvements.'
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($BaseUrl)) { throw 'BaseUrl or KODEWORK_UPDATE_BASE_URL is required.' }
$msiDirectory = Join-Path $root 'target\release\bundle\msi'
$msi = Get-ChildItem -LiteralPath $msiDirectory -Filter "*_${Version}_x64_en-US.msi" | Select-Object -First 1
if (-not $msi) { throw "MSI for version $Version was not found in $msiDirectory" }
$signaturePath = $msi.FullName + '.sig'
if (-not (Test-Path -LiteralPath $signaturePath)) { throw "Updater signature missing: $signaturePath" }

$signature = [System.IO.File]::ReadAllText($signaturePath).Trim()
if ([string]::IsNullOrWhiteSpace($signature)) { throw 'Updater signature is empty.' }
$channel = Join-Path $Destination 'stable'
New-Item -ItemType Directory -Force -Path $channel | Out-Null
$artifactName = "kodework-windows-$Version-x86_64.msi"
Copy-Item -LiteralPath $msi.FullName -Destination (Join-Path $channel $artifactName) -Force
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
  & $aws.Source s3 sync $channel ($env:KODEWORK_UPDATE_S3_URI.TrimEnd('/') + '/stable') --delete --cache-control 'public,max-age=300'
  if ($LASTEXITCODE -ne 0) { throw "aws s3 sync failed with exit code $LASTEXITCODE" }
}
Write-Host "Update channel prepared at $channel"
