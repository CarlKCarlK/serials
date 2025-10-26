//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd;
pub mod clock;
mod error;
pub mod led_strip;
mod ir_nec;
mod rfid;
pub mod servo;
pub mod time_sync;
pub mod unix_seconds;
pub mod wifi;

// Re-export commonly used items
pub use char_lcd::{CharLcd, CharLcdNotifier, CharLcdMessage};
pub use clock::{Clock, ClockCommand, ClockNotifier, ClockEvent, ClockState};
pub use error::{Error, Result};
pub use led_strip::{LedStrip, LedStripNotifier, Rgb};
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use rfid::{RfidEvent, RfidNotifier, Rfid};
pub use servo::Servo;
pub use time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
pub use unix_seconds::UnixSeconds;
pub use wifi::{Wifi, WifiEvent, WifiNotifier};
