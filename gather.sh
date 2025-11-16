#!/bin/bash

set -euo pipefail

output="all_code.txt"
rm -f "$output"

echo "Gathering Rust files from ., src, and examples..."
{
    echo "=========================================="

    for file in *.rs; do
        [ -e "$file" ] || continue
        echo "==== FILE: $file ===="
        cat "$file"
        echo
    done
} >> "$output"

if [ -d src ]; then
    while IFS= read -r -d '' file; do
        echo "==== FILE: $file ====" >> "$output"
        cat "$file" >> "$output"
        echo >> "$output"
    done < <(find src -name '*.rs' -print0)
fi

if [ -d examples ]; then
    while IFS= read -r -d '' file; do
        echo "==== FILE: $file ====" >> "$output"
        cat "$file" >> "$output"
        echo >> "$output"
    done < <(find examples -name '*.rs' -print0)
fi

echo "==========================================" >> "$output"
echo "Done! Combined files written to $output"
