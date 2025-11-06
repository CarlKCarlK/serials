use crate::Result;
use crate::cwf::Leds;
use crate::cwf::blinker::Text;
use crate::cwf::shared_constants::{BitsToIndexes, CELL_COUNT, CELL_COUNT_U8};
use crate::error::Error::BitsToIndexesFull;
use core::{array, num::NonZeroU8, ops::BitOrAssign, slice};

#[derive(defmt::Format, Debug, Clone)]
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

    pub fn from_text(text: &Text) -> Self {
        let bytes = text.map(|ch| Leds::ASCII_TABLE.get(ch as usize).copied().unwrap_or(0));
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
                    vec.push(index).map_err(|_| BitsToIndexesFull)?;
                } else {
                    let vec = heapless::Vec::from_slice(&[index]).map_err(|_| BitsToIndexesFull)?;
                    bits_to_index
                        .insert(nonzero_bits, vec)
                        .map_err(|_| BitsToIndexesFull)?;
                }
            }
        }
        Ok(())
    }
}

impl Default for BitMatrix {
    fn default() -> Self {
        Self([0; CELL_COUNT])
    }
}

impl BitOrAssign<u8> for BitMatrix {
    fn bitor_assign(&mut self, rhs: u8) {
        self.iter_mut().for_each(|bits| *bits |= rhs);
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

impl core::ops::Index<u8> for BitMatrix {
    type Output = u8;

    #[expect(clippy::indexing_slicing, reason = "Caller ensures bounds")]
    fn index(&self, index: u8) -> &Self::Output {
        &self.0[index as usize]
    }
}

impl core::ops::IndexMut<u8> for BitMatrix {
    #[expect(clippy::indexing_slicing, reason = "Caller ensures bounds")]
    fn index_mut(&mut self, index: u8) -> &mut Self::Output {
        &mut self.0[index as usize]
    }
}
