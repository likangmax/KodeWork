$ErrorActionPreference = "Continue"
$exe = "D:\OneDrive\AAA_KK\MYCODE\redock-windows\target\release\kodework-tauri.exe"
if (-not (Test-Path $exe)) { Write-Output "release exe missing"; exit 1 }
$times = @()
$rssSamples = @()
for ($i = 1; $i -le 10; $i++) {
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $exe -PassThru
    $deadline = (Get-Date).AddSeconds(15)
    while (-not $p.HasExited) {
        $p.Refresh()
        if ($p.MainWindowHandle -ne 0) { break }
        if ((Get-Date) -gt $deadline) { break }
        Start-Sleep -Milliseconds 100
    }
    $sw.Stop()
    $p.Refresh()
    $rss = [Math]::Round($p.WorkingSet64 / 1MB, 1)
    $times += $sw.ElapsedMilliseconds
    $rssSamples += $rss
    Write-Output "run $i : $($sw.ElapsedMilliseconds) ms to window, RSS $rss MB"
    Start-Sleep -Seconds 3
    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    Start-Sleep -Seconds 1
}
$sorted = $times | Sort-Object
$p50 = $sorted[[Math]::Floor($sorted.Count * 0.5)]
$p95 = $sorted[[Math]::Min($sorted.Count - 1, [Math]::Floor($sorted.Count * 0.95))]
$avgRss = [Math]::Round(($rssSamples | Measure-Object -Average).Average, 1)
Write-Output "=== cold start p50: $p50 ms, p95: $p95 ms ==="
Write-Output "=== idle RSS avg: $avgRss MB (samples: $($rssSamples -join ", ")) ==="