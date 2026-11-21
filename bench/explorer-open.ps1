# T0.10: time for File Explorer to show the first item of a folder, measured from
# `explorer.exe <folder>` until UI Automation finds a ListItem in the new window.
# Resolution is about the polling interval (~5-15 ms including UIA calls), so this
# is a comparison figure, not a frame-exact measurement. Opens and closes one
# Explorer window per run; does not touch files.
param([Parameter(Mandatory = $true)][string]$Folder, [int]$Runs = 10)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
Add-Type -TypeDefinition @"
using System; using System.Runtime.InteropServices;
public static class W {
  [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
}
"@
$UIA = [System.Windows.Automation.AutomationElement]
$Tree = [System.Windows.Automation.TreeScope]
$isListItem = New-Object System.Windows.Automation.PropertyCondition(
    $UIA::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem)
$isExplorer = New-Object System.Windows.Automation.PropertyCondition($UIA::ClassNameProperty, 'CabinetWClass')
$leaf = Split-Path -Leaf $Folder

function ExplorerWindows {
    @($UIA::RootElement.FindAll($Tree::Children, $isExplorer) | ForEach-Object { $_.Current.NativeWindowHandle })
}

$times = @()
for ($r = 0; $r -lt $Runs; $r++) {
    $before = ExplorerWindows
    $sw = [Diagnostics.Stopwatch]::StartNew()
    Start-Process explorer.exe -ArgumentList "`"$Folder`""
    $win = $null; $ms = $null
    while ($sw.Elapsed.TotalSeconds -lt 15) {
        if ($null -eq $win) {
            foreach ($w in $UIA::RootElement.FindAll($Tree::Children, $isExplorer)) {
                if ($before -notcontains $w.Current.NativeWindowHandle -and $w.Current.Name -like "*$leaf*") { $win = $w; break }
            }
        }
        if ($null -ne $win -and $null -ne $win.FindFirst($Tree::Descendants, $isListItem)) {
            $ms = $sw.Elapsed.TotalMilliseconds; break
        }
        Start-Sleep -Milliseconds 2
    }
    if ($null -ne $win) { [W]::PostMessage([IntPtr]$win.Current.NativeWindowHandle, 0x0010, [IntPtr]::Zero, [IntPtr]::Zero) | Out-Null }
    if ($null -eq $ms) { throw "run $r timed out" }
    $times += $ms
    Start-Sleep -Milliseconds 800
}
$sorted = $times | Sort-Object
$median = ($sorted[[int][math]::Floor(($Runs - 1) / 2)] + $sorted[[int][math]::Ceiling(($Runs - 1) / 2)]) / 2
$p95 = $sorted[[int][math]::Ceiling($Runs * 0.95) - 1]
"runs {0}: {1}" -f $Runs, (($times | ForEach-Object { $_.ToString('F0', [Globalization.CultureInfo]::InvariantCulture) }) -join ' ')
"median {0:F1} ms  p95 {1:F1} ms" -f $median, $p95
