//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd_i2c;
mod error;
mod hardware;
mod ir_nec;
mod lcd_async;
mod never;
mod output_array;
pub mod servo;
mod shared_constants;
mod spi_mfrc522;

// Re-export commonly used items
pub use char_lcd_i2c::CharLcdI2c;
pub use error::{Error, Result};
pub use hardware::Hardware;
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use lcd_async::{AsyncLcd, LcdChannel, LcdMessage};
pub use never::Never;
pub use servo::Servo;
pub use shared_constants::*;
pub use spi_mfrc522::{RfidEvent, SpiMfrc522Notifier, SpiMfrc522CommandChannel,
    SpiMfrc522Channels, SpiMfrc522Reader
};
