//! Host-side mapping algebra check for Led2d.
//!
//! Verifies that the clock-style 8x12 mapping matches a composition of
//! serpentine_12x4 panels using the const LedLayout primitives.

use device_kit::led_layout::LedLayout;

const CLOCK_EXPECTED: [(u16, u16); 96] = [
    (0, 11), (1, 11), (2, 11), (3, 11), (3, 10), (2, 10), (1, 10), (0, 10),
    (0, 9), (1, 9), (2, 9), (3, 9), (3, 8), (2, 8), (1, 8), (0, 8),
    (0, 7), (1, 7), (2, 7), (3, 7), (3, 6), (2, 6), (1, 6), (0, 6),
    (0, 5), (1, 5), (2, 5), (3, 5), (3, 4), (2, 4), (1, 4), (0, 4),
    (0, 3), (1, 3), (2, 3), (3, 3), (3, 2), (2, 2), (1, 2), (0, 2),
    (0, 1), (1, 1), (2, 1), (3, 1), (3, 0), (2, 0), (1, 0), (0, 0),
    (4, 11), (5, 11), (6, 11), (7, 11), (7, 10), (6, 10), (5, 10), (4, 10),
    (4, 9), (5, 9), (6, 9), (7, 9), (7, 8), (6, 8), (5, 8), (4, 8),
    (4, 7), (5, 7), (6, 7), (7, 7), (7, 6), (6, 6), (5, 6), (4, 6),
    (4, 5), (5, 5), (6, 5), (7, 5), (7, 4), (6, 4), (5, 4), (4, 4),
    (4, 3), (5, 3), (6, 3), (7, 3), (7, 2), (6, 2), (5, 2), (4, 2),
    (4, 1), (5, 1), (6, 1), (7, 1), (7, 0), (6, 0), (5, 0), (4, 0),
];

// Build the 8x12 mapping from two mirrored 12x4 serpentine panels:
// 1) serpentine_12x4 (4 rows x 12 cols)
// 2) rotate clockwise to get 12 rows x 4 cols
// 3) flip horizontally and vertically (panel orientation)
// 4) concat horizontally two panels to reach 8 cols (12 rows)
const PANEL_12X4: LedLayout<48, 4, 12> = LedLayout::<48, 4, 12>::serpentine_column_major();
const PANEL_12X4_ORIENTED: LedLayout<48, 12, 4> =
    PANEL_12X4.rotate_cw().flip_h().flip_v();
const CLOCK_COMPOSED: LedLayout<96, 12, 8> =
    PANEL_12X4_ORIENTED.concat_h::<48, 96, 4, 8>(PANEL_12X4_ORIENTED);

#[test]
fn clock_mapping_matches_composition() {
    assert_eq!(CLOCK_COMPOSED.map, CLOCK_EXPECTED);
}
