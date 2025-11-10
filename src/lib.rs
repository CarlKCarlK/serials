//! Device abstractions for peripherals for Pico 1 and 2 (with and without WiFi).
#![no_std]
#![no_main]

#[doc(hidden)]
pub mod bit_matrix_led4;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod button;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod char_lcd;
pub mod clock;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod clock_led4;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod clock_offset_store;
#[cfg(feature = "wifi")]
pub mod credential_store;
#[cfg(feature = "wifi")]
pub mod dhcp_server;
#[cfg(feature = "wifi")]
pub mod dns_server;
mod error;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod ir_nec;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod led24x4;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod led4;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod led4_simple;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod led_strip;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod rfid;
#[cfg(any(feature = "pico1", feature = "pico2"))]
pub mod servo;
pub mod time_sync;
mod unix_seconds;
#[cfg(feature = "wifi")]
pub mod wifi;
#[cfg(feature = "wifi")]
pub mod wifi_config;

// Re-export error types and result (used throughout)
pub use error::{Error, Result};
