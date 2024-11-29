use crate::error::Error::IndexOutOfBounds;
use crate::Result;
use core::num::NonZeroU8;
use embassy_rp::gpio::{self, Level};

pub struct OutputArray<'a, const N: usize>([gpio::Output<'a>; N]);

impl<'a, const N: usize> OutputArray<'a, N> {
    pub const fn new(outputs: [gpio::Output<'a>; N]) -> Self {
        Self(outputs)
    }

    #[inline]
    pub fn set_levels_at_indexes(&mut self, indexes: &[usize], level: Level) -> Result<()> {
        for &index in indexes {
            self.0
                .get_mut(index) // Mutable access
                .ok_or(IndexOutOfBounds)? // Return error if index is out of bounds
                .set_level(level); // Mutate the item
        }
        Ok(())
    }
}

impl OutputArray<'_, { u8::BITS as usize }> {
    #[expect(clippy::shadow_reuse, reason = "Just converting a NonZeroU8 to a u8.")]
    #[inline]
    pub fn set_from_bits(&mut self, bits: NonZeroU8) {
        let mut bits = bits.get();
        for output in &mut self.0 {
            let level: Level = ((bits & 1) == 1).into();
            output.set_level(level);
            bits >>= 1;
        }
    }
}
