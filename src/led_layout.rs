//! A fully const module that tells the x,y location of each LED pixel.
//!
//! See [`LedLayout`] for examples including: linear strips,
//! serpentine grids, rotations, flips, and concatenation.

/// A fully const struct that tells the x,y location of each LED pixel.
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
/// const ROTATED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().rotate_cw();
/// const EXPECTED: LedLayout<6, 2, 3> =
///     LedLayout::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
/// const _: () = assert!(ROTATED.equals(&EXPECTED));
/// ```
///
/// ```text
/// Serpentine 3×2 rotated to 2×3:
///   Before:            After:
///     LED0  LED3  LED4    LED1  LED0
///     LED1  LED2  LED5    LED2  LED3
///                         LED5  LED4
/// ```
///
/// Compile-time validation catches configuration errors:
///
/// ```compile_fail
/// # use device_kit::led_layout::LedLayout;
/// // Duplicate coordinate (0,0) - caught at compile time
/// const INVALID: LedLayout<3, 2, 2> = LedLayout::new([(0, 0), (0, 0), (1, 1)]);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LedLayout<const N: usize, const W: usize, const H: usize> {
    map: [(u16, u16); N],
}

impl<const N: usize, const W: usize, const H: usize> LedLayout<N, W, H> {
    /// Access the checked (col,row) mapping.
    #[must_use]
    pub const fn map(&self) -> &[(u16, u16); N] {
        &self.map
    }

    /// Reverse lookup: (row, col) → LED index in row-major order.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::led_layout::LedLayout;
    ///
    /// const LED_LAYOUT: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major();
    /// const MAPPING_BY_XY: [u16; 6] = LED_LAYOUT.mapping_by_xy();
    /// // Result: [0, 3, 4, 1, 2, 5]
    /// ```
    #[must_use]
    pub const fn mapping_by_xy(&self) -> [u16; N] {
        assert!(
            N <= u16::MAX as usize,
            "total LEDs must fit in u16 for mapping_by_xy"
        );

        let mut mapping = [None; N];

        let mut led_index = 0;
        while led_index < N {
            let (col, row) = self.map[led_index];
            let col = col as usize;
            let row = row as usize;
            assert!(col < W, "column out of bounds in mapping_by_xy");
            assert!(row < H, "row out of bounds in mapping_by_xy");
            let target_index = row * W + col;

            let slot = &mut mapping[target_index];
            assert!(
                slot.is_none(),
                "duplicate (col,row) in mapping_by_xy inversion"
            );
            *slot = Some(led_index as u16);

            led_index += 1;
        }

        let mut finalized = [0u16; N];
        let mut i = 0;
        while i < N {
            finalized[i] =
                mapping[i].expect("mapping_by_xy requires every (col,row) to be covered");
            i += 1;
        }

        finalized
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
    /// const LINEAR: LedLayout<4, 4, 1> = LedLayout::linear_h();
    /// const ROTATED: LedLayout<4, 4, 1> = LedLayout::linear_v().rotate_cw();
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

    /// Constructor: verifies mapping covers every cell exactly once across the W×H grid.
    ///
    /// ```rust,no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # #[panic_handler]
    /// # fn panic(_: &core::panic::PanicInfo) -> ! { loop {} }
    /// use device_kit::led_layout::LedLayout;
    ///
    /// // 3×2 grid (landscape, W×H)
    /// const MAP: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    ///
    /// // Rotate to portrait (CW)
    /// const ROTATED: LedLayout<6, 2, 3> = MAP.rotate_cw();
    ///
    /// // Expected: 2×3 grid (W×H)
    /// const EXPECTED: LedLayout<6, 2, 3> =
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
        assert!(W > 0 && H > 0, "W and H must be positive");
        assert!(W * H == N, "W*H must equal N");

        let mut seen = [false; N];

        let mut i = 0;
        while i < N {
            let (c, r) = map[i];
            let c = c as usize;
            let r = r as usize;

            assert!(c < W, "column out of bounds");
            assert!(r < H, "row out of bounds");

            let cell = r * W + c;
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
    /// const LINEAR: LedLayout<6, 6, 1> = LedLayout::linear_h();
    /// const EXPECTED: LedLayout<6, 6, 1> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (3, 0), (4, 0), (5, 0)]);
    /// const _: () = assert!(LINEAR.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// 6×1 strip maps to single row:
    ///   LED0  LED1  LED2  LED3  LED4  LED5
    /// ```
    #[must_use]
    pub const fn linear_h() -> Self {
        assert!(H == 1, "linear_h requires H == 1");
        assert!(W == N, "linear_h requires W == N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut column_index = 0;
        while column_index < W {
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
    /// const LINEAR: LedLayout<6, 1, 6> = LedLayout::linear_v();
    /// const EXPECTED: LedLayout<6, 1, 6> =
    ///     LedLayout::new([(0, 0), (0, 1), (0, 2), (0, 3), (0, 4), (0, 5)]);
    /// const _: () = assert!(LINEAR.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// 1×6 strip maps to single column:
    ///   LED0
    ///   LED1
    ///   LED2
    ///   LED3
    ///   LED4
    ///   LED5
    /// ```
    #[must_use]
    pub const fn linear_v() -> Self {
        assert!(W == 1, "linear_v requires W == 1");
        assert!(H == N, "linear_v requires H == N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut row_index = 0;
        while row_index < H {
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
    /// const MAP: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Strip snakes down columns (3×2 example):
    ///   LED0  LED3  LED4
    ///   LED1  LED2  LED5
    /// ```
    #[must_use]
    pub const fn serpentine_column_major() -> Self {
        assert!(W > 0 && H > 0, "W and H must be positive");
        assert!(W * H == N, "W*H must equal N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut row_index = 0;
        while row_index < H {
            let mut column_index = 0;
            while column_index < W {
                let led_index = if column_index % 2 == 0 {
                    // Even column: top-to-bottom
                    column_index * H + row_index
                } else {
                    // Odd column: bottom-to-top
                    column_index * H + (H - 1 - row_index)
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
    /// const MAP: LedLayout<6, 3, 2> = LedLayout::serpentine_row_major();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(0, 0), (1, 0), (2, 0), (2, 1), (1, 1), (0, 1)]);
    /// const _: () = assert!(MAP.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Strip snakes across rows (3×2 example):
    ///   LED0  LED1  LED2
    ///   LED5  LED4  LED3
    /// ```
    #[must_use]
    pub const fn serpentine_row_major() -> Self {
        assert!(W > 0 && H > 0, "W and H must be positive");
        assert!(W * H == N, "W*H must equal N");

        let mut mapping = [(0_u16, 0_u16); N];
        let mut row_index = 0;
        while row_index < H {
            let mut column_index = 0;
            while column_index < W {
                let led_index = if row_index % 2 == 0 {
                    row_index * W + column_index
                } else {
                    row_index * W + (W - 1 - column_index)
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
    /// const ROTATED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().rotate_cw();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(1, 0), (0, 0), (0, 1), (1, 1), (1, 2), (0, 2)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (3×2 serpentine): After (2×3):
    ///   LED0  LED3  LED4        LED1  LED0
    ///   LED1  LED2  LED5        LED2  LED3
    ///                           LED5  LED4
    /// ```
    #[must_use]
    pub const fn rotate_cw(self) -> LedLayout<N, H, W> {
        let mut out = [(0u16, 0u16); N];
        let mut i = 0;
        while i < N {
            let (c, r) = self.map[i];
            let c = c as usize;
            let r = r as usize;
            out[i] = ((H - 1 - r) as u16, c as u16);
            i += 1;
        }
        LedLayout::<N, H, W>::new(out)
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
    /// const FLIPPED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().flip_h();
    /// const EXPECTED: LedLayout<6, 3, 2> =
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
            out[i] = ((W - 1 - c) as u16, r);
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
    /// const ROTATED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().rotate_180();
    /// const EXPECTED: LedLayout<6, 3, 2> =
    ///     LedLayout::new([(2, 1), (2, 0), (1, 0), (1, 1), (0, 1), (0, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (3×2 serpentine): After 180°:
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
    /// const ROTATED: LedLayout<6, 2, 3> = LedLayout::serpentine_column_major().rotate_ccw();
    /// const EXPECTED: LedLayout<6, 2, 3> =
    ///     LedLayout::new([(0, 2), (1, 2), (1, 1), (0, 1), (0, 0), (1, 0)]);
    /// const _: () = assert!(ROTATED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Before (3×2 serpentine): After (2×3):
    ///   LED0  LED3  LED4        LED4  LED5
    ///   LED1  LED2  LED5        LED3  LED2
    ///                           LED0  LED1
    /// ```
    #[must_use]
    pub const fn rotate_ccw(self) -> LedLayout<N, H, W> {
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
    /// const FLIPPED: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major().flip_v();
    /// const EXPECTED: LedLayout<6, 3, 2> =
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
    /// const LED_LAYOUT: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major();
    /// const COMBINED: LedLayout<12, 6, 2> = LED_LAYOUT.concat_h::<6, 12, 3, 6>(LED_LAYOUT);
    /// const EXPECTED: LedLayout<12, 6, 2> = LedLayout::new([
    ///     (0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1), (3, 0), (3, 1), (4, 1),
    ///     (4, 0), (5, 0), (5, 1),
    /// ]);
    /// const _: () = assert!(COMBINED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Left serpentine (3×2):    Right serpentine (3×2):
    ///   0  3  4                   6  9 10
    ///   1  2  5                   7  8 11
    ///
    /// Combined (6×2):
    ///   0  3  4  6  9 10
    ///   1  2  5  7  8 11
    /// ```
    #[must_use]
    pub const fn concat_h<
        const N2: usize,
        const OUT_N: usize,
        const W2: usize,
        const OUT_W: usize,
    >(
        self,
        right: LedLayout<N2, W2, H>,
    ) -> LedLayout<OUT_N, OUT_W, H> {
        assert!(OUT_N == N + N2, "OUT_N must equal LEFT + RIGHT");
        assert!(OUT_W == W + W2, "OUT_W must equal W + W2");

        let mut out = [(0u16, 0u16); OUT_N];

        let mut i = 0;
        while i < N {
            out[i] = self.map[i];
            i += 1;
        }

        let mut j = 0;
        while j < N2 {
            let (c, r) = right.map[j];
            out[N + j] = ((c as usize + W) as u16, r);
            j += 1;
        }

        LedLayout::<OUT_N, OUT_W, H>::new(out)
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
    /// const LED_LAYOUT: LedLayout<6, 3, 2> = LedLayout::serpentine_column_major();
    /// const COMBINED: LedLayout<12, 3, 4> = LED_LAYOUT.concat_v::<6, 12, 2, 4>(LED_LAYOUT);
    /// const EXPECTED: LedLayout<12, 3, 4> = LedLayout::new([
    ///     (0, 0), (0, 1), (1, 1), (1, 0), (2, 0), (2, 1), (0, 2), (0, 3), (1, 3),
    ///     (1, 2), (2, 2), (2, 3),
    /// ]);
    /// const _: () = assert!(COMBINED.equals(&EXPECTED));
    /// ```
    ///
    /// ```text
    /// Top serpentine (3×2):    Bottom serpentine (3×2):
    ///   0  3  4                   6  9 10
    ///   1  2  5                   7  8 11
    ///
    /// Combined (3×4):
    ///   0  3  4
    ///   1  2  5
    ///   6  9 10
    ///   7  8 11
    /// ```
    #[must_use]
    pub const fn concat_v<
        const N2: usize,
        const OUT_N: usize,
        const H2: usize,
        const OUT_H: usize,
    >(
        self,
        bottom: LedLayout<N2, W, H2>,
    ) -> LedLayout<OUT_N, W, OUT_H> {
        assert!(OUT_N == N + N2, "OUT_N must equal TOP + BOTTOM");
        assert!(OUT_H == H + H2, "OUT_H must equal H + H2");

        // Derive vertical concat via transpose + horizontal concat + transpose back.
        // Transpose is implemented as rotate_cw + flip_h.
        let top_t = self.rotate_cw().flip_h(); // H width, W height
        let bot_t = bottom.rotate_cw().flip_h(); // H2 width, W height

        let combined_t: LedLayout<OUT_N, OUT_H, W> = top_t.concat_h::<N2, OUT_N, H2, OUT_H>(bot_t);

        combined_t.rotate_cw().flip_h() // transpose back to W x OUT_H
    }
}
