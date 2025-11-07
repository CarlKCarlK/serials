//! Shared items for the clock project.
#![no_std]
#![no_main]

mod bit_matrix;
mod blink_state;
mod button;
mod char_lcd;
pub mod clock;
pub mod clock_4led;
pub mod clock_offset_store;
#[cfg(feature = "wifi")]
pub mod credential_store;
pub mod cwf;
#[cfg(feature = "wifi")]
mod dhcp_server;
#[cfg(feature = "wifi")]
mod dns_server;
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
#[cfg(feature = "wifi")]
pub mod wifi_config;
// Re-export commonly used items
pub use bit_matrix::BitMatrix;
pub use blink_state::BlinkState;
pub use button::{BUTTON_DEBOUNCE_DELAY, Button, LONG_PRESS_DURATION, PressDuration};
pub use char_lcd::{CharLcd, CharLcdMessage, CharLcdNotifier};
pub use clock::{Clock, ClockCommand, ClockEvent, ClockNotifier, ClockState};
pub use clock_4led::{
    Clock4Led, Clock4LedNotifier, ClockCommand as Clock4LedCommand, ClockState as Clock4LedState,
};
#[cfg(feature = "wifi")]
pub use clock_offset_store::{
    clear as clear_timezone_offset, load as load_timezone_offset, save as save_timezone_offset,
};
#[cfg(feature = "wifi")]
pub use dns_server::dns_server_task;
pub use error::{Error, Result};
pub use ir_nec::{IrNec, IrNecEvent, IrNecNotifier};
pub use led_4seg::{Led4Seg, Led4SegNotifier, OutputArray, Text as Led4SegText};
pub use led_24x4::Led24x4;
pub use led_strip::{LedStrip, LedStripNotifier, PioBus, Rgb};
pub use rfid::{Rfid, RfidEvent, RfidNotifier};
pub use servo::Servo;
pub use smart_leds::colors;
#[cfg(feature = "wifi")]
pub use time_sync::{TimeSync, TimeSyncEvent, TimeSyncNotifier};
pub use unix_seconds::UnixSeconds;
#[cfg(feature = "wifi")]
pub use wifi::{Wifi, WifiEvent, WifiMode, WifiNotifier};
#[cfg(feature = "wifi")]
pub use wifi_config::{
    WifiCredentialSubmission, WifiCredentials, collect_wifi_credentials, http_config_server_task,
};

// Re-export macros (they're already at crate root due to #[macro_export])
// define_led_strips is available as lib::define_led_strips!
