# Helpers for the end-to-end tests (.github/workflows/e2e.yml), dot-sourced
# by each step. They run on fresh GitHub-hosted Windows runners, where the
# account is an administrator without UAC prompts: what cannot be tested on
# the developer's PC without a click (service, real MFT) runs here.
$ErrorActionPreference = 'Stop'

$Programs = Join-Path $env:LOCALAPPDATA 'Programs\fx'
$StartMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'Pathdash.lnk'
$LegacyStartMenu = Join-Path ([Environment]::GetFolderPath('Programs')) 'fx.lnk'
$UninstallKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\fx'
$InnoKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\{6E7C2B5A-3F1D-4C8E-9A2B-0F4D5E6A7B8C}_is1'
$Shots = Join-Path $env:RUNNER_TEMP 'shots'
New-Item -ItemType Directory -Force $Shots | Out-Null

# One line of the job summary per check, and a failure stops the step.
function Check([string]$what, [bool]$ok, [string]$detail = '') {
    $mark = if ($ok) { 'ok' } else { 'FALHOU' }
    "| $what | $mark | $detail |" | Out-File -Append -Encoding utf8 $env:GITHUB_STEP_SUMMARY
    Write-Host "[$mark] $what $detail"
    if (-not $ok) { throw "falhou: $what $detail" }
}

function Summary([string]$title) {
    "`n### $title`n`n| Verificação | Resultado | Detalhe |`n|---|---|---|" |
        Out-File -Append -Encoding utf8 $env:GITHUB_STEP_SUMMARY
}

# The whole screen, as the runner's desktop shows it.
function Shot([string]$name) {
    Add-Type -AssemblyName System.Windows.Forms, System.Drawing
    $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
    $bmp.Save((Join-Path $Shots "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
}

# Waits up to $seconds for $test to be true.
function WaitFor([scriptblock]$test, [int]$seconds) {
    $end = (Get-Date).AddSeconds($seconds)
    while ((Get-Date) -lt $end) {
        if (& $test) { return $true }
        Start-Sleep -Milliseconds 500
    }
    return [bool](& $test)
}

# Starts Pathdash on $folder, checks it stays up with a window, takes a
# screenshot and closes it.
function LaunchAndShoot([string]$exe, [string]$folder, [string]$name) {
    $p = Start-Process -FilePath $exe -ArgumentList "`"$folder`"" -PassThru
    $window = WaitFor { $p.Refresh(); $p.MainWindowHandle -ne 0 } 30
    Start-Sleep -Seconds 3
    $p.Refresh()
    Shot $name
    Check "Pathdash abre uma janela ($name)" ($window -and -not $p.HasExited) "PID $($p.Id), título '$($p.MainWindowTitle)'"
    $p.CloseMainWindow() | Out-Null
    if (-not (WaitFor { $p.HasExited } 10)) { Stop-Process -Id $p.Id -Force }
}

function Uninstall-Quiet {
    & (Join-Path $Programs 'fx-setup.exe') --uninstall --quiet
    WaitFor { -not (Test-Path (Join-Path $Programs 'pathdash.exe')) } 60 | Out-Null
}
