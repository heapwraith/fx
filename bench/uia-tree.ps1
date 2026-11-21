# T1.10: dumps the UI Automation tree of an fx window, the tree the Narrator
# reads (System.Windows.Automation, control view), plus the focused element.
# Read-only: sends no input.
param([Parameter(Mandatory = $true)][int]$ProcessId, [int]$MaxRows = 6)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient, UIAutomationTypes
$p = Get-Process -Id $ProcessId
for ($i = 0; $i -lt 50 -and $p.MainWindowHandle -eq 0; $i++) { Start-Sleep -Milliseconds 100; $p.Refresh() }
$root = [System.Windows.Automation.AutomationElement]::FromHandle($p.MainWindowHandle)
$walker = [System.Windows.Automation.TreeWalker]::ControlViewWalker

function Describe($e) {
    $c = $e.Current
    $text = "{0} '{1}'" -f $c.ControlType.ProgrammaticName.Replace('ControlType.', ''), $c.Name
    if ($c.HelpText) { $text += " [$($c.HelpText)]" }
    $sel = $null
    if ($e.TryGetCurrentPattern([System.Windows.Automation.SelectionItemPattern]::Pattern, [ref]$sel)) {
        if ($sel.Current.IsSelected) { $text += ' (selecionado)' }
    }
    $text
}

function Walk($e, $depth) {
    $n = 0
    $child = $walker.GetFirstChild($e)
    while ($child -ne $null) {
        if ($n -lt $MaxRows) { ('  ' * $depth) + (Describe $child) }
        elseif ($n -eq $MaxRows) { ('  ' * $depth) + '...' }
        Walk $child ($depth + 1)
        $n++
        $child = $walker.GetNextSibling($child)
    }
    if ($n -gt $MaxRows) { ('  ' * $depth) + "($n filhos)" }
}

'visible: ' + (-not $root.Current.IsOffscreen)
Describe $root
Walk $root 1
$focused = [System.Windows.Automation.AutomationElement]::FocusedElement
'focused: ' + (Describe $focused)
