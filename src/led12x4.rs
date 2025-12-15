//! A device abstraction for 4-character LED matrix displays (12x4 pixels).
//!
//! See [`Led12x4`] for the main usage example.

use crate::{LedStripDevice, Result};
use smart_leds::RGB8;

/// 3×4 font for digits 0..9. Each entry is 4 rows of 3 bits (LSB = rightmost column).
const FONT: [[u8; 4]; 10] = [
    // 0
    [0b111, 0b101, 0b101, 0b111],
    // 1
    [0b010, 0b110, 0b010, 0b111],
    // 2
    [0b110, 0b001, 0b010, 0b111],
    // 3
    [0b111, 0b001, 0b011, 0b111],
    // 4
    [0b101, 0b101, 0b111, 0b001],
    // 5
    [0b111, 0b100, 0b011, 0b111],
    // 6
    [0b100, 0b111, 0b101, 0b111],
    // 7
    [0b111, 0b001, 0b010, 0b100],
    // 8
    [0b111, 0b101, 0b010, 0b111],
    // 9
    [0b111, 0b101, 0b111, 0b001],
];

/// Display size in pixels
pub const COLS: usize = 12;
pub const ROWS: usize = 4;
// cmk isn't this font defined elsewhere?

const LETTER_A: [u8; 4] = [0b111, 0b101, 0b111, 0b101];
const LETTER_B: [u8; 4] = [0b110, 0b111, 0b101, 0b110];
const LETTER_C: [u8; 4] = [0b111, 0b100, 0b100, 0b111];
const LETTER_D: [u8; 4] = [0b110, 0b101, 0b101, 0b110];
const LETTER_E: [u8; 4] = [0b111, 0b110, 0b100, 0b111];
const LETTER_N: [u8; 4] = [0b101, 0b111, 0b111, 0b101];
const LETTER_O: [u8; 4] = [0b111, 0b101, 0b101, 0b111];

/// A device abstraction for a 4-character LED matrix display (12x4 pixels) built on LED strips.
///
/// ```no_run
/// # #![no_std]
/// # use panic_probe as _;
/// # fn main() {}
/// use serials::led12x4::Led12x4;
/// use serials::led_strip::{LedStrip, LedStripStatic, Rgb};
///
/// async fn example() -> serials::Result<()> {
///     static LED_STRIP_STATIC: LedStripStatic<{ serials::led12x4::COLS * serials::led12x4::ROWS }> =
///         LedStrip::new_static();
///     let strip = LedStrip::new(&LED_STRIP_STATIC)?;
///     let mut display = Led12x4::new(strip);
///
///     let red = Rgb::new(32, 0, 0);
///     let green = Rgb::new(0, 32, 0);
///     let blue = Rgb::new(0, 0, 32);
///     let yellow = Rgb::new(32, 32, 0);
///     display.display(['1', '2', '3', '4'], [red, green, blue, yellow]).await?;
///     Ok(())
/// }
/// ```
pub struct Led12x4<T: LedStripDevice<{ COLS * ROWS }>> {
    strip: T,
}

impl<T: LedStripDevice<{ COLS * ROWS }>> Led12x4<T> {
    /// Wrap an existing LED strip controller.
    pub fn new(strip: T) -> Self {
        Self { strip }
    }

    /// Render a fully defined frame to the display.
    pub async fn display_frame(&mut self, frame: &[RGB8; COLS * ROWS]) -> Result<()> {
        self.strip.update_pixels(frame).await?;
        Ok(())
    }

    /// Render four characters with four colors.
    ///
    /// `chars` is an array of 4 characters. Supported:
    /// - `' '` (space) = blank
    /// - `'0'..'9'` = digits from FONT
    /// - `'C'`, `'D'`, `'E'`, `'N'`, `'O'` (and lowercase) = letter glyphs
    /// - any other char = solid 3×4 block
    ///
    /// Builds the entire frame and updates all pixels at once.
    pub async fn display(&mut self, chars: [char; 4], colors: [RGB8; 4]) -> Result<()> {
        let black = RGB8::new(0, 0, 0);
        let mut frame = [black; COLS * ROWS];

        // Build the entire frame
        for (character_index, &character) in chars.iter().enumerate() {
            let color = colors[character_index];
            let base_col = character_index * 3; // leftmost column of this character

            // Render the 3×4 grid for this character
            match Self::glyph_rows(character) {
                Some(rows) => Self::render_glyph(rows, color, base_col, &mut frame, black),
                None => match character {
                    ' ' => {
                        // blank - set all 12 pixels to black
                        for row_index in 0..ROWS {
                            for column_offset in 0..3 {
                                let pixel_index =
                                    Self::xy_to_index(base_col + column_offset, row_index);
                                frame[pixel_index] = black;
                            }
                        }
                    }
                    _ => {
                        // solid 3×4 block
                        for row_index in 0..ROWS {
                            for column_offset in 0..3 {
                                let pixel_index =
                                    Self::xy_to_index(base_col + column_offset, row_index);
                                frame[pixel_index] = color;
                            }
                        }
                    }
                },
            }
        }

        // Update all pixels at once
        self.strip.update_pixels(&frame).await?;
        Ok(())
    }

    /// Display "1234" in red, green, blue, and yellow respectively.
    pub async fn display_1234(&mut self) -> Result<()> {
        let red = RGB8::new(32, 0, 0);
        let green = RGB8::new(0, 32, 0);
        let blue = RGB8::new(0, 0, 32);
        let yellow = RGB8::new(32, 32, 0);

        self.display(['1', '2', '3', '4'], [red, green, blue, yellow])
            .await
    }

    fn glyph_rows(character: char) -> Option<[u8; 4]> {
        match character {
            '0'..='9' => Some(FONT[(character as u8 - b'0') as usize]),
            'A' | 'a' => Some(LETTER_A),
            'B' | 'b' => Some(LETTER_B),
            'C' | 'c' => Some(LETTER_C),
            'D' | 'd' => Some(LETTER_D),
            'E' | 'e' => Some(LETTER_E),
            'N' | 'n' => Some(LETTER_N),
            'O' | 'o' => Some(LETTER_O),
            _ => None,
        }
    }

    fn render_glyph(
        rows: [u8; 4],
        color: RGB8,
        base_col: usize,
        frame: &mut [RGB8; COLS * ROWS],
        black: RGB8,
    ) {
        for row_index in 0..ROWS {
            let row_bits = rows[row_index];
            for column_offset in 0..3 {
                let bit = (row_bits >> (2 - column_offset)) & 1;
                let pixel_index = Self::xy_to_index(base_col + column_offset, row_index);
                let pixel_color = if bit != 0 { color } else { black };
                frame[pixel_index] = pixel_color;
            }
        }
    }

    #[inline]
    fn xy_to_index(x: usize, y: usize) -> usize {
        // Column-major with serpentine: even columns go down (top-to-bottom), odd columns go up (bottom-to-top)
        if x % 2 == 0 {
            // Even column: top-to-bottom
            x * ROWS + y
        } else {
            // Odd column: bottom-to-top (reverse y)
            x * ROWS + (ROWS - 1 - y)
        }
    }
}
