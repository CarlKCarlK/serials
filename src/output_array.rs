use core::convert::Infallible;
use embassy_rp::gpio::{self, Level};

pub struct OutputArray<'a, const N: usize>([gpio::Output<'a>; N]);

impl<'a, const N: usize> OutputArray<'a, N> {
    pub fn new(outputs: [gpio::Output<'a>; N]) -> Self {
        Self(outputs)
    }

    #[inline]
    pub fn set_levels_at_indexes(&mut self, indexes: &[usize], level: Level) {
        for &index in indexes {
            self.0[index].set_level(level);
        }
    }
}

impl OutputArray<'_, { u8::BITS as usize }> {
    #[inline]
    #[must_use = "Possible error result should not be ignored"]
    // On some hardware (but not here), setting a bit can fail, so we return a Result
    pub fn set_from_bits(&mut self, mut bits: u8) -> Result<(), Infallible> {
        for output in &mut self.0 {
            let level: Level = ((bits & 1) == 1).into();
            output.set_level(level);
            bits >>= 1;
        }
        Ok(())
    }
}
