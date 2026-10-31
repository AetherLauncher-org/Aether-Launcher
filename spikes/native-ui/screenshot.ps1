# Starts a spike, waits for the window to settle, and saves a picture of that window only
# (PrintWindow on its handle, not a screen grab) to screenshots\<name>.png. Then closes it.
#
#   powershell -File screenshot.ps1 spike-iced
param([Parameter(Mandatory)][string]$Name)

Add-Type -AssemblyName System.Drawing
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr h, IntPtr dc, uint flags);
}
'@

$dir = Join-Path $PSScriptRoot 'screenshots'
New-Item -ItemType Directory -Force $dir | Out-Null
$exe = Join-Path $PSScriptRoot "target\release\$Name.exe"
$p = Start-Process -FilePath $exe -PassThru
try {
  $deadline = (Get-Date).AddSeconds(20)
  while ((Get-Date) -lt $deadline -and $p.MainWindowHandle -eq 0) { Start-Sleep -Milliseconds 300; $p.Refresh() }
  Start-Sleep -Seconds 4
  $p.Refresh()
  $rect = New-Object Win+RECT
  [void][Win]::GetWindowRect($p.MainWindowHandle, [ref]$rect)
  $w = $rect.Right - $rect.Left; $h = $rect.Bottom - $rect.Top
  $bitmap = New-Object System.Drawing.Bitmap $w, $h
  $g = [System.Drawing.Graphics]::FromImage($bitmap)
  $dc = $g.GetHdc()
  # 2 = PW_RENDERFULLCONTENT: also captures GPU-composited windows
  $ok = [Win]::PrintWindow($p.MainWindowHandle, $dc, 2)
  $g.ReleaseHdc($dc); $g.Dispose()
  $path = Join-Path $dir "$Name.png"
  $bitmap.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  "saved $path ($w x $h, PrintWindow=$ok)"
} finally {
  if (-not $p.HasExited) { $p.Kill() }
}
