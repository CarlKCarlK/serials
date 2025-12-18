#!/bin/bash

set -euo pipefail

if ! command -v /mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe >/dev/null 2>&1; then
    echo "Windows PowerShell executable not found." >&2
    exit 1
fi

# Get the directory where this script lives
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Locate probeusb.ps1 inside scripts/
SCRIPT_WIN_PATH="$(wslpath -w "$SCRIPT_DIR/probeusb.ps1")"

/mnt/c/Windows/System32/WindowsPowerShell/v1.0/powershell.exe -ExecutionPolicy Bypass -File "$SCRIPT_WIN_PATH" wsl
