#!/bin/bash
# Regenerate reference PNGs for led2d text rendering tests
# 
# Usage:
#   ./scripts/regenerate-text-pngs.sh [output-dir]
#
# If output-dir is not specified, uses a temp directory and prints its location.
# After visually inspecting the PNGs, copy them to tests/data/text_render/

set -e

OUTPUT_DIR="${1:-}"

if [ -z "$OUTPUT_DIR" ]; then
    OUTPUT_DIR=$(mktemp -d)
    export SERIALS_GENERATE_TEXT_PNGS="$OUTPUT_DIR"
    echo "Generating PNGs to: $OUTPUT_DIR"
else
    export SERIALS_GENERATE_TEXT_PNGS="$OUTPUT_DIR"
    echo "Generating PNGs to: $OUTPUT_DIR"
fi

cargo test --features host --no-default-features --test led2d_text_render

echo ""
echo "PNGs generated in: $OUTPUT_DIR"
echo ""
echo "To view them (requires an image viewer):"
echo "  eog $OUTPUT_DIR/*.png"
echo "  # or"
echo "  display $OUTPUT_DIR/*.png"
echo ""
echo "To copy to reference directory after visual inspection:"
echo "  cp $OUTPUT_DIR/*.png tests/data/text_render/"
