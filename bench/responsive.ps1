# Task T0.5: checks that an fx window keeps responding while a slow folder loads.
# Every ~16 ms it moves and resizes the window (SetWindowPos) and measures how long
# the window takes to answer a WM_NULL message (SendMessageTimeout, 1 s limit).
# Called by `fx-bench responsive`.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$Seconds = 10)
$ErrorActionPreference = 'Stop'

Add-Type -TypeDefinition @"
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;
using System.Threading;

public static class Resp {
    [DllImport("user32.dll")] static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int cx, int cy, uint flags);
    [DllImport("user32.dll")] static extern IntPtr SendMessageTimeout(IntPtr h, uint msg, UIntPtr w, IntPtr l, uint flags, uint timeout, out UIntPtr result);
    const uint SWP_NOZORDER = 0x0004, SWP_NOACTIVATE = 0x0010, SMTO_ABORTIFHUNG = 0x0002, WM_NULL = 0;

    // Returns "samples max_ms p95_ms timeouts".
    public static string Run(IntPtr hwnd, int seconds) {
        var lat = new System.Collections.Generic.List<double>();
        int timeouts = 0;
        var total = Stopwatch.StartNew();
        int i = 0;
        while (total.Elapsed.TotalSeconds < seconds) {
            int dx = (i % 20) * 4, grow = (i % 2) * 60;
            SetWindowPos(hwnd, IntPtr.Zero, 300 + dx, 200 + dx, 900 + grow, 640 + grow, SWP_NOZORDER | SWP_NOACTIVATE);
            var sw = Stopwatch.StartNew();
            UIntPtr r;
            if (SendMessageTimeout(hwnd, WM_NULL, UIntPtr.Zero, IntPtr.Zero, SMTO_ABORTIFHUNG, 1000, out r) == IntPtr.Zero) {
                timeouts++;
            }
            lat.Add(sw.Elapsed.TotalMilliseconds);
            i++;
            Thread.Sleep(15);
        }
        lat.Sort();
        double p95 = lat[Math.Max(0, (int)Math.Ceiling(lat.Count * 0.95) - 1)];
        return String.Format(System.Globalization.CultureInfo.InvariantCulture,
            "{0} {1:F3} {2:F3} {3}", lat.Count, lat[lat.Count - 1], p95, timeouts);
    }
}
"@

$proc = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 100 -and $proc.MainWindowHandle -eq 0; $i++) {
    Start-Sleep -Milliseconds 20
    $proc.Refresh()
}
if ($proc.MainWindowHandle -eq 0) { throw "fx window not found for process $ProcessId" }
[Resp]::Run($proc.MainWindowHandle, $Seconds)
