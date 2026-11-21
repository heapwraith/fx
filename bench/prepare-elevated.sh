#!/usr/bin/env bash
# Builds what bench/run-elevated.ps1 needs and copies it to %TEMP%\fx-elevated on
# the Windows side (elevated processes may not see \\wsl.localhost paths).
# Run from the repository root, in WSL: bench/prepare-elevated.sh
set -euo pipefail
cd "$(dirname "$0")/.."
target=x86_64-pc-windows-msvc

cargo xwin build --target "$target" -p fx-bench --release
test_exe=$(cargo xwin test --target "$target" -p fx-index --no-run --message-format=json 2>/dev/null |
    grep -o '"executable":"[^"]*fx_index-[^"]*\.exe"' | tail -1 | sed 's/"executable":"//; s/"$//')
[ -n "$test_exe" ] || { echo "could not find the fx-index test executable" >&2; exit 1; }

win_temp=$(cmd.exe /c 'echo %TEMP%' 2>/dev/null | tr -d '\r')
dest="$(wslpath -u "$win_temp")/fx-elevated"
mkdir -p "$dest/bench/results"
cp "target/$target/release/fx-bench.exe" "$dest/"
cp "$test_exe" "$dest/fx-index-tests.exe"
cp bench/run-elevated.ps1 "$dest/"
echo "ready: $win_temp\\fx-elevated"
echo "in an elevated PowerShell:  cd \$env:TEMP\\fx-elevated; powershell -ExecutionPolicy Bypass -File .\\run-elevated.ps1"
