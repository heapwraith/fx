# Drives an fx window for visual checks (T1.20-T1.24): a list of steps
# separated by ';', each one of
#   key:ctrl+shift+p   press a key combination (modifiers ctrl shift alt win; keys a-z 0-9 f1-f12 enter esc
#                      space tab up down left right home end pgup pgdn back del)
#   type:texto         types any text the keyboard layout can (via VkKeyScanW); no ';'
#   click:x,y          left click at client coordinates (px)
#   rclick:x,y         right click
#   move:x,y           moves the pointer
#   wait:ms            sleeps
#   shot:C:\out.png    saves a screenshot of the window
# It SENDS REAL INPUT to the fx window: do not use mouse or keyboard meanwhile.
param(
    [Parameter(Mandatory = $true)][int]$ProcessId,
    [Parameter(Mandatory = $true)][string]$Steps
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class Drv {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref POINT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int dx, int dy, int d, UIntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte s, uint f, UIntPtr e);
  [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern short VkKeyScanW(char c);
  [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int a, out RECT r, int size);
}
"@
[Drv]::SetProcessDPIAware() | Out-Null
$p = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
$h = $p.MainWindowHandle
function Focus {
    for ($i = 0; $i -lt 5 -and [Drv]::GetForegroundWindow() -ne $h; $i++) {
        [Drv]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [Drv]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
        [Drv]::SetForegroundWindow($h) | Out-Null; Start-Sleep -Milliseconds 150
    }
}
$vk = @{
    enter = 0x0D; esc = 0x1B; space = 0x20; tab = 0x09; up = 0x26; down = 0x28; left = 0x25; right = 0x27
    home = 0x24; end = 0x23; pgup = 0x21; pgdn = 0x22; back = 0x08; del = 0x2E; '-' = 0xBD; '.' = 0xBE
}
$mods = @{ ctrl = 0x11; shift = 0x10; alt = 0x12; win = 0x5B }
function Vk([string]$k) {
    if ($vk.ContainsKey($k)) { return [byte]$vk[$k] }
    if ($k -match '^f(\d+)$') { return [byte](0x6F + [int]$Matches[1]) }
    if ($k.Length -eq 1) { return [byte][char]$k.ToUpper() }
    throw "unknown key: $k"
}
function Press([string]$combo) {
    $parts = $combo.ToLower().Split('+')
    $held = @($parts | Select-Object -SkipLast 1 | ForEach-Object { [byte]$mods[$_] })
    $key = Vk $parts[-1]
    foreach ($m in $held) { [Drv]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Drv]::keybd_event($key, 0, 0, [UIntPtr]::Zero); [Drv]::keybd_event($key, 0, 2, [UIntPtr]::Zero)
    [array]::Reverse($held)
    foreach ($m in $held) { [Drv]::keybd_event($m, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 40
}
# Any character the current keyboard layout can type (':' '>' 'ç'...).
function TypeChar([char]$c) {
    if ($c -eq ' ') { Press 'space'; return }
    $scan = [Drv]::VkKeyScanW($c)
    if ($scan -eq -1) { throw "cannot type: $c" }
    $vkc = [byte]($scan -band 0xFF); $state = ($scan -shr 8) -band 0xFF
    $held = @()
    if ($state -band 1) { $held += [byte]0x10 }
    if ($state -band 2) { $held += [byte]0x11 }
    if ($state -band 4) { $held += [byte]0x12 }
    foreach ($m in $held) { [Drv]::keybd_event($m, 0, 0, [UIntPtr]::Zero) }
    [Drv]::keybd_event($vkc, 0, 0, [UIntPtr]::Zero); [Drv]::keybd_event($vkc, 0, 2, [UIntPtr]::Zero)
    [array]::Reverse($held)
    foreach ($m in $held) { [Drv]::keybd_event($m, 0, 2, [UIntPtr]::Zero) }
    Start-Sleep -Milliseconds 40
}
function ToScreen([string]$xy) {
    $a = $xy.Split(','); $o = New-Object Drv+POINT; $o.X = [int]$a[0]; $o.Y = [int]$a[1]
    [Drv]::ClientToScreen($h, [ref]$o) | Out-Null; return $o
}
Focus
foreach ($step in $Steps.Split(';')) {
    if (-not $step) { continue }
    $i = $step.IndexOf(':'); $op = $step.Substring(0, $i); $arg = $step.Substring($i + 1)
    switch ($op) {
        'key' { Press $arg }
        'type' { foreach ($c in $arg.ToCharArray()) { TypeChar $c } }
        'move' { $s = ToScreen $arg; [Drv]::SetCursorPos($s.X, $s.Y) | Out-Null; Start-Sleep -Milliseconds 80 }
        'click' {
            $s = ToScreen $arg; [Drv]::SetCursorPos($s.X, $s.Y) | Out-Null; Start-Sleep -Milliseconds 80
            [Drv]::mouse_event(2, 0, 0, 0, [UIntPtr]::Zero); [Drv]::mouse_event(4, 0, 0, 0, [UIntPtr]::Zero)
            Start-Sleep -Milliseconds 80
        }
        'rclick' {
            $s = ToScreen $arg; [Drv]::SetCursorPos($s.X, $s.Y) | Out-Null; Start-Sleep -Milliseconds 80
            [Drv]::mouse_event(8, 0, 0, 0, [UIntPtr]::Zero); [Drv]::mouse_event(16, 0, 0, 0, [UIntPtr]::Zero)
            Start-Sleep -Milliseconds 80
        }
        'wait' { Start-Sleep -Milliseconds ([int]$arg) }
        'shot' {
            $r = New-Object Drv+RECT
            [Drv]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) | Out-Null  # DWMWA_EXTENDED_FRAME_BOUNDS
            $b = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
            $g = [System.Drawing.Graphics]::FromImage($b)
            $g.CopyFromScreen($r.L, $r.T, 0, 0, $b.Size)
            $b.Save($arg); $g.Dispose(); $b.Dispose()
        }
        default { throw "unknown step: $step" }
    }
}
