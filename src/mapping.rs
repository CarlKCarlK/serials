//! LED index → (col,row) mapping utilities. See [`LedLayout`] for examples and transforms.
//!
//! Exposes a const-friendly [`LedLayout`] type plus generators and transforms used by led2d devices.

/// Checked LED index→(col,row) mapping for a fixed grid size.
///
/// # Examples
///
/// ```rust,no_run
/// # #![no_std]
/// # #![no_main]
/// # #[panic_handler]
/// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
/// use device_kit::mapping::LedLayout;
///
/// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_cw();
/// const EXPECTED: LedLayout<6, 3, 2> =
///     LedLayout::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
/// const _: () = assert!(ROTATED.equals(&EXPECTED));
/// ```
// cmk0 consider renaming LedLayout to better distinguish type vs instances.
// cmk0 consider renaming the map field for clarity (may no longer apply once API settles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedLayout<const N: usize, const ROWS: usize, const COLS: usize> {
    pub map: [(u16, u16); N],
}

impl<const N: usize, const ROWS: usize, const COLS: usize> LedLayout<N, ROWS, COLS> {
    /// Const equality helper for doctests/examples.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const LINEAR: LedLayout<4, 1, 4> = LedLayout::linear_h();
    /// const ROTATED: LedLayout<4, 1, 4> = LedLayout::<4, 4, 1>::linear_v().rotate_cw();
    ///
    /// const _: () = assert!(LINEAR.equals(&LINEAR));
    /// const _: () = assert!(!LINEAR.equals(&ROTATED));
    /// ```
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

    /// Constructor: verifies mapping covers every cell exactly once across the ROWS×COLS grid.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// // 3×2 grid (landscape)
    /// const MAP: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    ///
    /// // Rotate to portrait (CW)
    /// const ROTATED: LedLayout<6, 3, 2> = MAP.rotate_cw();
    ///
    /// // Expected: 2×3 grid
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(1, 0), (1, 1), (1, 2), (0, 2), (0, 1), (0, 0)]);
    ///
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
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

    /// Linear row-major mapping for a single-row strip (cols increase left-to-right).
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const LINEAR: LedLayout<6, 1, 6> = LedLayout::linear_h();
    /// const EXPECTED: LedLayout<6, 1, 6> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]);
    /// const _: () = assert!(LINEAR.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn linear_h() -> Self {
        assert!(ROWS == 1, "linear_h requires ROWS == 1");
        assert!(COLS == N, "linear_h requires COLS == N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut column_index = 0;
        while column_index < COLS {
            mapping[column_index] = (column_index as u16, 0);
            column_index += 1;
        }
        Self::new(mapping)
    }

    /// Linear column-major mapping for a single-column strip (rows increase top-to-bottom).
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const LINEAR: LedLayout<6, 6, 1> = LedLayout::linear_v();
    /// const EXPECTED: LedLayout<6, 6, 1> =
    ///     LedLayout::new([(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
    /// const _: () = assert!(LINEAR.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn linear_v() -> Self {
        assert!(COLS == 1, "linear_v requires COLS == 1");
        assert!(ROWS == N, "linear_v requires ROWS == N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut row_index = 0;
        while row_index < ROWS {
            mapping[row_index] = (0, row_index as u16);
            row_index += 1;
        }
        Self::new(mapping)
    }

    /// Serpentine column-major mapping returned as a checked `LedLayout`.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const MAP: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1)]);
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

    /// Serpentine row-major mapping (alternating left-to-right and right-to-left across rows).
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const MAP: LedLayout<6, 2, 3> = LedLayout::serpentine_row_major();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn serpentine_row_major() -> Self {
        assert!(ROWS > 0 && COLS > 0, "ROWS and COLS must be positive");
        assert!(ROWS * COLS == N, "ROWS*COLS must equal N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut row_index = 0;
        while row_index < ROWS {
            let mut column_index = 0;
            while column_index < COLS {
                let led_index = if row_index % 2 == 0 {
                    row_index * COLS + column_index
                } else {
                    row_index * COLS + (COLS - 1 - column_index)
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
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_cw();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn rotate_cw(self) -> LedLayout<N, COLS, ROWS> {
        let mut out = [(0u16, 0u16); N];
        let mut i = 0;
        while i < N {
            let (c, r) = self.map[i];
            let c = c as usize;
            let r = r as usize;
            out[i] = ((ROWS - 1 - r) as u16, c as u16);
            i += 1;
        }
        LedLayout::<N, COLS, ROWS>::new(out)
    }

    /// Flip horizontally (mirror columns).
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const FLIPPED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().flip_h();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(2, 0), (2, 1), (1, 1), (1, 0), (0, 0), (0, 1)]);
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
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const ROTATED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().rotate_180();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(2, 1), (2, 0), (1, 0), (1, 1), (0, 1), (0, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn rotate_180(self) -> Self {
        self.rotate_cw().rotate_cw()
    }

    /// Rotate 90° counter-clockwise derived from rotate_cw.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_ccw();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(0, 2), (1, 2), (1, 1), (0, 1), (0, 0), (1, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn rotate_ccw(self) -> LedLayout<N, COLS, ROWS> {
        self.rotate_cw().rotate_cw().rotate_cw()
    }

    /// Flip vertically derived from rotation + horizontal flip.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const FLIPPED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().flip_v();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 1), (0, 0), (1, 0), (1, 1), (2, 1), (2, 0)]);
    /// const _: () = assert!(FLIPPED.equals(&EXPECTED));
    /// ```
    #[must_use]
    pub const fn flip_v(self) -> Self {
        self.rotate_cw().flip_h().rotate_ccw()
    }

    /// Concatenate horizontally with another mapping sharing the same rows.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const LEFT: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major();
    /// const RIGHT: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major();
    /// const COMBINED: LedLayout<12, 2, 6> = LEFT.concat_h::<6, 12, 3, 6>(RIGHT);
    /// const EXPECTED: LedLayout<12, 2, 6> = LedLayout::new([
    ///     (0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1), (3, 0), (3, 1), (4, 1),
    ///     (4, 0), (5, 0), (5, 1),
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
        right: LedLayout<RIGHT, ROWS, RCOLS>,
    ) -> LedLayout<TOTAL, ROWS, TCOLS> {
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

        LedLayout::<TOTAL, ROWS, TCOLS>::new(out)
    }

    /// Concatenate vertically with another mapping sharing the same columns.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::mapping::LedLayout;
    ///
    /// const TOP: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major();
    /// const BOTTOM: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major();
    /// const COMBINED: LedLayout<12, 4, 3> = TOP.concat_v::<6, 12, 2, 4>(BOTTOM);
    /// const EXPECTED: LedLayout<12, 4, 3> = LedLayout::new([
    ///     (0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1), (0, 2), (0, 3), (1, 3),
    ///     (1, 2), (2, 2), (2, 3),
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
        bottom: LedLayout<BOTTOM, BOT_ROWS, COLS>,
    ) -> LedLayout<TOTAL, TROWS, COLS> {
        assert!(TOTAL == N + BOTTOM, "TOTAL must equal TOP + BOTTOM");
        assert!(
            TROWS == ROWS + BOT_ROWS,
            "TROWS must equal TOP_ROWS + BOT_ROWS"
        );

        // Derive vertical concat via transpose + horizontal concat + transpose back.
        // Transpose is implemented as rotate_cw + flip_h.
        let top_t = self.rotate_cw().flip_h(); // ROWS cols, COLS rows
        let bot_t = bottom.rotate_cw().flip_h(); // BOT_ROWS cols, COLS rows

        let combined_t: LedLayout<TOTAL, COLS, TROWS> =
            top_t.concat_h::<BOTTOM, TOTAL, BOT_ROWS, TROWS>(bot_t);

        combined_t.rotate_cw().flip_h() // transpose back to TROWS x COLS
    }
}
