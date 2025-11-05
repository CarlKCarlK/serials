//! Shared items for the clock project.
#![no_std]
#![no_main]

mod button;
mod char_lcd;
pub mod clock;
pub mod clock_4led;
mod error;
mod ir_nec;
pub mod led_24x4;
pub mod led_4seg;
pub mod led_strip;
mod rfid;
pub mod servo;
#[cfg(feature = "wifi")]
pub mod time_sync;
pub mod unix_seconds;
#[cfg(feature = "wifi")]
pub mod wifi;

// Re-export commonly used items
pub use button::{Button, PressDuration, BUTTON_DEBOUNCE_DELAY, LONG_PRESS_DURATION};
pub use char_lcd::{CharLcd, CharLcdMessage, CharLcdNotifier};
pub use clock::{Clock, ClockCommand, ClockEvent, ClockNotifier, ClockState};
pub use clock_4led::{Clock4Led, Clock4LedNotifier, ClockCommand as Clock4LedCommand, ClockState as Clock4LedState};
pub use error::{Error, Result};
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use led_24x4::Led24x4;
pub use led_4seg::{Led4Seg, Led4SegNotifier, BlinkState, Text as Led4SegText, OutputArray};
pub use led_strip::{LedStrip, LedStripNotifier, PioBus, Rgb};
pub use rfid::{Rfid, RfidEvent, RfidNotifier};
pub use servo::Servo;
pub use smart_leds::colors;
#[cfg(feature = "wifi")]
pub use time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
pub use unix_seconds::UnixSeconds;
#[cfg(feature = "wifi")]
pub use wifi::{Wifi, WifiEvent, WifiNotifier};

// Re-export macros (they're already at crate root due to #[macro_export])
// define_led_strips is available as lib::define_led_strips!
