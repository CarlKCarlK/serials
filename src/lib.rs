//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd;
pub mod clock;
mod error;
mod ir_nec;
mod rfid;
pub mod servo;
pub mod time_sync;
pub mod unix_seconds;
pub mod wifi;

// Re-export commonly used items
pub use char_lcd::{CharLcd, LcdChannel, LcdMessage};
pub use clock::{Clock, ClockCommand, ClockNotifier, TimeInfo, TimeState};
pub use error::{Error, Result};
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use rfid::{RfidEvent, RfidNotifier, RfidCommandChannel, RfidChannels, RfidReader};
pub use servo::Servo;
pub use time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
pub use unix_seconds::UnixSeconds;
pub use wifi::{StackStorage, Wifi, WifiEvent, WifiNotifier};
