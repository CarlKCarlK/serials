//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd;
pub mod clock;
mod error;
#[cfg(feature = "led-strip")]
pub mod led_strip;
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
#[cfg(feature = "led-strip")]
pub use led_strip::{LedStrip, LedStripNotifier, Rgb};
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use rfid::{RfidEvent, RfidNotifier, Rfid};
pub use servo::Servo;
pub use unix_seconds::UnixSeconds;
#[cfg(feature = "wifi")]
pub use time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
#[cfg(feature = "wifi")]
pub use wifi::{Wifi, WifiEvent, WifiNotifier};
