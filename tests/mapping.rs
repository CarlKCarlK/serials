//! Host-level tests for mapping primitives.

use device_kit::mapping::Mapping;

#[test]
fn linear_single_row_matches_expected() {
    const LINEAR: Mapping<4, 1, 4> = Mapping::<4, 1, 4>::linear_h();
    assert_eq!(LINEAR.map, [(0, 0), (1, 0), (2, 0), (3, 0)]);
}

#[test]
fn linear_single_column_matches_expected() {
    const LINEAR: Mapping<4, 4, 1> = Mapping::<4, 4, 1>::linear_v();
    assert_eq!(LINEAR.map, [(0, 0), (0, 1), (0, 2), (0, 3)]);
}

#[test]
fn linear_row_major_3x2_matches_expected() {
    const MAP: Mapping<6, 2, 3> = Mapping::<6, 2, 3>::linear_row_major();
    assert_eq!(
        MAP.map,
        [
            (0, 0),
            (1, 0),
            (2, 0),
            (0, 1),
            (1, 1),
            (2, 1),
        ]
    );
}

#[test]
fn rotate_and_flip_small_grid() {
    const MAP: Mapping<6, 2, 3> = Mapping::<6, 2, 3>::linear_row_major();
    let rotated = MAP.rotate_cw();
    assert_eq!(
        rotated.map,
        [
            (1, 0),
            (1, 1),
            (1, 2),
            (0, 0),
            (0, 1),
            (0, 2),
        ]
    );

    let flipped = MAP.flip_h();
    assert_eq!(
        flipped.map,
        [
            (2, 0),
            (1, 0),
            (0, 0),
            (2, 1),
            (1, 1),
            (0, 1),
        ]
    );
}

#[test]
fn concat_horizontal_and_vertical() {
    const LEFT: Mapping<2, 1, 2> = Mapping::<2, 1, 2>::linear_h();
    const RIGHT: Mapping<4, 1, 4> = Mapping::<4, 1, 4>::linear_h();
    let combined_h = LEFT.concat_h::<4, 6, 4, 6>(RIGHT);
    assert_eq!(combined_h.map, [(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]);

    const TOP: Mapping<2, 2, 1> = Mapping::<2, 2, 1>::linear_v();
    const BOTTOM: Mapping<3, 3, 1> = Mapping::<3, 3, 1>::linear_v();
    let combined_v = TOP.concat_v::<3, 5, 3, 5>(BOTTOM);
    assert_eq!(
        combined_v.map,
        [
            (0, 0),
            (0, 1),
            (0, 2),
            (0, 3),
            (0, 4),
        ]
    );
}
