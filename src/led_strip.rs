//! A device abstraction for WS2812-style LED strips.
//!
//! See [`LedStripShared`] and [`define_led_strips_shared!`] for managing strips on a PIO.

pub mod gamma;

include!("led_strip/led_strip_shared.rs");
// See [`LedStripShared`] and [`define_led_strips_shared!`] for multi-strip setups on one PIO.
#[doc(inline)]
pub use smart_leds::colors;

/// Used by [`define_led_strips_shared!`] to budget current for LED strips.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Milliamps(pub u16);

impl Milliamps {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }
}
