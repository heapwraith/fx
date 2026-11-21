# T1.1: screenshot of an fx window as the user sees it (Mica included), with the
# 3rd row selected (click) and the pointer hovering the 5th. Moves the mouse.
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [Parameter(Mandatory = $true)][string]$Png,
    [Parameter(Mandatory = $true)][int]$RowHeight
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices; using System.Threading;
public static class Cap {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte s, uint f, UIntPtr e);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int size);
}
"@
[Cap]::SetProcessDPIAware() | Out-Null
$p = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
$h = $p.MainWindowHandle
for ($i = 0; $i -lt 5 -and [Cap]::GetForegroundWindow() -ne $h; $i++) {
    [Cap]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [Cap]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
    [Cap]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 150
}
$o = New-Object Cap+POINT; [Cap]::ClientToScreen($h, [ref]$o) | Out-Null
$c = New-Object Cap+RECT; [Cap]::GetClientRect($h, [ref]$c) | Out-Null
# Path bar 40 px + divider + column header (one row) + rows.
$top = $o.Y + 41 + $RowHeight
$x = $o.X + 200
[Cap]::SetCursorPos($x, $top + [int]($RowHeight * 2.5)) | Out-Null; Start-Sleep -Milliseconds 100
[Cap]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [Cap]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
[Cap]::SetCursorPos($x, $top + [int]($RowHeight * 4.5)) | Out-Null
Start-Sleep -Milliseconds 500
$r = New-Object Cap+RECT
[Cap]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) | Out-Null  # DWMWA_EXTENDED_FRAME_BOUNDS
$b = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
$g = [System.Drawing.Graphics]::FromImage($b)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $b.Size)
$b.Save($Png)
"{0}x{1}" -f ($r.R - $r.L), ($r.B - $r.T)
