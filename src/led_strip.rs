//! A device abstraction for WS2812-style LED strips.
//!
//! See [`LedStrip`] and [`define_led_strips!`] for managing strips on a PIO.

pub mod gamma;

include!("led_strip/strip.rs");
// See [`LedStrip`] and [`define_led_strips!`] for multi-strip setups on one PIO.
#[doc(inline)]
pub use smart_leds::colors;

/// Used by [`define_led_strips!`] to budget current for LED strips.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct Milliamps(pub u16);

impl Milliamps {
    #[must_use]
    pub const fn as_u32(self) -> u32 {
        self.0 as u32
    }
}
