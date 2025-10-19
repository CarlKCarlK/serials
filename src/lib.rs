//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd;
mod error;
mod ir_nec;
mod rfid;
pub mod servo;

// Re-export commonly used items
pub use char_lcd::{CharLcd, LcdChannel, LcdMessage};
pub use error::{Error, Result};
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use rfid::{RfidEvent, RfidNotifier, RfidCommandChannel, RfidChannels, RfidReader};
pub use servo::Servo;
