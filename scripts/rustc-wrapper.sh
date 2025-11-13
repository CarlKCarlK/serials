#!/bin/sh
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TMP_DIR="$ROOT_DIR/target/tmp"

mkdir -p "$TMP_DIR"
touch "$ROOT_DIR/target/.rustc-wrapper-used"

export TMPDIR="$TMP_DIR"
export TMP="$TMP_DIR"
export TEMP="$TMP_DIR"
export CARGO_TARGET_TMPDIR="$TMP_DIR"

REAL_RUSTC="$1"
shift

exec "$REAL_RUSTC" "$@"
