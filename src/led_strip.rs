//! A device abstraction for WS2812-style LED strips.
//!
//! See [`LedStrip`], [`led_strip!`] for single strips, and [`led_strips!`] for managing multiple strips on one PIO.

pub mod gamma;

include!("led_strip/strip.rs");
#[doc(inline)]
pub use smart_leds::colors;

pub use led_strip;
pub use led_strips;

/// Used by [`led_strips!`] to budget current for LED strips.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Current {
    Milliamps(u16),
    Unlimited,
}

pub use Current::Milliamps;
pub use Current::Unlimited;

impl Current {
    /// Calculate maximum brightness based on current budget and worst-case current draw.
    ///
    /// Returns 255 (full brightness) for Unlimited, or a scaled value for Milliamps.
    #[must_use]
    pub const fn max_brightness(self, worst_case_ma: u32) -> u8 {
        assert!(worst_case_ma > 0, "worst_case_ma must be positive");
        match self {
            Self::Milliamps(ma) => {
                let scale = (ma as u32 * 255) / worst_case_ma;
                if scale > 255 { 255 } else { scale as u8 }
            }
            Self::Unlimited => 255,
        }
    }
}
