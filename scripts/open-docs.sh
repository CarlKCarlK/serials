#!/usr/bin/env bash
set -e

DISTRO=Ubuntu
DOC_PATH="$(pwd)/target/thumbv8m.main-none-eabihf/doc/serials/index.html"

WIN_PATH="\\\\wsl$\\$DISTRO${DOC_PATH//\//\\}"

explorer.exe "$WIN_PATH"
