//! Mapping primitives for LED index → (col,row) layouts.
//!
//! Exposes a const-friendly `Mapping` type plus generators and transforms used by led2d devices.

/// Checked LED index→(col,row) mapping for a fixed grid size.
#[derive(Clone, Copy)]
pub struct Mapping<const N: usize, const ROWS: usize, const COLS: usize> {
    pub map: [(u16, u16); N],
}

impl<const N: usize, const ROWS: usize, const COLS: usize> Mapping<N, ROWS, COLS> {
    /// Constructor: verifies mapping is a bijection from indices 0..N onto the ROWS×COLS grid.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: Mapping<4, 2, 2> =
    ///         Mapping::new([(0, 0), (0, 1), (1, 1), (1, 0)]);
    ///     assert_eq!(MAP.map, [(0, 0), (0, 1), (1, 1), (1, 0)]);
    /// }
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

    /// Serpentine column-major mapping (LED index → `(col, row)`).
    ///
    /// Even columns go top-to-bottom (row 0→ROWS-1), odd columns go bottom-to-top (row ROWS-1→0).
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: [(u16, u16); 4] = Mapping::<4, 2, 2>::serpentine_column_major_array();
    ///     assert_eq!(MAP, [(0, 0), (0, 1), (1, 1), (1, 0)]);
    /// }
    /// ```
    #[must_use]
    pub const fn serpentine_column_major_array() -> [(u16, u16); N] {
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
        mapping
    }

    /// Serpentine column-major mapping returned as a checked `Mapping`.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: Mapping<4, 2, 2> = Mapping::<4, 2, 2>::serpentine_column_major();
    ///     assert_eq!(MAP.map, [(0, 0), (0, 1), (1, 1), (1, 0)]);
    /// }
    /// ```
    #[must_use]
    pub const fn serpentine_column_major() -> Self {
        Self::new(Self::serpentine_column_major_array())
    }

    /// Row-major mapping (no serpentine): 0→(0,0), 1→(1,0), then next row.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: [(u16, u16); 6] = Mapping::<6, 2, 3>::linear_row_major_array();
    ///     assert_eq!(
    ///         MAP,
    ///         [
    ///             (0, 0),
    ///             (1, 0),
    ///             (2, 0),
    ///             (0, 1),
    ///             (1, 1),
    ///             (2, 1),
    ///         ]
    ///     );
    /// }
    /// ```
    #[must_use]
    pub const fn linear_row_major_array() -> [(u16, u16); N] {
        assert!(ROWS > 0 && COLS > 0, "ROWS and COLS must be positive");
        assert!(ROWS * COLS == N, "ROWS*COLS must equal N");

        let mut map = [(0u16, 0u16); N];
        let mut row_index = 0;
        while row_index < ROWS {
            let mut column_index = 0;
            while column_index < COLS {
                let led_index = row_index * COLS + column_index;
                map[led_index] = (column_index as u16, row_index as u16);
                column_index += 1;
            }
            row_index += 1;
        }
        map
    }

    /// Row-major mapping returned as a checked `Mapping`.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: Mapping<6, 2, 3> = Mapping::<6, 2, 3>::linear_row_major();
    ///     assert_eq!(
    ///         MAP.map,
    ///         [
    ///             (0, 0),
    ///             (1, 0),
    ///             (2, 0),
    ///             (0, 1),
    ///             (1, 1),
    ///             (2, 1),
    ///         ]
    ///     );
    /// }
    /// ```
    #[must_use]
    pub const fn linear_row_major() -> Self {
        Self::new(Self::linear_row_major_array())
    }

    /// Horizontal linear mapping (single row): 0→(0,0), 1→(1,0), ..., N-1→(N-1,0).
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: Mapping<3, 1, 3> = Mapping::<3, 1, 3>::linear_h();
    ///     assert_eq!(MAP.map, [(0, 0), (1, 0), (2, 0)]);
    /// }
    /// ```
    #[must_use]
    pub const fn linear_h() -> Self {
        assert!(ROWS == 1, "linear_h requires ROWS == 1");
        Self::linear_row_major()
    }

    /// Vertical linear mapping (single column): 0→(0,0), 1→(0,1), ..., N-1→(0,N-1).
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const MAP: Mapping<3, 3, 1> = Mapping::<3, 3, 1>::linear_v();
    ///     assert_eq!(MAP.map, [(0, 0), (0, 1), (0, 2)]);
    /// }
    /// ```
    #[must_use]
    pub const fn linear_v() -> Self {
        assert!(COLS == 1, "linear_v requires COLS == 1");
        assert!(ROWS > 0, "ROWS must be positive");

        // Define in terms of the horizontal mapping for consistency.
        let rotated = Mapping::<N, 1, N>::linear_h().rotate_cw();
        Mapping::<N, ROWS, COLS>::new(rotated.map)
    }

    /// Rotate 90° clockwise (dims swap).
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const BASE: Mapping<4, 2, 2> = Mapping::<4, 2, 2>::linear_row_major();
    ///     const ROTATED: Mapping<4, 2, 2> = BASE.rotate_cw();
    ///     assert_eq!(ROTATED.map, [(1, 0), (1, 1), (0, 0), (0, 1)]);
    /// }
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
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const BASE: Mapping<4, 2, 2> = Mapping::<4, 2, 2>::linear_row_major();
    ///     const FLIPPED: Mapping<4, 2, 2> = BASE.flip_h();
    ///     assert_eq!(FLIPPED.map, [(1, 0), (0, 0), (1, 1), (0, 1)]);
    /// }
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
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const BASE: Mapping<4, 2, 2> = Mapping::<4, 2, 2>::linear_row_major();
    ///     const ROTATED: Mapping<4, 2, 2> = BASE.rotate_180();
    ///     assert_eq!(ROTATED.map, [(1, 1), (0, 1), (1, 0), (0, 0)]);
    /// }
    /// ```
    #[must_use]
    pub const fn rotate_180(self) -> Self {
        self.rotate_cw().rotate_cw()
    }

    /// Rotate 90° counter-clockwise derived from rotate_cw.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const BASE: Mapping<4, 2, 2> = Mapping::<4, 2, 2>::linear_row_major();
    ///     const ROTATED: Mapping<4, 2, 2> = BASE.rotate_ccw();
    ///     assert_eq!(ROTATED.map, [(0, 1), (0, 0), (1, 1), (1, 0)]);
    /// }
    /// ```
    #[must_use]
    pub const fn rotate_ccw(self) -> Mapping<N, COLS, ROWS> {
        self.rotate_cw().rotate_cw().rotate_cw()
    }

    /// Flip vertically derived from rotation + horizontal flip.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const BASE: Mapping<4, 2, 2> = Mapping::<4, 2, 2>::linear_row_major();
    ///     const FLIPPED: Mapping<4, 2, 2> = BASE.flip_v();
    ///     assert_eq!(FLIPPED.map, [(0, 1), (1, 1), (0, 0), (1, 0)]);
    /// }
    /// ```
    #[must_use]
    pub const fn flip_v(self) -> Self {
        self.rotate_cw().flip_h().rotate_ccw()
    }

    /// Concatenate horizontally with another mapping sharing the same rows.
    ///
    /// ```no_run
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const LEFT: Mapping<2, 1, 2> = Mapping::<2, 1, 2>::linear_h();
    ///     const RIGHT: Mapping<2, 1, 2> = Mapping::<2, 1, 2>::linear_h();
    ///     const COMBINED: Mapping<4, 1, 4> = LEFT.concat_h::<2, 4, 2, 4>(RIGHT);
    ///     assert_eq!(COMBINED.map, [(0, 0), (1, 0), (2, 0), (3, 0)]);
    /// }
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
    /// #![no_std]
    /// # use core::panic::PanicInfo;
    /// use device_kit::mapping::Mapping;
    ///
    /// # #[panic_handler]
    /// # fn panic(_info: &PanicInfo) -> ! { loop {} }
    /// fn main() {
    ///     const TOP: Mapping<2, 2, 1> = Mapping::<2, 2, 1>::linear_v();
    ///     const BOTTOM: Mapping<3, 3, 1> = Mapping::<3, 3, 1>::linear_v();
    ///     const COMBINED: Mapping<5, 5, 1> = TOP.concat_v::<3, 5, 3, 5>(BOTTOM);
    ///     assert_eq!(COMBINED.map, [(0, 0), (0, 1), (0, 2), (0, 3), (0, 4)]);
    /// }
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
        assert!(TROWS == ROWS + BOT_ROWS, "TROWS must equal TOP_ROWS + BOT_ROWS");

        // Derive vertical concat via transpose + horizontal concat + transpose back.
        // Transpose is implemented as rotate_cw + flip_h.
        let top_t = self.rotate_cw().flip_h(); // ROWS cols, COLS rows
        let bot_t = bottom.rotate_cw().flip_h(); // BOT_ROWS cols, COLS rows

        let combined_t: Mapping<TOTAL, COLS, TROWS> =
            top_t.concat_h::<BOTTOM, TOTAL, BOT_ROWS, TROWS>(bot_t);

        combined_t.rotate_cw().flip_h() // transpose back to TROWS x COLS
    }
}
