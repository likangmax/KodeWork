# Builds the pinned, redistributable Tailscale CLI and userspace daemon used
# by Kodework's embedded mode. Outputs follow Tauri's externalBin target
# naming convention and are never committed as source artifacts.
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$source = Join-Path $root 'references\tailscale'
$output = Join-Path $root 'src-tauri\binaries'
$tag = 'v1.102.2'
$commit = 'eb67e5dcbe145d63e1128b9b4b630f8a82da101f'
$target = 'x86_64-pc-windows-msvc'
$go = 'C:\Program Files\Go\bin\go.exe'
$userPipePatch = Join-Path $root 'patches\tailscale-embedded-user-pipe.patch'

if (-not (Test-Path $go -PathType Leaf)) {
  $go = (Get-Command go -ErrorAction Stop).Source
}
if (-not (Test-Path $source -PathType Container)) {
  New-Item -ItemType Directory -Force -Path (Split-Path -Parent $source) | Out-Null
  & git clone --filter=blob:none --branch $tag --depth 1 https://github.com/tailscale/tailscale.git $source
  if ($LASTEXITCODE -ne 0) { throw "failed to clone Tailscale $tag" }
}
$actual = (& git -C $source rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $actual -ne $commit) {
  throw "Tailscale source must be pinned to $tag ($commit), found $actual"
}
# Upstream's service pipe declares Builtin Administrators as its owner. A
# per-user embedded daemon cannot assign that owner and exits before opening
# LocalAPI. Replace the exact version-pinned declaration so line endings and
# Git patch heuristics cannot make clean CI clones behave differently.
$pipeSource = Join-Path $source 'safesocket\pipe_windows.go'
$originalSddl = 'var windowsSDDL = "O:BAG:BAD:PAI(A;OICI;GWGR;;;BU)(A;OICI;GWGR;;;SY)"'
$patchedSddl = 'var windowsSDDL = "D:PAI(A;OICI;GWGR;;;BU)(A;OICI;GWGR;;;SY)"'
$pipeText = [System.IO.File]::ReadAllText($pipeSource)
if ($pipeText.Contains($originalSddl)) {
  $pipeText = $pipeText.Replace($originalSddl, $patchedSddl)
  [System.IO.File]::WriteAllText($pipeSource, $pipeText, [System.Text.UTF8Encoding]::new($false))
} elseif (-not $pipeText.Contains($patchedSddl)) {
  throw "Tailscale source differs from the audited embedded pipe patch at $userPipePatch"
}

New-Item -ItemType Directory -Force -Path $output | Out-Null
$previousCgo = $env:CGO_ENABLED
$previousGoos = $env:GOOS
$previousGoarch = $env:GOARCH
try {
  $env:CGO_ENABLED = '0'
  $env:GOOS = 'windows'
  $env:GOARCH = 'amd64'
  Push-Location $source
  $versionStamp = '1.102.2-kodework.1'
  $linkerFlags = "-s -w -X tailscale.com/version.longStamp=$versionStamp -X tailscale.com/version.shortStamp=1.102.2 -X tailscale.com/version.gitCommitStamp=$commit"
  $common = @('build', '-trimpath', '-buildvcs=false', "-ldflags=$linkerFlags")
  $cliOutput = Join-Path $output "tailscale-$target.exe"
  $daemonOutput = Join-Path $output "tailscaled-$target.exe"
  & $go @common '-o' $cliOutput './cmd/tailscale'
  if ($LASTEXITCODE -ne 0) { throw "tailscale.exe build failed" }
  & $go @common '-o' $daemonOutput './cmd/tailscaled'
  if ($LASTEXITCODE -ne 0) { throw "tailscaled.exe build failed" }
  Copy-Item -LiteralPath (Join-Path $source 'LICENSE') -Destination (Join-Path $output 'TAILSCALE-LICENSE.txt') -Force
  & $cliOutput version
  if ($LASTEXITCODE -ne 0) { throw "built tailscale.exe failed its version smoke test" }
} finally {
  if ((Get-Location).Path -eq $source) { Pop-Location }
  $env:CGO_ENABLED = $previousCgo
  $env:GOOS = $previousGoos
  $env:GOARCH = $previousGoarch
}

Write-Host "Pinned Tailscale sidecars prepared in $output"
