# P4 (task T0.6): types into an fx window with real keyboard events so its
# FX_TRACE log records key -> frame latency. Types "report-123" (10 keys, one
# every 50 ms), presses Escape, and repeats. Called by `fx-bench type`.
#
# It SENDS REAL KEYSTROKES to the foreground window for ~3 s: do not type meanwhile.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Rounds = 3, [int]$IntervalMs = 50)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class P4 {
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
    const uint KEYUP = 0x0002;
    const byte VK_ESCAPE = 0x1B, VK_OEM_MINUS = 0xBD, VK_MENU = 0x12;

    static void Press(byte vk) {
        keybd_event(vk, 0, 0, UIntPtr.Zero);
        keybd_event(vk, 0, KEYUP, UIntPtr.Zero);
    }

    static byte Vk(char c) {
        if (c >= 'a' && c <= 'z') return (byte)(c - 'a' + 0x41);
        if (c >= '0' && c <= '9') return (byte)c;
        if (c == '-') return VK_OEM_MINUS;
        throw new ArgumentException("unsupported key: " + c);
    }

    public static string Run(IntPtr hwnd, string text, int rounds, int intervalMs) {
        // Windows only lets the foreground process change the foreground window;
        // a synthetic Alt press lifts that lock (fx has no menu, so it is harmless).
        for (int i = 0; i < 5 && GetForegroundWindow() != hwnd; i++) {
            Press(VK_MENU);
            SetForegroundWindow(hwnd);
            Thread.Sleep(100);
        }
        Thread.Sleep(200);
        if (GetForegroundWindow() != hwnd) throw new Exception("fx window is not in the foreground");
        var sw = Stopwatch.StartNew();
        int n = 0;
        for (int r = 0; r < rounds; r++) {
            foreach (char c in text) {
                Press(Vk(c));
                n++;
                while (sw.Elapsed.TotalMilliseconds < n * intervalMs) { Thread.Sleep(0); }
            }
            Press(VK_ESCAPE);
            n += 4; // pause ~200 ms between rounds
            while (sw.Elapsed.TotalMilliseconds < n * intervalMs) { Thread.Sleep(0); }
        }
        Thread.Sleep(300);
        return String.Format("{0} rounds of \"{1}\"", rounds, text);
    }
}
"@

$proc = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $proc.MainWindowHandle -eq 0; $i++) {
    Start-Sleep -Milliseconds 100
    $proc.Refresh()
}
if ($proc.MainWindowHandle -eq 0) { throw "fx window not found for process $ProcessId" }
[P4]::Run($proc.MainWindowHandle, 'report-123', $Rounds, $IntervalMs)
