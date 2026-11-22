#!/usr/bin/env bash
# Builds fx-indexd and copies it with bench/indexd-test.ps1 to
# %TEMP%\fx-elevated (elevated processes may not see \\wsl.localhost paths).
# Run from the repository root, in WSL: bench/prepare-indexd.sh
set -euo pipefail
cd "$(dirname "$0")/.."
cargo xwin build --release -p fx-indexd
win_temp=$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')
dest="$(wslpath -u "$win_temp")/fx-elevated"
mkdir -p "$dest"
cp target/x86_64-pc-windows-msvc/release/fx-indexd.exe bench/indexd-test.ps1 "$dest/"
echo "ready: $win_temp\\fx-elevated"
echo "in an elevated PowerShell:  cd \$env:TEMP\\fx-elevated; powershell -ExecutionPolicy Bypass -File .\\indexd-test.ps1"
