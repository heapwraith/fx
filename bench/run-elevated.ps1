# Runs every Phase 0 measurement that needs Administrator (T0.7 and T0.8):
#   1. fx-bench index --volume C:   -> P6 (time) and P9 (memory) on the real disk
#   2. fx-bench search --volume C:  -> P5 on the real C: index
#   3. fx-index USN test (#[ignore]) -> create/rename/delete a file in %TEMP%
# Read-only on the disk except for the test file in %TEMP%, which it removes.
#
# Prepared by bench/prepare-elevated.sh into %TEMP%\fx-elevated (elevated
# processes may not see \\wsl.localhost). Run from an elevated PowerShell:
#   cd $env:TEMP\fx-elevated; powershell -ExecutionPolicy Bypass -File .\run-elevated.ps1
$ErrorActionPreference = 'Continue'
Set-Location $PSScriptRoot

$admin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {
    Write-Host "Run as Administrator: right-click PowerShell -> Run as administrator (execute como Administrador)."
    exit 1
}

$log = Join-Path $PSScriptRoot 'elevated.log'
"fx elevated run $(Get-Date -Format s)" | Out-File $log -Encoding utf8
function Step($title, [scriptblock]$cmd) {
    "`n== $title" | Tee-Object -FilePath $log -Append
    & $cmd 2>&1 | ForEach-Object { "$_" } | Tee-Object -FilePath $log -Append
    "exit code: $LASTEXITCODE" | Tee-Object -FilePath $log -Append
}

Step 'fx-bench index --volume C: (P6, P9)' { .\fx-bench.exe index --volume C: }
Step 'fx-bench search --volume C: (P5)' { .\fx-bench.exe search --volume C: }
Step 'fx-index USN test (create/rename/delete in %TEMP%)' { .\fx-index-tests.exe --ignored --nocapture }

"`nDone. Results: $log and bench\results\*.json in $PSScriptRoot" | Tee-Object -FilePath $log -Append
