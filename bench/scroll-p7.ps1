# P7 (task T0.5): drives real mouse input into an fx window so its FX_TRACE frame
# log covers wheel scrolling from the start to the end of the list and dragging
# the scrollbar thumb end to end. Called by `fx-bench scroll`.
#
# It MOVES THE REAL MOUSE for ~15 s: do not touch the mouse while it runs.
param([Parameter(Mandatory = $true)][int]$ProcessId)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class P7 {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
    [DllImport("user32.dll")] static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] static extern bool GetClientRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] static extern bool ClientToScreen(IntPtr h, ref POINT p);
    [DllImport("user32.dll")] static extern uint GetDpiForWindow(IntPtr h);
    [DllImport("user32.dll")] static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] static extern void mouse_event(uint flags, int dx, int dy, int data, UIntPtr extra);
    const uint LEFTDOWN = 0x0002, LEFTUP = 0x0004, WHEEL = 0x0800;

    static void Wait(Stopwatch sw, double untilMs) {
        while (sw.Elapsed.TotalMilliseconds < untilMs) { Thread.Sleep(0); }
    }

    // The thumb is dragged from the top to the bottom and back over dragMs each way,
    // then wheelEvents events of wheelDelta are sent every stepMs.
    public static string Run(IntPtr hwnd, int wheelEvents, int wheelDelta, double stepMs, double dragMs) {
        SetProcessDPIAware();
        SetForegroundWindow(hwnd);
        Thread.Sleep(400);
        RECT rc; GetClientRect(hwnd, out rc);
        POINT origin = new POINT(); ClientToScreen(hwnd, ref origin);
        double s = GetDpiForWindow(hwnd) / 96.0;
        int w = rc.R - rc.L, h = rc.B - rc.T;

        // 1) Drag the thumb from the top to the bottom and back. With the list at
        //    the top, the 24 px thumb sits right below the path bar (40 px), the
        //    divider (1 px) and the column header (32 px, standard density).
        int x = origin.X + w - (int)(6 * s);
        int yTop = origin.Y + (int)(85 * s);
        int yBottom = origin.Y + h - (int)(12 * s);
        SetCursorPos(x, yTop);
        Thread.Sleep(100);
        mouse_event(LEFTDOWN, 0, 0, 0, UIntPtr.Zero);
        int steps = (int)(dragMs / stepMs);
        var sw = Stopwatch.StartNew();
        for (int i = 1; i <= steps; i++) {
            SetCursorPos(x, yTop + (yBottom - yTop) * i / steps);
            Wait(sw, i * stepMs);
        }
        sw.Restart();
        for (int i = 1; i <= steps; i++) {
            SetCursorPos(x, yBottom + (yTop - yBottom) * i / steps);
            Wait(sw, i * stepMs);
        }
        mouse_event(LEFTUP, 0, 0, 0, UIntPtr.Zero);
        Thread.Sleep(600);

        // 2) Wheel, from the top to the end of the list.
        SetCursorPos(origin.X + w / 2, origin.Y + h / 2);
        Thread.Sleep(100);
        sw.Restart();
        for (int i = 0; i < wheelEvents; i++) {
            mouse_event(WHEEL, 0, 0, wheelDelta, UIntPtr.Zero);
            Wait(sw, (i + 1) * stepMs);
        }
        Thread.Sleep(400);
        return String.Format("client {0}x{1} at {2},{3}, scale {4}", w, h, origin.X, origin.Y, s);
    }
}
"@

$proc = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $proc.MainWindowHandle -eq 0; $i++) {
    Start-Sleep -Milliseconds 100
    $proc.Refresh()
}
if ($proc.MainWindowHandle -eq 0) { throw "fx window not found for process $ProcessId" }
# 2,600 events of 20 notches each, one every 4 ms (~10 s) reach the end of a
# 100k-row list at 32 px rows (one notch = 3 lines); dragging takes 3 s each way.
[P7]::Run($proc.MainWindowHandle, 2600, -2400, 4.0, 3000.0)
