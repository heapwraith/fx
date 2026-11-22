# T2.5: installs the fx-indexd service, waits for the index, measures how
# long a new file takes to show up in search (target ≤ 1 s) and to disappear
# after deletion, reads the service memory (P9), then removes the service and
# checks nothing is left (service, executable, folder).
#
# Needs Administrator. Prepared into %TEMP%\fx-elevated by
# bench/prepare-indexd.sh (elevated processes may not see \\wsl.localhost).
# Writes indexd-test.log next to itself. Only touches: the service, its copy in
# %ProgramFiles%\fx, and one probe file in %TEMP% (deleted).
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot
$log = Join-Path $PSScriptRoot 'indexd-test.log'
function Say($text) { $text | Tee-Object -FilePath $log -Append }
"fx-indexd test $(Get-Date -Format s)" | Out-File $log -Encoding utf8

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) { Say 'FAIL: not elevated'; exit 1 }
$exe = Join-Path $PSScriptRoot 'fx-indexd.exe'

Say '== install'
& $exe install 2>&1 | ForEach-Object { Say "  $_" }
if ($LASTEXITCODE -ne 0) { Say 'FAIL: install'; exit 1 }

Say '== index ready'
$sw = [Diagnostics.Stopwatch]::StartNew()
do { Start-Sleep -Milliseconds 250; $status = (& $exe status) -join ' ' } until ($status -match 'pronto' -or $sw.Elapsed.TotalSeconds -gt 120)
Say ("  {0} after {1:N1} s" -f $status, $sw.Elapsed.TotalSeconds)
$items = if ($status -match '\((\d+) itens\)') { [double]$Matches[1] } else { 0 }
Start-Sleep -Seconds 3
$proc = Get-Process fx-indexd -ErrorAction SilentlyContinue
if ($proc -and $items -gt 0) {
    $mb = $proc.PrivateMemorySize64 / 1MB
    Say ("  service memory {0:N1} MB for {1:N0} items = {2:N1} MB per million (P9 target <= 150)" -f $mb, $items, ($mb * 1e6 / $items))
}

Say '== new file shows up in search (target <= 1 s)'
$results = @()
for ($i = 0; $i -lt 5; $i++) {
    $name = "fxprobe$(Get-Random)"
    $file = Join-Path $env:TEMP "$name.txt"
    New-Item -ItemType File -Path $file | Out-Null
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do { $q = (& $exe query $name) -join "`n" } until ($q -match [regex]::Escape($name + '.txt') -or $sw.Elapsed.TotalSeconds -gt 5)
    $shown = $sw.Elapsed.TotalMilliseconds
    Remove-Item $file
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do { $q = (& $exe query $name) -join "`n" } until ($q -notmatch [regex]::Escape($name + '.txt') -or $sw.Elapsed.TotalSeconds -gt 5)
    $gone = $sw.Elapsed.TotalMilliseconds
    Say ("  probe {0}: shown after {1:N0} ms, gone after {2:N0} ms" -f ($i + 1), $shown, $gone)
    $results += $shown
}
Say ("  worst: {0:N0} ms" -f ($results | Measure-Object -Maximum).Maximum)
$q = (& $exe query 'notepad') -join "`n"
Say "  sample query 'notepad':"; ($q -split "`n" | Select-Object -First 4) | ForEach-Object { Say "    $_" }

Say '== uninstall'
& $exe uninstall 2>&1 | ForEach-Object { Say "  $_" }
$svc = Get-Service fx-indexd -ErrorAction SilentlyContinue
$copy = Join-Path ${env:ProgramFiles} 'fx\fx-indexd.exe'
Say ("  service left: {0}; executable left: {1}; folder left: {2}" -f [bool]$svc, (Test-Path $copy), (Test-Path (Split-Path $copy)))
Say 'done'
