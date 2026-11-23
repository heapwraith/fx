# T1.16: opens the shell context menu with right clicks on 20 rows of an fx
# window and closes each with Escape; fx's FX_TRACE log records click -> menu
# on screen (menu_ready_ms). Called by `fx-bench menu`.
#
# It MOVES THE MOUSE, CLICKS and SENDS KEYS for ~20 s: do not use the PC meanwhile.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Rounds = 1, [int]$Menus = 20)
$ErrorActionPreference = 'Stop'
Add-Type @"
using System;using System.Runtime.InteropServices;using System.Threading;
public static class Menu {
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint f, int x, int y, int d, UIntPtr e);
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte s, uint f, UIntPtr e);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
    const uint RIGHTDOWN = 0x0008, RIGHTUP = 0x0010;

    public static string Run(IntPtr hwnd, int menus) {
        SetProcessDPIAware();
        for (int i = 0; i < 5 && GetForegroundWindow() != hwnd; i++) {
            keybd_event(0x12, 0, 0, UIntPtr.Zero); keybd_event(0x12, 0, 2, UIntPtr.Zero);
            SetForegroundWindow(hwnd); Thread.Sleep(150);
        }
        if (GetForegroundWindow() != hwnd) throw new Exception("fx window is not in the foreground");
        POINT o = new POINT(); ClientToScreen(hwnd, ref o);
        double s = GetDpiForWindow(hwnd) / 96.0;
        // Rows start below the tab strip (40), path bar (48) and column header
        // (33): row k is centered at 138 + 32k; x past the sidebar (240).
        for (int k = 0; k < menus; k++) {
            int x = o.X + (int)(400 * s), y = o.Y + (int)((138 + 32 * (k % 14)) * s);
            SetCursorPos(x, y); Thread.Sleep(80);
            mouse_event(RIGHTDOWN, 0, 0, 0, UIntPtr.Zero); mouse_event(RIGHTUP, 0, 0, 0, UIntPtr.Zero);
            Thread.Sleep(700);
            keybd_event(0x1B, 0, 0, UIntPtr.Zero); keybd_event(0x1B, 0, 2, UIntPtr.Zero); // Escape
            Thread.Sleep(300);
        }
        return menus + " menus";
    }
}
"@
$proc = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $proc.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $proc.Refresh() }
[Menu]::Run($proc.MainWindowHandle, $Menus)
