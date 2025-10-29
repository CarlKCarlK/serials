//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd;
pub mod clock;
mod error;
pub mod led_strip;
pub mod led_24x4;
mod ir_nec;
mod rfid;
pub mod servo;
pub mod unix_seconds;
#[cfg(feature = "wifi")]
pub mod time_sync;
#[cfg(feature = "wifi")]
pub mod wifi;

// Re-export commonly used items
pub use char_lcd::{CharLcd, CharLcdNotifier, CharLcdMessage};
pub use clock::{Clock, ClockCommand, ClockNotifier, ClockEvent, ClockState};
pub use error::{Error, Result};
pub use led_strip::{LedStrip, LedStripNotifier, Rgb, PioBus};
pub use led_24x4::Led24x4;
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use rfid::{RfidEvent, RfidNotifier, Rfid};
pub use servo::Servo;
pub use unix_seconds::UnixSeconds;
pub use smart_leds::colors;
#[cfg(feature = "wifi")]
pub use time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
#[cfg(feature = "wifi")]
pub use wifi::{Wifi, WifiEvent, WifiNotifier};

// Re-export macros (they're already at crate root due to #[macro_export])
// define_led_strips is available as lib::define_led_strips!
