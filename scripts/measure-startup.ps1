param(
  [string]$Executable,
  [ValidateRange(1, 100)][int]$Runs = 10,
  [ValidateRange(1, 60)][int]$WindowTimeoutSeconds = 15,
  [ValidateRange(0, 30)][int]$IdleSampleSeconds = 3
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
if ([string]::IsNullOrWhiteSpace($Executable)) {
  $Executable = Join-Path $root 'target\release\kodework-tauri.exe'
} elseif (-not [System.IO.Path]::IsPathRooted($Executable)) {
  $Executable = Join-Path $root $Executable
}
$Executable = [System.IO.Path]::GetFullPath($Executable)

if (-not (Test-Path -LiteralPath $Executable -PathType Leaf)) {
  throw "Executable not found: $Executable. Build a release binary first or pass -Executable."
}

function Get-Percentile([long[]]$Values, [double]$Percentile) {
  $sorted = @($Values | Sort-Object)
  $index = [Math]::Max(0, [Math]::Ceiling($Percentile * $sorted.Count) - 1)
  return $sorted[$index]
}

$times = [System.Collections.Generic.List[long]]::new()
$rssSamples = [System.Collections.Generic.List[double]]::new()

for ($i = 1; $i -le $Runs; $i++) {
  $process = $null
  try {
    $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Executable -PassThru
    $deadline = (Get-Date).AddSeconds($WindowTimeoutSeconds)

    while ($true) {
      $process.Refresh()
      if ($process.HasExited) {
        throw "Run $i exited before creating a window (exit code $($process.ExitCode))."
      }
      if ($process.MainWindowHandle -ne 0) { break }
      if ((Get-Date) -gt $deadline) {
        throw "Run $i did not create a window within $WindowTimeoutSeconds seconds."
      }
      Start-Sleep -Milliseconds 100
    }

    $stopwatch.Stop()
    if ($IdleSampleSeconds -gt 0) { Start-Sleep -Seconds $IdleSampleSeconds }
    $process.Refresh()

    $rssMb = [Math]::Round($process.WorkingSet64 / 1MB, 1)
    $times.Add($stopwatch.ElapsedMilliseconds)
    $rssSamples.Add($rssMb)
    Write-Output "run $i : $($stopwatch.ElapsedMilliseconds) ms to window, RSS $rssMb MB"
  } finally {
    if ($null -ne $process -and -not $process.HasExited) {
      Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
      [void]$process.WaitForExit(5000)
    }
  }
}

$p50 = Get-Percentile $times.ToArray() 0.50
$p95 = Get-Percentile $times.ToArray() 0.95
$avgRss = [Math]::Round(($rssSamples | Measure-Object -Average).Average, 1)

Write-Output "=== cold start p50: $p50 ms, p95: $p95 ms ==="
Write-Output "=== idle RSS avg: $avgRss MB (samples: $($rssSamples -join ', ')) ==="
