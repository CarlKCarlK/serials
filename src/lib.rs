//! Shared items for the clock project.
#![no_std]
#![no_main]

mod char_lcd_i2c;
mod error;
mod hardware;
mod never;
mod output_array;
mod shared_constants;
mod spi_mfrc522;

// Re-export commonly used items
pub use char_lcd_i2c::CharLcdI2c;
pub use error::{Error, Result};
pub use hardware::Hardware;
pub use never::Never;
pub use shared_constants::*;
pub use spi_mfrc522::new_spi_mfrc522;
