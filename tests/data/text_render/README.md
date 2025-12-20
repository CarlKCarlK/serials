# LED2D Text Rendering Reference PNGs

This directory contains reference PNG images for the `led2d_text_render` tests. These tests verify that the text rendering functions produce the expected output by comparing the generated pixel data against these reference images.

## Running the Tests

To run the tests and verify text rendering matches the references:

```bash
cargo test --features host --no-default-features --test '*'
```

## Regenerating Reference PNGs

When you modify the font rendering code or want to update the reference images:

1. Generate new PNGs to a temp directory:

   ```bash
   ./scripts/regenerate-text-pngs.sh
   ```

   This will print the temp directory location.

2. Visually inspect the generated PNGs:

   ```bash
   eog /tmp/serials-text-pngs-XXXXX/*.png
   # or
   display /tmp/serials-text-pngs-XXXXX/*.png
   ```

3. If they look correct, copy them to replace the references:

   ```bash
   cp /tmp/serials-text-pngs-XXXXX/*.png tests/data/text_render/
   ```

4. Commit the updated PNGs to git.

## Current Test Cases

- `font3x4_12x4.png` - 3x4 font rendering "RUST" on a 12x4 display
- `font4x6_12x4.png` - 4x6 font rendering "RUST\ntwo" on a 12x4 display (bottom clips)
- `font6x10_24x16.png` - 6x10 font rendering "Hello Rust\nWrap me" on a 24x16 display

## How It Works

The tests use the `SERIALS_GENERATE_TEXT_PNGS` environment variable to switch between two modes:

- **Not set**: Test mode - compare rendered output against reference PNGs
- **Set to a directory**: Generation mode - write rendered output as PNGs to that directory

The `render_text_to_frame` function is tested with different font sizes, display dimensions, and text content to ensure proper rendering, clipping, wrapping, and color cycling.
