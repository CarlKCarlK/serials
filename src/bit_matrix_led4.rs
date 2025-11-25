//! Internal segment state representation for 4-digit 7-segment displays.

use core::num::NonZeroU8;
use core::ops::{BitOrAssign, Index, IndexMut};
use heapless::{LinearMap, Vec};

#[cfg(not(feature = "host"))]
use crate::{Error, Result};

// Define simple error types for host testing
#[cfg(feature = "host")]
#[derive(Debug)]
pub enum Error {
    BitsToIndexesFull,
}

#[cfg(feature = "host")]
pub type Result<T> = core::result::Result<T, Error>;

/// Number of digits in the display.
const CELL_COUNT: usize = 4;
const CELL_COUNT_U8: u8 = CELL_COUNT as u8;

/// Internal type for optimizing multiplexing by grouping digits with identical segment patterns.
///
/// Maps from segment bit patterns to the indexes of digits that share that pattern.
#[doc(hidden)]
pub type BitsToIndexes = LinearMap<NonZeroU8, Vec<u8, CELL_COUNT>, CELL_COUNT>;

// ============================================================================
// LED Constants
// ============================================================================

/// Constants for 7-segment LED displays.
struct Leds;

impl Leds {
    /// Segment A of the 7-segment display.
    const SEG_A: u8 = 0b_0000_0001;
    /// Segment B of the 7-segment display.
    const SEG_B: u8 = 0b_0000_0010;
    /// Segment C of the 7-segment display.
    const SEG_C: u8 = 0b_0000_0100;
    /// Segment D of the 7-segment display.
    const SEG_D: u8 = 0b_0000_1000;
    /// Segment E of the 7-segment display.
    const SEG_E: u8 = 0b_0001_0000;
    /// Segment F of the 7-segment display.
    const SEG_F: u8 = 0b_0010_0000;

    #[cfg_attr(not(test), allow(dead_code))]
    /// Array representing the segments for digits 0-9 on a 7-segment display.
    const DIGITS: [u8; 10] = [
        0b_0011_1111, // Digit 0
        0b_0000_0110, // Digit 1
        0b_0101_1011, // Digit 2
        0b_0100_1111, // Digit 3
        0b_0110_0110, // Digit 4
        0b_0110_1101, // Digit 5
        0b_0111_1101, // Digit 6
        0b_0000_0111, // Digit 7
        0b_0111_1111, // Digit 8
        0b_0110_1111, // Digit 9
    ];

    #[cfg_attr(not(test), allow(dead_code))]
    /// Decimal point of the 7-segment display.
    const DECIMAL: u8 = 0b_1000_0000;

    /// ASCII table mapping characters to their 7-segment display representations.
    const ASCII_TABLE: [u8; 128] = [
        // Control characters (0-31) + space (32)
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        // Symbols (33-47)
        0b_1000_0110,              // !
        Self::SEG_A | Self::SEG_B, // "
        0b_0000_0000,              // #
        0b_0000_0000,              // $
        0b_0000_0000,              // %
        0b_0000_0000,              // &
        Self::SEG_A,               // '
        Self::SEG_A | Self::SEG_F, // (
        Self::SEG_C | Self::SEG_D, // )
        Self::SEG_D | Self::SEG_E, // *
        0b_0000_0000,              // +
        0b_0000_0000,              // ,
        0b_0100_0000,              // -
        0b_1000_0000,              // .
        0b_0000_0000,              // /
        // Numbers (48-57)
        0b_0011_1111,
        0b_0000_0110,
        0b_0101_1011,
        0b_0100_1111,
        0b_0110_0110,
        0b_0110_1101,
        0b_0111_1101,
        0b_0000_0111,
        0b_0111_1111,
        0b_0110_1111,
        // Symbols (58-64)
        0b_0000_0000,              // :
        0b_0000_0000,              // ;
        Self::SEG_E | Self::SEG_F, // <
        0b_0000_0000,              // =
        Self::SEG_B | Self::SEG_C, // >
        0b_0000_0000,              // ?
        0b_0000_0000,              // @
        // Uppercase letters (65-90)
        0b_0111_0111,
        0b_0111_1100,
        0b_0011_1001,
        0b_0101_1110,
        0b_0111_1001,
        0b_0111_0001,
        0b_0011_1101,
        0b_0111_0110,
        0b_0000_0110,
        0b_0001_1110,
        0b_0111_0110,
        0b_0011_1000,
        0b_0001_0101,
        0b_0101_0100,
        0b_0011_1111,
        0b_0111_0011,
        0b_0110_0111,
        0b_0101_0000,
        0b_0110_1101,
        0b_0111_1000,
        0b_0011_1110,
        0b_0010_1010,
        0b_0001_1101,
        0b_0111_0110,
        0b_0110_1110,
        0b_0101_1011,
        // Symbols (91-96)
        0b_0011_1001,
        0b_0000_0000,
        0b_0000_1111,
        0b_0000_0000,
        0b_0000_1000,
        0b_0000_0000,
        // Lowercase letters (97-122)
        0b_0111_0111,
        0b_0111_1100,
        0b_0011_1001,
        0b_0101_1110,
        0b_0111_1001,
        0b_0111_0001,
        0b_0011_1101,
        0b_0111_0100,
        0b_0000_0110,
        0b_0001_1110,
        0b_0111_0110,
        0b_0011_1000,
        0b_0001_0101,
        0b_0101_0100,
        0b_0011_1111,
        0b_0111_0011,
        0b_0110_0111,
        0b_0101_0000,
        0b_0110_1101,
        0b_0111_1000,
        0b_0011_1110,
        0b_0010_1010,
        0b_0001_1101,
        0b_0111_0110,
        0b_0110_1110,
        0b_0101_1011,
        // Symbols (123-127)
        0b_0011_1001,
        0b_0000_0110,
        0b_0000_1111,
        0b_0100_0000,
        0b_0000_0000,
    ];
}

// ============================================================================
// BitMatrixLed4
// ============================================================================

/// LED segment state for a 4-digit 7-segment display.
///
/// Represents the raw bit patterns for LED segments.
#[cfg_attr(feature = "defmt", derive(defmt::Format))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BitMatrixLed4([u8; CELL_COUNT]);

impl BitMatrixLed4 {
    pub(crate) const fn new(bits: [u8; CELL_COUNT]) -> Self {
        Self(bits)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn from_bits(bits: u8) -> Self {
        Self([bits; CELL_COUNT])
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.0.iter()
    }

    pub(crate) fn iter_mut(&mut self) -> core::slice::IterMut<'_, u8> {
        self.0.iter_mut()
    }

    pub fn from_text(text: &[char; 4]) -> Self {
        let bytes = text.map(|char| Leds::ASCII_TABLE.get(char as usize).copied().unwrap_or(0));
        Self::new(bytes)
    }

    #[expect(
        clippy::indexing_slicing,
        clippy::integer_division_remainder_used,
        reason = "Indexing and arithmetic are safe; modulo is required for digit extraction"
    )]
    /// Creates bit matrix from number. If overflow, lights decimal points.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn from_number(mut number: u16, padding: u8) -> Self {
        let mut bit_matrix = Self::from_bits(padding);

        for bits in bit_matrix.iter_mut().rev() {
            *bits = Leds::DIGITS[(number % 10) as usize];
            number /= 10;
            if number == 0 {
                break;
            }
        }
        if number > 0 {
            bit_matrix |= Leds::DECIMAL;
        }

        bit_matrix
    }

    /// Converts to optimized index mapping for multiplexing.
    #[doc(hidden)]
    pub fn bits_to_indexes(&self, bits_to_index: &mut BitsToIndexes) -> Result<()> {
        bits_to_index.clear();
        for (&bits, index) in self.iter().zip(0..CELL_COUNT_U8) {
            if let Some(nonzero_bits) = NonZeroU8::new(bits) {
                if let Some(vec) = bits_to_index.get_mut(&nonzero_bits) {
                    vec.push(index).map_err(|_| Error::BitsToIndexesFull)?;
                } else {
                    let vec = Vec::from_slice(&[index]).map_err(|_| Error::BitsToIndexesFull)?;
                    bits_to_index
                        .insert(nonzero_bits, vec)
                        .map_err(|_| Error::BitsToIndexesFull)?;
                }
            }
        }
        Ok(())
    }
}

impl Default for BitMatrixLed4 {
    fn default() -> Self {
        Self([0; CELL_COUNT])
    }
}

impl BitOrAssign<u8> for BitMatrixLed4 {
    fn bitor_assign(&mut self, rhs: u8) {
        self.iter_mut().for_each(|bits| *bits |= rhs);
    }
}

impl Index<usize> for BitMatrixLed4 {
    type Output = u8;

    #[expect(clippy::indexing_slicing, reason = "Caller's responsibility")]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for BitMatrixLed4 {
    #[expect(clippy::indexing_slicing, reason = "Caller's responsibility")]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl IntoIterator for BitMatrixLed4 {
    type Item = u8;
    type IntoIter = core::array::IntoIter<u8, CELL_COUNT>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a BitMatrixLed4 {
    type Item = &'a u8;
    type IntoIter = core::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut BitMatrixLed4 {
    type Item = &'a mut u8;
    type IntoIter = core::slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::*;

    #[test]
    fn test_from_bits() {
        // Creates array with same bits in all positions
        let matrix = BitMatrixLed4::from_bits(0b_0011_1111);
        assert_eq!(matrix[0], 0b_0011_1111);
        assert_eq!(matrix[1], 0b_0011_1111);
        assert_eq!(matrix[2], 0b_0011_1111);
        assert_eq!(matrix[3], 0b_0011_1111);
    }

    #[test]
    fn test_from_number() {
        // Converts number to digit segments (1234)
        let matrix = BitMatrixLed4::from_number(1234, 0);
        assert_eq!(matrix[0], 0b_0000_0110); // '1'
        assert_eq!(matrix[1], 0b_0101_1011); // '2'
        assert_eq!(matrix[2], 0b_0100_1111); // '3'
        assert_eq!(matrix[3], 0b_0110_0110); // '4'
    }

    #[test]
    fn test_from_number_overflow() {
        // Number > 9999 should light all decimal points
        let matrix = BitMatrixLed4::from_number(12345, 0);
        for &bits in matrix.iter() {
            assert_ne!(bits & 0b_1000_0000, 0, "Decimal point should be lit");
        }
    }

    #[test]
    fn test_from_text() {
        // Converts characters to segments
        let matrix = BitMatrixLed4::from_text(&['A', 'b', 'C', 'd']);
        assert_eq!(matrix[0], 0b_0111_0111); // 'A'
        assert_eq!(matrix[1], 0b_0111_1100); // 'b'
        assert_eq!(matrix[2], 0b_0011_1001); // 'C'
        assert_eq!(matrix[3], 0b_0101_1110); // 'd'
    }

    #[test]
    fn test_bits_to_indexes() {
        // Optimizes multiplexing by grouping identical patterns
        // For "1221", digit '1' appears at positions 0 and 3, digit '2' at positions 1 and 2
        let matrix = BitMatrixLed4::from_number(1221, 0);
        let mut bits_to_index = BitsToIndexes::new();

        matrix
            .bits_to_indexes(&mut bits_to_index)
            .expect("Should succeed");

        // Should have exactly 2 unique patterns
        assert_eq!(
            bits_to_index.len(),
            2,
            "Should have 2 unique digit patterns"
        );

        // Check that the grouping is correct
        let pattern_1 = NonZeroU8::new(0b_0000_0110).unwrap(); // '1'
        let pattern_2 = NonZeroU8::new(0b_0101_1011).unwrap(); // '2'

        if let Some(indexes) = bits_to_index.get(&pattern_1) {
            assert_eq!(indexes.len(), 2, "Pattern '1' should appear twice");
            assert!(indexes.contains(&0), "Should include index 0");
            assert!(indexes.contains(&3), "Should include index 3");
        } else {
            panic!("Pattern '1' not found");
        }

        if let Some(indexes) = bits_to_index.get(&pattern_2) {
            assert_eq!(indexes.len(), 2, "Pattern '2' should appear twice");
            assert!(indexes.contains(&1), "Should include index 1");
            assert!(indexes.contains(&2), "Should include index 2");
        } else {
            panic!("Pattern '2' not found");
        }
    }

    #[test]
    fn test_char_to_led_special_chars() {
        // Test some special character mappings
        let matrix = BitMatrixLed4::from_text(&[' ', '-', '_', '.']);
        assert_eq!(matrix[0], 0b_0000_0000); // space
        assert_eq!(matrix[1], 0b_0100_0000); // '-'
        assert_eq!(matrix[2], 0b_0000_1000); // '_'
        assert_eq!(matrix[3], 0b_1000_0000); // '.'
    }
}
