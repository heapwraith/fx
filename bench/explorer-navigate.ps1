# T0.10: time for an already open File Explorer window to show the first item
# after navigating to a folder (Shell.Application Navigate2), detected by UI
# Automation (a ListItem whose name is not one of the starting folder's items).
# Resolution ~5-15 ms. Comparable to fx "open folder with the app running".
param([Parameter(Mandatory = $true)][string]$Folder, [int]$Runs = 10)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$UIA = [System.Windows.Automation.AutomationElement]
$Tree = [System.Windows.Automation.TreeScope]
$isListItem = New-Object System.Windows.Automation.PropertyCondition(
    $UIA::ControlTypeProperty, [System.Windows.Automation.ControlType]::ListItem)
$parent = Split-Path -Parent $Folder
$parentItems = @(Get-ChildItem -LiteralPath $parent -Force | ForEach-Object Name)

$shell = New-Object -ComObject Shell.Application
$before = @($shell.Windows() | ForEach-Object { $_.HWND })
Start-Process explorer.exe -ArgumentList "`"$parent`""
$win = $null
for ($i = 0; $i -lt 100 -and $null -eq $win; $i++) {
    Start-Sleep -Milliseconds 100
    $win = $shell.Windows() | Where-Object { $before -notcontains $_.HWND } | Select-Object -First 1
}
if ($null -eq $win) { throw "Explorer window did not open" }
Start-Sleep -Milliseconds 1500
$el = $UIA::FromHandle([IntPtr]$win.HWND)

function FirstNewItem {
    $item = $el.FindFirst($Tree::Descendants, $isListItem)
    ($null -ne $item) -and ($parentItems -notcontains $item.Current.Name)
}

$times = @()
for ($r = 0; $r -lt $Runs; $r++) {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $win.Navigate2($Folder)
    $ms = $null
    while ($sw.Elapsed.TotalSeconds -lt 15) {
        if (FirstNewItem) { $ms = $sw.Elapsed.TotalMilliseconds; break }
        Start-Sleep -Milliseconds 2
    }
    if ($null -eq $ms) { throw "run $r timed out" }
    $times += $ms
    $win.Navigate2($parent)
    Start-Sleep -Milliseconds 1200
}
$win.Quit()
$sorted = $times | Sort-Object
$median = ($sorted[[int][math]::Floor(($Runs - 1) / 2)] + $sorted[[int][math]::Ceiling(($Runs - 1) / 2)]) / 2
$p95 = $sorted[[int][math]::Ceiling($Runs * 0.95) - 1]
"runs {0}: {1}" -f $Runs, (($times | ForEach-Object { $_.ToString('F0', [Globalization.CultureInfo]::InvariantCulture) }) -join ' ')
"median {0:F1} ms  p95 {1:F1} ms" -f $median, $p95
