//! Mapping primitives for LED index → (col,row) layouts.
//!
//! Exposes a const-friendly `Mapping` type plus generators and transforms used by led2d devices.

/// Checked LED index→(col,row) mapping for a fixed grid size.
// cmk0 consider renaming Mapping to better distinguish type vs instances.
// cmk0 consider renaming the map field for clarity (may no longer apply once API settles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping<const N: usize, const ROWS: usize, const COLS: usize> {
    pub map: [(u16, u16); N],
}

impl<const N: usize, const ROWS: usize, const COLS: usize> Mapping<N, ROWS, COLS> {
    /// Const equality helper for doctests/examples.
    #[must_use]
    pub const fn equals(&self, other: &Self) -> bool {
        let mut i = 0;
        while i < N {
            if self.map[i].0 != other.map[i].0 || self.map[i].1 != other.map[i].1 {
                return false;
            }
            i += 1;
        }
        true
    }

    /// Constructor: verifies mapping is a bijection from indices 0..N onto the ROWS×COLS grid.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const MAP: Mapping<6, 2, 3> = Mapping::new([
    ///     (0, 0),
    ///     (0, 1),
    ///     (1, 1),
    ///     (1, 0),
    ///     (2, 0),
    ///     (2, 1),
    /// ]);
    /// const EXPECTED: Mapping<6, 2, 3> =
    ///     Mapping::new([(0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn new(map: [(u16, u16); N]) -> Self {
        assert!(ROWS > 0 && COLS > 0, "ROWS and COLS must be positive");
        assert!(ROWS * COLS == N, "ROWS*COLS must equal N");

        let mut seen = [false; N];

        let mut i = 0;
        while i < N {
            let (c, r) = map[i];
            let c = c as usize;
            let r = r as usize;

            assert!(c < COLS, "column out of bounds");
            assert!(r < ROWS, "row out of bounds");

            let cell = r * COLS + c;
            assert!(!seen[cell], "duplicate (col,row) in mapping");
            seen[cell] = true;

            i += 1;
        }

        let mut k = 0;
        while k < N {
            assert!(seen[k], "mapping does not cover every cell");
            k += 1;
        }

        Self { map }
    }

    /// Serpentine column-major mapping returned as a checked `Mapping`.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const MAP: Mapping<6, 2, 3> = Mapping::serpentine_column_major();
    /// const EXPECTED: Mapping<6, 2, 3> =
    ///     Mapping::new([(0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn serpentine_column_major() -> Self {
        assert!(ROWS > 0 && COLS > 0, "ROWS and COLS must be positive");
        assert!(ROWS * COLS == N, "ROWS*COLS must equal N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut row_index = 0;
        while row_index < ROWS {
            let mut column_index = 0;
            while column_index < COLS {
                let led_index = if column_index % 2 == 0 {
                    // Even column: top-to-bottom
                    column_index * ROWS + row_index
                } else {
                    // Odd column: bottom-to-top
                    column_index * ROWS + (ROWS - 1 - row_index)
                };
                mapping[led_index] = (column_index as u16, row_index as u16);
                column_index += 1;
            }
            row_index += 1;
        }
        Self::new(mapping)
    }

    /// Rotate 90° clockwise (dims swap).
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const ROTATED: Mapping<6, 3, 2> = Mapping::serpentine_column_major().rotate_cw();
    /// const EXPECTED: Mapping<6, 3, 2> =
    ///     Mapping::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn rotate_cw(self) -> Mapping<N, COLS, ROWS> {
        let mut out = [(0u16, 0u16); N];
        let mut i = 0;
        while i < N {
            let (c, r) = self.map[i];
            let c = c as usize;
            let r = r as usize;
            out[i] = ((ROWS - 1 - r) as u16, c as u16);
            i += 1;
        }
        Mapping::<N, COLS, ROWS>::new(out)
    }

    /// Flip horizontally (mirror columns).
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const FLIPPED: Mapping<6, 2, 3> = Mapping::serpentine_column_major().flip_h();
    /// const EXPECTED: Mapping<6, 2, 3> =
    ///     Mapping::new([(2, 0), (2, 1), (1, 1), (1, 0), (0, 0), (0, 1)]);
    /// const _: () = assert!(FLIPPED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn flip_h(self) -> Self {
        let mut out = [(0u16, 0u16); N];
        let mut i = 0;
        while i < N {
            let (c, r) = self.map[i];
            let c = c as usize;
            out[i] = ((COLS - 1 - c) as u16, r);
            i += 1;
        }
        Self::new(out)
    }

    /// Rotate 180° derived from rotate_cw.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const ROTATED: Mapping<6, 2, 3> = Mapping::serpentine_column_major().rotate_180();
    /// const EXPECTED: Mapping<6, 2, 3> =
    ///     Mapping::new([(2, 1), (2, 0), (1, 0), (1, 1), (0, 1), (0, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn rotate_180(self) -> Self {
        self.rotate_cw().rotate_cw()
    }

    /// Rotate 90° counter-clockwise derived from rotate_cw.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const ROTATED: Mapping<6, 3, 2> = Mapping::serpentine_column_major().rotate_ccw();
    /// const EXPECTED: Mapping<6, 3, 2> =
    ///     Mapping::new([(0, 2), (1, 2), (1, 1), (0, 1), (0, 0), (1, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn rotate_ccw(self) -> Mapping<N, COLS, ROWS> {
        self.rotate_cw().rotate_cw().rotate_cw()
    }

    /// Flip vertically derived from rotation + horizontal flip.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const FLIPPED: Mapping<6, 2, 3> = Mapping::serpentine_column_major().flip_v();
    /// const EXPECTED: Mapping<6, 2, 3> =
    ///     Mapping::new([(0, 1), (0, 0), (1, 0), (1, 1), (2, 1), (2, 0)]);
    /// const _: () = assert!(FLIPPED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn flip_v(self) -> Self {
        self.rotate_cw().flip_h().rotate_ccw()
    }

    /// Concatenate horizontally with another mapping sharing the same rows.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const LEFT: Mapping<6, 2, 3> = Mapping::serpentine_column_major();
    /// const RIGHT: Mapping<6, 2, 3> = Mapping::serpentine_column_major();
    /// const COMBINED: Mapping<12, 2, 6> = LEFT.concat_h::<6, 12, 3, 6>(RIGHT);
    /// const EXPECTED: Mapping<12, 2, 6> = Mapping::new([
    ///     (0, 0),
    ///     (0, 1),
    ///     (1, 1),
    ///     (1, 0),
    ///     (2, 0),
    ///     (2, 1),
    ///     (3, 0),
    ///     (3, 1),
    ///     (4, 1),
    ///     (4, 0),
    ///     (5, 0),
    ///     (5, 1),
    /// ]);
    /// const _: () = assert!(COMBINED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn concat_h<
        const RIGHT: usize,
        const TOTAL: usize,
        const RCOLS: usize,
        const TCOLS: usize,
    >(
        self,
        right: Mapping<RIGHT, ROWS, RCOLS>,
    ) -> Mapping<TOTAL, ROWS, TCOLS> {
        assert!(TOTAL == N + RIGHT, "TOTAL must equal LEFT + RIGHT");
        assert!(TCOLS == COLS + RCOLS, "TCOLS must equal LCOLS + RCOLS");

        let mut out = [(0u16, 0u16); TOTAL];

        let mut i = 0;
        while i < N {
            out[i] = self.map[i];
            i += 1;
        }

        let mut j = 0;
        while j < RIGHT {
            let (c, r) = right.map[j];
            out[N + j] = ((c as usize + COLS) as u16, r);
            j += 1;
        }

        Mapping::<TOTAL, ROWS, TCOLS>::new(out)
    }

    /// Concatenate vertically with another mapping sharing the same columns.
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::Mapping;
    ///
    /// const TOP: Mapping<6, 2, 3> = Mapping::serpentine_column_major();
    /// const BOTTOM: Mapping<6, 2, 3> = Mapping::serpentine_column_major();
    /// const COMBINED: Mapping<12, 4, 3> = TOP.concat_v::<6, 12, 2, 4>(BOTTOM);
    /// const EXPECTED: Mapping<12, 4, 3> = Mapping::new([
    ///     (0, 0),
    ///     (0, 1),
    ///     (1, 1),
    ///     (1, 0),
    ///     (2, 0),
    ///     (2, 1),
    ///     (0, 2),
    ///     (0, 3),
    ///     (1, 3),
    ///     (1, 2),
    ///     (2, 2),
    ///     (2, 3),
    /// ]);
    /// const _: () = assert!(COMBINED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn concat_v<
        const BOTTOM: usize,
        const TOTAL: usize,
        const BOT_ROWS: usize,
        const TROWS: usize,
    >(
        self,
        bottom: Mapping<BOTTOM, BOT_ROWS, COLS>,
    ) -> Mapping<TOTAL, TROWS, COLS> {
        assert!(TOTAL == N + BOTTOM, "TOTAL must equal TOP + BOTTOM");
        assert!(
            TROWS == ROWS + BOT_ROWS,
            "TROWS must equal TOP_ROWS + BOT_ROWS"
        );

        // Derive vertical concat via transpose + horizontal concat + transpose back.
        // Transpose is implemented as rotate_cw + flip_h.
        let top_t = self.rotate_cw().flip_h(); // ROWS cols, COLS rows
        let bot_t = bottom.rotate_cw().flip_h(); // BOT_ROWS cols, COLS rows

        let combined_t: Mapping<TOTAL, COLS, TROWS> =
            top_t.concat_h::<BOTTOM, TOTAL, BOT_ROWS, TROWS>(bot_t);

        combined_t.rotate_cw().flip_h() // transpose back to TROWS x COLS
    }
}
