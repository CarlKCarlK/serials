use crate::error::Error::BitsToIndexesNotEnoughSpace;
use core::ops::BitOrAssign;

use heapless::{LinearMap, Vec};

use crate::{error::Error, leds::Leds};

#[derive(defmt::Format, Debug)]
pub struct BitMatrix<const CELL_COUNT: usize>([u8; CELL_COUNT]);

impl<const CELL_COUNT: usize> BitMatrix<CELL_COUNT> {
    pub fn new(bits: [u8; CELL_COUNT]) -> Self {
        Self(bits)
    }
    pub fn from_bits(bits: u8) -> Self {
        Self([bits; CELL_COUNT])
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> core::slice::IterMut<u8> {
        self.0.iter_mut()
    }

    // If too long, turn on all decimal points
    pub fn from_str<S: AsRef<str>>(str: S) -> Self {
        let str = str.as_ref();

        let mut bit_matrix = BitMatrix::default();
        for (bits, c) in bit_matrix.iter_mut().zip(str.chars()) {
            *bits = Leds::ASCII_TABLE[c as usize];
        }

        if str.len() > CELL_COUNT {
            bit_matrix |= Leds::DECIMAL;
        }

        bit_matrix
    }

    pub fn from_chars(chars: &[char; CELL_COUNT]) -> Self {
        let bytes = chars.map(|c| Leds::ASCII_TABLE[c as usize]);
        Self::new(bytes)
    }

    pub fn from_number(mut number: u16, padding: u8) -> Self {
        let mut bit_matrix = BitMatrix::from_bits(padding);

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

    pub fn bits_to_indexes(&self) -> Result<BitsToIndexes<CELL_COUNT>, Error> {
        let mut acc: BitsToIndexes<CELL_COUNT> = LinearMap::new();
        for (index, &bits) in self.iter().enumerate().filter(|(_, &bits)| bits != 0) {
            if let Some(vec) = acc.get_mut(&bits) {
                vec.push(index).map_err(|_| BitsToIndexesNotEnoughSpace)?;
            } else {
                let vec = Vec::from_slice(&[index]).map_err(|()| BitsToIndexesNotEnoughSpace)?;
                acc.insert(bits, vec)
                    .map_err(|_| BitsToIndexesNotEnoughSpace)?;
            }
        }
        Ok(acc)
    }
}

impl<const CELL_COUNT: usize> Default for BitMatrix<CELL_COUNT> {
    fn default() -> Self {
        Self([0; CELL_COUNT])
    }
}

// Implement `|=` for `BitMatrix`
impl<const CELL_COUNT: usize> BitOrAssign<u8> for BitMatrix<CELL_COUNT> {
    fn bitor_assign(&mut self, rhs: u8) {
        self.0.iter_mut().for_each(|bits| *bits |= rhs);
    }
}

impl<const CELL_COUNT: usize> IntoIterator for BitMatrix<CELL_COUNT> {
    type Item = u8;
    type IntoIter = core::array::IntoIter<u8, CELL_COUNT>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, const CELL_COUNT: usize> IntoIterator for &'a BitMatrix<CELL_COUNT> {
    type Item = &'a u8;
    type IntoIter = core::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a, const CELL_COUNT: usize> IntoIterator for &'a mut BitMatrix<CELL_COUNT> {
    type Item = &'a mut u8;
    type IntoIter = core::slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

// implement index for BitMatrix and &BitMatrix
impl<const CELL_COUNT: usize> core::ops::Index<usize> for BitMatrix<CELL_COUNT> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

// index that you can assign to
impl<const CELL_COUNT: usize> core::ops::IndexMut<usize> for BitMatrix<CELL_COUNT> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

// cmk move
type BitsToIndexes<const CELL_COUNT: usize> = LinearMap<u8, Vec<usize, CELL_COUNT>, CELL_COUNT>;
