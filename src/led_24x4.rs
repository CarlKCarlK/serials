//! Virtual 4-character 3x4-font display built on top of `led_strip`.
//!
//! The display maps four characters (3×4 pixels each) onto a 12×4 LED strip (row-major).

use crate::{led_strip::LedStrip, Result};
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

pub struct Led24x4 {
    strip: LedStrip<{ COLS * ROWS }>,
}

impl Led24x4 {
    /// Wrap an existing `LedStrip<12*4>` controller.
    pub fn new(strip: LedStrip<{ COLS * ROWS }>) -> Self {
        Self { strip }
    }

    /// Render four characters with four colors.
    ///
    /// `chars` is an array of 4 characters. Supported:
    /// - `' '` (space) = blank
    /// - `'0'..'9'` = digits from FONT
    /// - any other char = solid 3×4 block
    ///
    /// Builds the entire frame and updates all pixels at once.
    pub async fn display(&mut self, chars: [char; 4], colors: [RGB8; 4]) -> Result<()> {
        let black = RGB8::new(0, 0, 0);
        let mut frame = [black; COLS * ROWS];

        // Build the entire frame
        for (ch_i, &ch) in chars.iter().enumerate() {
            let color = colors[ch_i];
            let base_col = ch_i * 3; // leftmost column of this character

            // Render the 3×4 grid for this character
            match ch {
                '0'..='9' => {
                    let digit = (ch as u8 - b'0') as usize;
                    for row in 0..ROWS {
                        let row_bits = FONT[digit][row];
                        for col in 0..3 {
                            let bit = (row_bits >> (2 - col)) & 1;
                            let idx = Self::xy_to_index(base_col + col, row);
                            let pixel_color = if bit != 0 { color } else { black };
                            frame[idx] = pixel_color;
                        }
                    }
                }
                ' ' => {
                    // blank - set all 12 pixels to black
                    for row in 0..ROWS {
                        for col in 0..3 {
                            let idx = Self::xy_to_index(base_col + col, row);
                            frame[idx] = black;
                        }
                    }
                }
                _ => {
                    // solid 3×4 block
                    for row in 0..ROWS {
                        for col in 0..3 {
                            let idx = Self::xy_to_index(base_col + col, row);
                            frame[idx] = color;
                        }
                    }
                }
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
        
        self.display(['1', '2', '3', '4'], [red, green, blue, yellow]).await
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
