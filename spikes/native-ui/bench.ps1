# Runs the three release spikes in `--bench` mode, twice each for `animate` (spinning cube) and
# `idle` (still cube), and writes what they print to bench-results.txt next to this script.
#
#   cargo build --release -p spike-iced -p spike-egui -p spike-slint
#   powershell -File bench.ps1
#
# Each run opens a window for 14 seconds on its own and closes it. Nobody has to click. The
# first 4 seconds are warm-up; the `steady` numbers are the last 10.
$ErrorActionPreference = 'Continue'
$root = Join-Path $PSScriptRoot 'target\release'
$out = Join-Path $PSScriptRoot 'bench-results.txt'
'' | Set-Content $out
$gpu = Get-CimInstance Win32_VideoController |
  Select-Object Name, CurrentRefreshRate, CurrentHorizontalResolution, CurrentVerticalResolution, DriverVersion
($gpu | Format-List | Out-String) | Add-Content $out
foreach ($round in 1..2) {
  foreach ($mode in 'animate', 'idle') {
    foreach ($exe in 'spike-iced', 'spike-egui', 'spike-slint') {
      "--- round $round $exe $mode" | Add-Content $out
      $p = Start-Process -FilePath "$root\$exe.exe" -ArgumentList '--bench', $mode, '14' `
        -RedirectStandardOutput "$env:TEMP\spike-out.txt" -RedirectStandardError "$env:TEMP\spike-err.txt" -PassThru
      if (-not $p.WaitForExit(40000)) { $p.Kill(); 'TIMEOUT' | Add-Content $out }
      Get-Content "$env:TEMP\spike-out.txt" | Add-Content $out
      $err = Get-Content "$env:TEMP\spike-err.txt" -ErrorAction SilentlyContinue | Select-Object -First 5
      if ($err) { "stderr: $err" | Add-Content $out }
      Start-Sleep -Seconds 2
    }
  }
}
Get-Content $out
