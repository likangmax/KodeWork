param(
  [ValidateRange(1, 168)][int]$Hours = 4,
  [switch]$IncludeLargeTransfer,
  [switch]$AllowSleepCycle
)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$resultDir = Join-Path $root 'test-results\soak'
New-Item -ItemType Directory -Force -Path $resultDir | Out-Null
$stamp = Get-Date -Format 'yyyyMMdd-HHmmss'
$log = Join-Path $resultDir "soak-$stamp.log"

function Invoke-Gate([string]$Name, [scriptblock]$Command) {
  "[$([DateTimeOffset]::Now.ToString('o'))] START $Name" | Tee-Object -FilePath $log -Append
  & $Command 2>&1 | Tee-Object -FilePath $log -Append
  if ($LASTEXITCODE -ne 0) { throw "$Name failed with exit code $LASTEXITCODE" }
  "[$([DateTimeOffset]::Now.ToString('o'))] PASS $Name" | Tee-Object -FilePath $log -Append
}

Push-Location $root
try {
  Invoke-Gate '20 concurrent terminal panes' { cargo test -p kodework-core --test session_manager supports_twenty_concurrent_terminal_panes_without_cross_talk -- --exact }
  Invoke-Gate 'network drop/reconnect state tests' { cargo test -p kodework-core --test session_manager reconnect -- --nocapture }
  Invoke-Gate 'transfer pause/resume/retry matrix' { cargo test -p kodework-sftp --test transfer_manager }
  if ($IncludeLargeTransfer) { Invoke-Gate '512 MiB streaming transfer' { cargo test -p kodework-sftp --test large_file --release -- --nocapture } }

  $deadline = [DateTimeOffset]::Now.AddHours($Hours)
  $iteration = 0
  while ([DateTimeOffset]::Now -lt $deadline) {
    $iteration++
    Invoke-Gate "stability iteration $iteration" { cargo test -p kodework-ssh --test fake_server; cargo test -p kodework-sftp --test sftp_roundtrip }
    if ($AllowSleepCycle) {
      Write-Warning 'The machine will suspend now. Ensure this runner is allowed to sleep and wake manually or by a configured wake timer.'
      rundll32.exe powrprof.dll,SetSuspendState Sleep
      Start-Sleep -Seconds 15
      Invoke-Gate "post-resume iteration $iteration" { cargo test -p kodework-core --test session_manager connect_reaches_ready_and_streams_events -- --exact }
    }
  }
} finally { Pop-Location }
Write-Host "Soak matrix completed. Log: $log"
