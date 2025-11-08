//! BitMatrix - Represents LED display state for 4-digit 7-segment displays

use core::num::NonZeroU8;
use core::ops::{BitOrAssign, Index, IndexMut};
use heapless::Vec;

use crate::led_4seg::{BitsToIndexes, Leds, Text, CELL_COUNT, CELL_COUNT_U8};
use crate::Result;
use crate::error::Error;

#[derive(defmt::Format, Debug, Clone)]
pub struct BitMatrix4Led([u8; CELL_COUNT]);

impl BitMatrix4Led {
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

    pub fn from_text(text: &Text) -> Self {
        let bytes = text.map(|char| Leds::ASCII_TABLE.get(char as usize).copied().unwrap_or(0));
        Self::new(bytes)
    }

    #[expect(
        clippy::indexing_slicing,
        clippy::integer_division_remainder_used,
        reason = "Indexing and arithmetic are safe; modulo is required for digit extraction"
    )]
    pub fn from_number(mut number: u16, padding: u8) -> Self {
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

impl Default for BitMatrix4Led {
    fn default() -> Self {
        Self([0; CELL_COUNT])
    }
}

impl BitOrAssign<u8> for BitMatrix4Led {
    fn bitor_assign(&mut self, rhs: u8) {
        self.iter_mut().for_each(|bits| *bits |= rhs);
    }
}

impl Index<usize> for BitMatrix4Led {
    type Output = u8;

    #[expect(clippy::indexing_slicing, reason = "Caller's responsibility")]
    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

impl IndexMut<usize> for BitMatrix4Led {
    #[expect(clippy::indexing_slicing, reason = "Caller's responsibility")]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

impl IntoIterator for BitMatrix4Led {
    type Item = u8;
    type IntoIter = core::array::IntoIter<u8, CELL_COUNT>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a> IntoIterator for &'a BitMatrix4Led {
    type Item = &'a u8;
    type IntoIter = core::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut BitMatrix4Led {
    type Item = &'a mut u8;
    type IntoIter = core::slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}
