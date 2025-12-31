//! LED index → (col,row) layout utilities for 2D LED panels. See [`LedLayout`] for const-checked
//! grid layouts plus the common patterns (linear strips, serpentine grids, rotations, flips, and
//! concatenation) used throughout led2d devices.

/// Checked LED index→(col,row) mapping for a fixed grid size.
///
/// # Examples
///
/// ```rust,no_run
/// # #![no_std]
/// # #![no_main]
/// # #[panic_handler]
/// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
/// use device_kit::led_layout::LedLayout;
///
/// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_cw();
/// const EXPECTED: LedLayout<6, 3, 2> =
///     LedLayout::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
/// const _: () = assert!(ROTATED.equals(&EXPECTED));
/// ```
///
/// ```text
/// Serpentine 2×3 rotated to 3×2:
///   Before:            After:
///     LED0  LED3  LED4    LED1  LED0
///     LED1  LED2  LED5    LED2  LED3
///                         LED5  LED4
/// ```
// cmk0 consider renaming LedLayout to better distinguish type vs instances.
// cmk0 consider renaming the map field for clarity (may no longer apply once API settles).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedLayout<const N: usize, const ROWS: usize, const COLS: usize> {
    map: [(u16, u16); N],
}

impl<const N: usize, const ROWS: usize, const COLS: usize> LedLayout<N, ROWS, COLS> {
    /// Access the checked (col,row) mapping.
    #[must_use]
    pub const fn map(&self) -> &[(u16, u16); N] {
        &self.map
    }

    /// Const equality helper for doctests/examples.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const LINEAR: LedLayout<4, 1, 4> = LedLayout::linear_h();
    /// const ROTATED: LedLayout<4, 1, 4> = LedLayout::<4, 4, 1>::linear_v().rotate_cw();
    ///
    /// const _: () = assert!(LINEAR.equals(&LINEAR));
    /// const _: () = assert!(!LINEAR.equals(&ROTATED));
    /// ```
    ///
    /// ```text
    /// LINEAR:  LED0  LED1  LED2  LED3
    /// ROTATED: LED3  LED2  LED1  LED0
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
    /// use device_kit::led_layout::LedLayout;
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
    ///
    /// ```text
    /// 3×2 input (col,row by LED index):
    ///   LED0  LED1  LED2
    ///   LED5  LED4  LED3
    ///
    /// After rotate to 2×3:
    ///   LED1  LED0
    ///   LED2  LED3
    ///   LED5  LED4
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const LINEAR: LedLayout<6, 1, 6> = LedLayout::linear_h();
    /// const EXPECTED: LedLayout<6, 1, 6> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]);
    /// const _: () = assert!(LINEAR.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// 1×6 strip maps to single row:
    ///   LED0  LED1  LED2  LED3  LED4  LED5
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const LINEAR: LedLayout<6, 6, 1> = LedLayout::linear_v();
    /// const EXPECTED: LedLayout<6, 6, 1> =
    ///     LedLayout::new([(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
    /// const _: () = assert!(LINEAR.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// 6×1 strip maps to single column:
    ///   LED0
    ///   LED1
    ///   LED2
    ///   LED3
    ///   LED4
    ///   LED5
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const MAP: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Strip snakes down columns (2×3 example):
    ///   LED0  LED3  LED4
    ///   LED1  LED2  LED5
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const MAP: LedLayout<6, 2, 3> = LedLayout::serpentine_row_major();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Strip snakes across rows (2×3 example):
    ///   LED0  LED1  LED2
    ///   LED5  LED4  LED3
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_cw();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (2×3 serpentine): After (3×2):
    ///   LED0  LED3  LED4        LED1  LED0
    ///   LED1  LED2  LED5        LED2  LED3
    ///                           LED5  LED4
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const FLIPPED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().flip_h();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(2, 0), (2, 1), (1, 1), (1, 0), (0, 0), (0, 1)]);
    /// const _: () = assert!(FLIPPED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (serpentine): After:
    ///   LED0  LED3  LED4      LED4  LED3  LED0
    ///   LED1  LED2  LED5      LED5  LED2  LED1
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const ROTATED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().rotate_180();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(2, 1), (2, 0), (1, 0), (1, 1), (0, 1), (0, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (2×3 serpentine): After 180°:
    ///   LED0  LED3  LED4        LED5  LED2  LED1
    ///   LED1  LED2  LED5        LED4  LED3  LED0
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_ccw();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(0, 2), (1, 2), (1, 1), (0, 1), (0, 0), (1, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (2×3 serpentine): After (3×2):
    ///   LED0  LED3  LED4        LED4  LED5
    ///   LED1  LED2  LED5        LED3  LED2
    ///                           LED0  LED1
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
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const FLIPPED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().flip_v();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 1), (0, 0), (1, 0), (1, 1), (2, 1), (2, 0)]);
    /// const _: () = assert!(FLIPPED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (serpentine): After:
    ///   LED0  LED3  LED4      LED1  LED2  LED5
    ///   LED1  LED2  LED5      LED0  LED3  LED4
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
    /// use device_kit::led_layout::LedLayout;
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
    ///
    /// ```text
    /// Left serpentine (2×3):    Right serpentine (2×3):
    ///   0  3  4                   6  9 10
    ///   1  2  5                   7  8 11
    ///
    /// Combined (2×6):
    ///   0  3  4  6  9 10
    ///   1  2  5  7  8 11
    /// ```
    #[must_use]
    pub const fn concat_h<
        const N2: usize,
        const OUT_N: usize,
        const COLS2: usize,
        const OUT_COLS: usize,
    >(
        self,
        right: LedLayout<N2, ROWS, COLS2>,
    ) -> LedLayout<OUT_N, ROWS, OUT_COLS> {
        assert!(OUT_N == N + N2, "OUT_N must equal LEFT + RIGHT");
        assert!(OUT_COLS == COLS + COLS2, "OUT_COLS must equal COLS + COLS2");

        let mut out = [(0u16, 0u16); OUT_N];

        let mut i = 0;
        while i < N {
            out[i] = self.map[i];
            i += 1;
        }

        let mut j = 0;
        while j < N2 {
            let (c, r) = right.map[j];
            out[N + j] = ((c as usize + COLS) as u16, r);
            j += 1;
        }

        LedLayout::<OUT_N, ROWS, OUT_COLS>::new(out)
    }

    /// Concatenate vertically with another mapping sharing the same columns.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::led_layout::LedLayout;
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
    ///
    /// ```text
    /// Top serpentine (2×3):    Bottom serpentine (2×3):
    ///   0  3  4                   6  9 10
    ///   1  2  5                   7  8 11
    ///
    /// Combined (4×3):
    ///   0  3  4
    ///   1  2  5
    ///   6  9 10
    ///   7  8 11
    /// ```
    #[must_use]
    pub const fn concat_v<
        const N2: usize,
        const OUT_N: usize,
        const ROWS2: usize,
        const OUT_ROWS: usize,
    >(
        self,
        bottom: LedLayout<N2, ROWS2, COLS>,
    ) -> LedLayout<OUT_N, OUT_ROWS, COLS> {
        assert!(OUT_N == N + N2, "OUT_N must equal TOP + BOTTOM");
        assert!(
            OUT_ROWS == ROWS + ROWS2,
            "OUT_ROWS must equal ROWS + ROWS2"
        );

        // Derive vertical concat via transpose + horizontal concat + transpose back.
        // Transpose is implemented as rotate_cw + flip_h.
        let top_t = self.rotate_cw().flip_h(); // ROWS cols, COLS rows
        let bot_t = bottom.rotate_cw().flip_h(); // ROWS2 cols, COLS rows

        let combined_t: LedLayout<OUT_N, COLS, OUT_ROWS> =
            top_t.concat_h::<N2, OUT_N, ROWS2, OUT_ROWS>(bot_t);

        combined_t.rotate_cw().flip_h() // transpose back to OUT_ROWS x COLS
    }
}
