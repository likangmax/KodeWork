# Installation, upgrade and removal acceptance for the Windows MSI.
#
# This is the evidence that "the package installs, an installed copy upgrades, and
# the result starts" - the layer that MSI-table inspection cannot cover. It needs
# administrator rights and it installs software, so it is meant for a disposable
# Windows machine (a CI runner or a throwaway VM), never for a workstation that
# has a real Kodework installation.
#
# Usage:
#   scripts/verify-install-upgrade.ps1 -NewMsi <path> [-PreviousMsi <path>] [-SkipLaunchCheck]
#
# Checks, in order:
#   1. the machine starts without a Kodework product installed;
#   2. the previous release installs and registers;
#   3. the new package installs over it and is treated as a major upgrade: the
#      previous ProductCode disappears instead of remaining side by side;
#   4. the installed version matches the package, and the executable is present;
#   5. the installed application starts and creates a window;
#   6. uninstalling removes the product.
param(
  [Parameter(Mandatory = $true)][string]$NewMsi,
  [string]$PreviousMsi,
  [switch]$SkipLaunchCheck,
  [int]$LaunchTimeoutSeconds = 90
)
$ErrorActionPreference = 'Stop'

$results = [System.Collections.Generic.List[string]]::new()
function Check([string]$name, [bool]$ok, [string]$detail) {
  $line = "{0}  {1,-52} {2}" -f ($(if ($ok) { 'PASS' } else { 'FAIL' })), $name, $detail
  $results.Add($line)
  Write-Output $line
}

$principal = New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
  throw 'This acceptance script installs a per-machine MSI and requires an elevated shell.'
}
if (-not (Test-Path -LiteralPath $NewMsi)) { throw "new package not found: $NewMsi" }
if ($PreviousMsi -and -not (Test-Path -LiteralPath $PreviousMsi)) { throw "previous package not found: $PreviousMsi" }

$uninstallRoots = @(
  'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*'
)
function Get-KodeworkProducts {
  Get-ItemProperty $uninstallRoots -ErrorAction SilentlyContinue |
    Where-Object { $_.DisplayName -match 'Kodework' } |
    ForEach-Object { [pscustomobject]@{ ProductCode = $_.PSChildName; Version = $_.DisplayVersion; Location = $_.InstallLocation } }
}
function Install-Msi([string]$path, [string]$tag) {
  $log = Join-Path $env:TEMP "kodework-$tag-install.log"
  $arguments = @('/i', "`"$path`"", '/qn', '/norestart', '/l*v', "`"$log`"")
  $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList $arguments -Wait -PassThru
  # 0 = success; 3010 = success, reboot required.
  if ($process.ExitCode -ne 0 -and $process.ExitCode -ne 3010) {
    Write-Output "--- tail of $log ---"
    Get-Content -LiteralPath $log -Tail 25 -ErrorAction SilentlyContinue | ForEach-Object { "    $_" }
    throw "msiexec /i $tag failed with exit code $($process.ExitCode)"
  }
  return $process.ExitCode
}
function Uninstall-Product([string]$productCode, [string]$tag) {
  $log = Join-Path $env:TEMP "kodework-$tag-remove.log"
  $arguments = @('/x', $productCode, '/qn', '/norestart', '/l*v', "`"$log`"")
  $process = Start-Process -FilePath 'msiexec.exe' -ArgumentList $arguments -Wait -PassThru
  return $process.ExitCode
}

$existing = @(Get-KodeworkProducts)
Check 'machine starts without a Kodework install' ($existing.Count -eq 0) $(if ($existing.Count -eq 0) { 'clean' } else { "already installed: " + (($existing | ForEach-Object { "$($_.Version) $($_.ProductCode)" }) -join ', ') })
if ($existing.Count -ne 0) {
  throw 'Refusing to run: this machine already has a Kodework installation. Use a disposable machine.'
}

$previousProductCode = $null
try {
  # ---- 2. the previous release installs --------------------------------
  if ($PreviousMsi) {
    Install-Msi $PreviousMsi 'previous' | Out-Null
    $afterPrevious = @(Get-KodeworkProducts)
    $previousProductCode = $afterPrevious | Select-Object -First 1
    Check 'previous release installs and registers' ($afterPrevious.Count -eq 1) "$($previousProductCode.Version) $($previousProductCode.ProductCode)"
  } else {
    Check 'previous release installs and registers' $true 'skipped: no -PreviousMsi supplied'
  }

  # ---- 3. the new package upgrades it -----------------------------------
  Install-Msi $NewMsi 'new' | Out-Null
  $afterNew = @(Get-KodeworkProducts)
  Check 'exactly one Kodework product is registered' ($afterNew.Count -eq 1) (($afterNew | ForEach-Object { "$($_.Version) $($_.ProductCode)" }) -join ', ')
  if ($previousProductCode) {
    $leftBehind = $afterNew | Where-Object { $_.ProductCode -eq $previousProductCode.ProductCode }
    Check 'the previous version was removed, not left side by side' ($null -eq $leftBehind) "previous=$($previousProductCode.ProductCode)"
  }
  $installed = $afterNew | Select-Object -First 1

  # ---- 4. version and files ---------------------------------------------
  $expectedVersion = $null
  try {
    $installer = New-Object -ComObject WindowsInstaller.Installer
    $database = $installer.GetType().InvokeMember('OpenDatabase', 'InvokeMethod', $null, $installer, @((Resolve-Path $NewMsi).Path, 0))
    $view = $database.GetType().InvokeMember('OpenView', 'InvokeMethod', $null, $database, @("SELECT ``Value`` FROM ``Property`` WHERE ``Property``='ProductVersion'"))
    $view.GetType().InvokeMember('Execute', 'InvokeMethod', $null, $view, $null) | Out-Null
    $record = $view.GetType().InvokeMember('Fetch', 'InvokeMethod', $null, $view, $null)
    if ($record) { $expectedVersion = $record.GetType().InvokeMember('StringData', 'GetProperty', $null, $record, @(1)) }
  } catch {
    Write-Warning "could not read the package version: $($_.Exception.Message)"
  }
  Check 'installed version matches the package' ($expectedVersion -and $installed.Version -eq $expectedVersion) "package=$expectedVersion installed=$($installed.Version)"

  $executable = Get-ChildItem -Path $installed.Location -Filter 'kodework-tauri.exe' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $executable) {
    $executable = Get-ChildItem -Path "${env:ProgramFiles}\Kodework*" -Filter 'kodework-tauri.exe' -Recurse -ErrorAction SilentlyContinue | Select-Object -First 1
  }
  Check 'the installed executable is present' ($null -ne $executable) $(if ($executable) { $executable.FullName } else { "not found under $($installed.Location)" })

  # ---- 5. the installed application starts ------------------------------
  if ($SkipLaunchCheck -or -not $executable) {
    Check 'the installed application starts' $false 'skipped (no executable, or -SkipLaunchCheck)'
  } else {
    $process = Start-Process -FilePath $executable.FullName -PassThru
    $deadline = (Get-Date).AddSeconds($LaunchTimeoutSeconds)
    $windowed = $false
    $exited = $false
    while ((Get-Date) -lt $deadline) {
      $process.Refresh()
      if ($process.HasExited) { $exited = $true; break }
      if ($process.MainWindowHandle -ne 0) { $windowed = $true; break }
      Start-Sleep -Milliseconds 250
    }
    $detail = if ($windowed) { 'window created' } elseif ($exited) { "exited early with code $($process.ExitCode)" } else { "no window within ${LaunchTimeoutSeconds}s" }
    Check 'the installed application starts' $windowed $detail
    if (-not $process.HasExited) { Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue }
  }
} finally {
  # ---- 6. removal, and a clean machine for the next run ------------------
  foreach ($product in @(Get-KodeworkProducts)) {
    $code = Uninstall-Product $product.ProductCode 'cleanup'
    Check "uninstall removes $($product.Version)" ($code -eq 0 -or $code -eq 3010) "msiexec /x exit $code"
  }
  $left = @(Get-KodeworkProducts)
  Check 'no Kodework product remains' ($left.Count -eq 0) (($left | ForEach-Object { $_.ProductCode }) -join ', ')
  $failed = ($results | Where-Object { $_ -like 'FAIL*' }).Count
  Write-Output ''
  Write-Output "===== install acceptance: $($results.Count - $failed)/$($results.Count) checks passed ====="
  if ($failed -gt 0) { exit 1 }
}
