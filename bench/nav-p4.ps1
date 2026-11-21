# P4 (task T1.2): navigation keys in an fx window opened on the fixtures root,
# with real keyboard events so its FX_TRACE log records key -> frame latency.
# Each round: types "flat-100k" (the filter puts the cursor on it), Enter opens
# it (100,000 files), then Down x10, Shift+Down x5, PgDn x3, End, Up x5, Home,
# PgDn, Shift+PgDn, and Backspace goes back up. Called by `fx-bench nav`.
#
# It SENDS REAL KEYSTROKES to the foreground window for ~8 s: do not type meanwhile.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Rounds = 3, [int]$IntervalMs = 60)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class Nav {
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    const uint EXTENDED = 0x0001, KEYUP = 0x0002;
    const byte VK_BACK = 0x08, VK_RETURN = 0x0D, VK_SHIFT = 0x10, VK_MENU = 0x12,
        VK_PRIOR = 0x21, VK_NEXT = 0x22, VK_END = 0x23, VK_HOME = 0x24,
        VK_UP = 0x26, VK_DOWN = 0x28, VK_OEM_MINUS = 0xBD;

    static Stopwatch sw;
    static int slot;

    static bool Extended(byte vk) { return vk >= VK_PRIOR && vk <= VK_DOWN; }

    static void Press(byte vk, bool shift) {
        uint ext = Extended(vk) ? EXTENDED : 0;
        if (shift) keybd_event(VK_SHIFT, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, ext, UIntPtr.Zero);
        keybd_event(vk, 0, ext | KEYUP, UIntPtr.Zero);
        if (shift) keybd_event(VK_SHIFT, 0, KEYUP, UIntPtr.Zero);
    }

    // One key per interval; `pause` extra intervals afterwards.
    static void Key(byte vk, bool shift, int interval, int pause) {
        Press(vk, shift);
        slot += 1 + pause;
        while (sw.Elapsed.TotalMilliseconds < slot * interval) { Thread.Sleep(0); }
    }

    static byte Vk(char c) {
        if (c >= 'a' && c <= 'z') return (byte)(c - 'a' + 0x41);
        if (c >= '0' && c <= '9') return (byte)c;
        if (c == '-') return VK_OEM_MINUS;
        throw new ArgumentException("unsupported key: " + c);
    }

    public static string Run(IntPtr hwnd, int rounds, int interval) {
        // Windows only lets the foreground process change the foreground window;
        // a synthetic Alt press lifts that lock (fx has no menu, so it is harmless).
        for (int i = 0; i < 5 && GetForegroundWindow() != hwnd; i++) {
            Press(VK_MENU, false);
            SetForegroundWindow(hwnd);
            Thread.Sleep(100);
        }
        Thread.Sleep(200);
        if (GetForegroundWindow() != hwnd) throw new Exception("fx window is not in the foreground");
        sw = Stopwatch.StartNew();
        slot = 0;
        int open = 1500 / interval; // let 100k load and sort after Enter
        for (int r = 0; r < rounds; r++) {
            foreach (char c in "flat-100k") Key(Vk(c), false, interval, 0);
            Key(VK_RETURN, false, interval, open);
            for (int i = 0; i < 10; i++) Key(VK_DOWN, false, interval, 0);
            for (int i = 0; i < 5; i++) Key(VK_DOWN, true, interval, 0);
            for (int i = 0; i < 3; i++) Key(VK_NEXT, false, interval, 0);
            Key(VK_END, false, interval, 0);
            for (int i = 0; i < 5; i++) Key(VK_UP, false, interval, 0);
            Key(VK_HOME, false, interval, 0);
            Key(VK_NEXT, false, interval, 0);
            Key(VK_NEXT, true, interval, 0);
            Key(VK_BACK, false, interval, open / 2);
        }
        Thread.Sleep(300);
        return String.Format("{0} rounds", rounds);
    }
}
"@

$proc = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $proc.MainWindowHandle -eq 0; $i++) {
    Start-Sleep -Milliseconds 100
    $proc.Refresh()
}
if ($proc.MainWindowHandle -eq 0) { throw "fx window not found for process $ProcessId" }
[Nav]::Run($proc.MainWindowHandle, $Rounds, $IntervalMs)
