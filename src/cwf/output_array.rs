use crate::Result;
use crate::error::Error::IndexOutOfBounds;
use core::num::NonZeroU8;
use embassy_rp::gpio::{self, Level};

pub struct OutputArray<'a, const N: usize>([gpio::Output<'a>; N]);

impl<'a, const N: usize> OutputArray<'a, N> {
    pub const fn new(outputs: [gpio::Output<'a>; N]) -> Self {
        Self(outputs)
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &gpio::Output<'a>> {
        self.0.iter()
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut gpio::Output<'a>> {
        self.0.iter_mut()
    }

    #[inline]
    pub fn set_levels_at_indexes(&mut self, indexes: &[u8], level: Level) -> Result<()> {
        for &index in indexes {
            self.set_level_at_index(index, level)?;
        }
        Ok(())
    }

    #[inline]
    pub fn set_level_at_index(&mut self, index: u8, level: Level) -> Result<()> {
        self.get_mut(index as usize)
            .ok_or(IndexOutOfBounds)?
            .set_level(level);
        Ok(())
    }

    #[inline]
    pub fn get_mut(&mut self, index: usize) -> Option<&mut gpio::Output<'a>> {
        self.0.get_mut(index)
    }
}

impl OutputArray<'_, { u8::BITS as usize }> {
    #[expect(clippy::shadow_reuse, reason = "Converting NonZeroU8 to u8")]
    #[inline]
    pub fn set_from_nonzero_bits(&mut self, bits: NonZeroU8) {
        let mut bits = bits.get();
        for output in &mut self.0 {
            let level: Level = ((bits & 1) == 1).into();
            output.set_level(level);
            bits >>= 1;
        }
    }

    #[inline]
    pub fn set_from_bits(&mut self, mut bits: u8) {
        for output in &mut self.0 {
            let level: Level = ((bits & 1) == 1).into();
            output.set_level(level);
            bits >>= 1;
        }
    }
}
