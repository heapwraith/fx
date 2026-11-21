# P4 (task T1.3): path bar and history in an fx window opened on the fixtures
# root, with real keyboard events so its FX_TRACE log records key -> frame
# latency. Each round: Ctrl+L, End, Tab (completes "fx-fixtures\": the parent
# folder changes, so the next keys run while its listing loads), types "fla",
# erases it, types "de", Tab ("deep-tree\"), Enter (opens it), then Alt+Left,
# Alt+Right, Alt+Left. Called by `fx-bench path`.
#
# It SENDS REAL KEYSTROKES to the foreground window for ~10 s: do not type meanwhile.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Rounds = 3, [int]$IntervalMs = 60)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class PathKeys {
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    const uint EXTENDED = 0x0001, KEYUP = 0x0002;
    const byte VK_BACK = 0x08, VK_TAB = 0x09, VK_RETURN = 0x0D, VK_CONTROL = 0x11, VK_MENU = 0x12,
        VK_END = 0x23, VK_LEFT = 0x25, VK_RIGHT = 0x27;

    static Stopwatch sw;
    static int slot;

    static bool Extended(byte vk) { return vk >= 0x21 && vk <= 0x28; }

    static void Press(byte vk, byte modifier) {
        uint ext = Extended(vk) ? EXTENDED : 0;
        if (modifier != 0) keybd_event(modifier, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, ext, UIntPtr.Zero);
        keybd_event(vk, 0, ext | KEYUP, UIntPtr.Zero);
        if (modifier != 0) keybd_event(modifier, 0, KEYUP, UIntPtr.Zero);
    }

    // One key per interval; `pause` extra intervals afterwards.
    static void Key(byte vk, byte modifier, int interval, int pause) {
        Press(vk, modifier);
        slot += 1 + pause;
        while (sw.Elapsed.TotalMilliseconds < slot * interval) { Thread.Sleep(0); }
    }

    static void Type(string text, int interval) {
        foreach (char c in text) Key((byte)(c - 'a' + 0x41), 0, interval, 0);
    }

    public static string Run(IntPtr hwnd, int rounds, int interval) {
        // Windows only lets the foreground process change the foreground window;
        // a synthetic Alt press lifts that lock (fx has no menu, so it is harmless).
        for (int i = 0; i < 5 && GetForegroundWindow() != hwnd; i++) {
            Press(VK_MENU, 0);
            SetForegroundWindow(hwnd);
            Thread.Sleep(100);
        }
        Thread.Sleep(200);
        if (GetForegroundWindow() != hwnd) throw new Exception("fx window is not in the foreground");
        sw = Stopwatch.StartNew();
        slot = 0;
        int wait = 500 / interval;
        for (int r = 0; r < rounds; r++) {
            Key(0x4C, VK_CONTROL, interval, 0);   // Ctrl+L
            Key(VK_END, 0, interval, wait);       // parent (%TEMP%) listing loads
            Key(VK_TAB, 0, interval, 0);          // "fx-fixtures\"
            Type("fla", interval);
            for (int i = 0; i < 3; i++) Key(VK_BACK, 0, interval, 0);
            Type("de", interval);
            Key(VK_TAB, 0, interval, wait);       // "deep-tree\"
            Key(VK_RETURN, 0, interval, 2 * wait);
            Key(VK_LEFT, VK_MENU, interval, wait);
            Key(VK_RIGHT, VK_MENU, interval, wait);
            Key(VK_LEFT, VK_MENU, interval, wait);
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
[PathKeys]::Run($proc.MainWindowHandle, $Rounds, $IntervalMs)
