use crate::{
    error::Error::BitsToIndexesNotEnoughSpace,
    shared_constants::{BitsToIndexes, CELL_COUNT},
};
use core::{array, num::NonZeroU8, ops::BitOrAssign, slice};

use heapless::{LinearMap, Vec};

use crate::{leds::Leds, Result};

#[derive(defmt::Format, Debug)]
pub struct BitMatrix([u8; CELL_COUNT]);

impl BitMatrix {
    pub const fn new(bits: [u8; CELL_COUNT]) -> Self {
        Self(bits)
    }
    pub const fn from_bits(bits: u8) -> Self {
        Self([bits; CELL_COUNT])
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> core::slice::IterMut<'_, u8> {
        self.0.iter_mut()
    }

    pub fn from_chars(chars: &[char; CELL_COUNT]) -> Self {
        let bytes = chars.map(|char| Leds::ASCII_TABLE.get(char as usize).copied().unwrap_or(0));
        Self::new(bytes)
    }

    #[expect(
        clippy::indexing_slicing,
        clippy::integer_division_remainder_used,
        reason = "Indexing and arithmetic are safe: Leds::DIGITS has 10 elements, and (number % 10) is in 0..9. \
        Modulo is required for digit extraction in no_std."
    )]
    pub fn from_number(mut number: u16, padding: u8) -> Self {
        let mut bit_matrix = Self::from_bits(padding);

        for bits in bit_matrix.iter_mut().rev() {
            *bits = Leds::DIGITS[(number % 10) as usize]; // Get the last digit
            number /= 10; // Remove the last digit
            if number == 0 {
                break;
            }
        }
        // If the original number was out of range, turn on all decimal points
        if number > 0 {
            bit_matrix |= Leds::DECIMAL;
        }

        bit_matrix
    }

    pub fn bits_to_indexes(&self) -> Result<BitsToIndexes> {
        let mut acc: BitsToIndexes = LinearMap::new();
        for (index, &bits) in self.iter().enumerate() {
            if let Some(nonzero_bits) = NonZeroU8::new(bits) {
                if let Some(vec) = acc.get_mut(&nonzero_bits) {
                    vec.push(index).map_err(|_| BitsToIndexesNotEnoughSpace)?;
                } else {
                    let vec =
                        Vec::from_slice(&[index]).map_err(|()| BitsToIndexesNotEnoughSpace)?;
                    acc.insert(nonzero_bits, vec)
                        .map_err(|_| BitsToIndexesNotEnoughSpace)?;
                }
            }
        }
        Ok(acc)
    }
}

impl core::str::FromStr for BitMatrix {
    type Err = (); // Replace with a meaningful error type if needed

    /// Parse a string into a `BitMatrix`. If too long, the decimal point will be turned on.
    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let mut bit_matrix = Self::default();

        for (bits, char) in bit_matrix.iter_mut().zip(input.chars()) {
            *bits = Leds::ASCII_TABLE.get(char as usize).copied().ok_or(())?;
        }

        if input.len() > CELL_COUNT {
            bit_matrix |= Leds::DECIMAL;
        }

        Ok(bit_matrix)
    }
}
impl Default for BitMatrix {
    fn default() -> Self {
        Self([0; CELL_COUNT])
    }
}

// Implement `|=` for `BitMatrix`
impl BitOrAssign<u8> for BitMatrix {
    fn bitor_assign(&mut self, rhs: u8) {
        self.0.iter_mut().for_each(|bits| *bits |= rhs);
    }
}

impl IntoIterator for BitMatrix {
    type Item = u8;
    type IntoIter = array::IntoIter<u8, CELL_COUNT>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a BitMatrix {
    type Item = &'a u8;
    type IntoIter = slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut BitMatrix {
    type Item = &'a mut u8;
    type IntoIter = slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

// implement index for BitMatrix and &BitMatrix
impl core::ops::Index<usize> for BitMatrix {
    type Output = u8;

    #[expect(
        clippy::indexing_slicing,
        reason = "Bounds checking is the caller's responsibility."
    )]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

// index that you can assign to
#[expect(
    clippy::indexing_slicing,
    reason = "Bounds checking is the caller's responsibility."
)]
impl core::ops::IndexMut<usize> for BitMatrix {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}
