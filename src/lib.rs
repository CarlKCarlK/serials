//! Shared items for the clock project.
#![no_std]
#![no_main]

mod bit_matrix_led4;
pub mod button;
pub mod char_lcd;
pub mod clock;
pub mod clock_led4;
mod blinker_led4;
mod display_led4;
pub mod clock_offset_store;
#[cfg(feature = "wifi")]
pub mod credential_store;
#[cfg(feature = "wifi")]
pub mod dhcp_server;
#[cfg(feature = "wifi")]
pub mod dns_server;
mod error;
pub mod ir_nec;
pub mod led24x4;
pub mod led4;
pub mod led_strip;
pub mod rfid;
pub mod servo;
pub mod time_sync;
mod unix_seconds;
#[cfg(feature = "wifi")]
pub mod wifi;
#[cfg(feature = "wifi")]
pub mod wifi_config;

// Re-export error types and result (used throughout)
pub use error::{Error, Result};
