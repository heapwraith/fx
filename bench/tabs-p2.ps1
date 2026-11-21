# P2 (b) and P4 for tabs (task T1.4): fx opened on flat-100k (100,000 files).
# Opens 10 tabs with Ctrl+T (each lists the same folder again), switches with
# Ctrl+Tab x10 and Ctrl+Shift+Tab x5, then closes 9 tabs with Ctrl+W. The
# FX_TRACE log records Ctrl+T -> first frame with rows (tab_open_ms) and
# key -> frame for the tab keys. Called by `fx-bench tabs`.
#
# It SENDS REAL KEYSTROKES to the foreground window for ~10 s: do not type meanwhile.
# -Rounds is accepted (fx-bench passes it to every key script) but unused.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Rounds = 1, [int]$Tabs = 10, [int]$IntervalMs = 60)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class TabKeys {
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    const uint KEYUP = 0x0002;
    const byte VK_TAB = 0x09, VK_SHIFT = 0x10, VK_CONTROL = 0x11, VK_MENU = 0x12, VK_T = 0x54, VK_W = 0x57;

    static Stopwatch sw;
    static int slot;

    static void Press(byte vk, bool shift) {
        keybd_event(VK_CONTROL, 0, 0, UIntPtr.Zero);
        if (shift) keybd_event(VK_SHIFT, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, KEYUP, UIntPtr.Zero);
        if (shift) keybd_event(VK_SHIFT, 0, KEYUP, UIntPtr.Zero);
        keybd_event(VK_CONTROL, 0, KEYUP, UIntPtr.Zero);
    }

    // One Ctrl+key per interval; `pause` extra intervals afterwards.
    static void Key(byte vk, bool shift, int interval, int pause) {
        Press(vk, shift);
        slot += 1 + pause;
        while (sw.Elapsed.TotalMilliseconds < slot * interval) { Thread.Sleep(0); }
    }

    public static string Run(IntPtr hwnd, int tabs, int interval) {
        // Windows only lets the foreground process change the foreground window;
        // a synthetic Alt press lifts that lock (fx has no menu, so it is harmless).
        for (int i = 0; i < 5 && GetForegroundWindow() != hwnd; i++) {
            keybd_event(VK_MENU, 0, 0, UIntPtr.Zero);
            keybd_event(VK_MENU, 0, KEYUP, UIntPtr.Zero);
            SetForegroundWindow(hwnd);
            Thread.Sleep(100);
        }
        Thread.Sleep(200);
        if (GetForegroundWindow() != hwnd) throw new Exception("fx window is not in the foreground");
        sw = Stopwatch.StartNew();
        slot = 0;
        for (int i = 0; i < tabs; i++) Key(VK_T, false, interval, 600 / interval);
        for (int i = 0; i < 10; i++) Key(VK_TAB, false, interval, 0);
        for (int i = 0; i < 5; i++) Key(VK_TAB, true, interval, 0);
        for (int i = 0; i < tabs - 1; i++) Key(VK_W, false, interval, 1);
        Thread.Sleep(300);
        return String.Format("{0} tabs", tabs);
    }
}
"@

$proc = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $proc.MainWindowHandle -eq 0; $i++) {
    Start-Sleep -Milliseconds 100
    $proc.Refresh()
}
if ($proc.MainWindowHandle -eq 0) { throw "fx window not found for process $ProcessId" }
[TabKeys]::Run($proc.MainWindowHandle, $Tabs, $IntervalMs)
