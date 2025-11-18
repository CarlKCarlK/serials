//! Device abstractions for peripherals for Pico 1 and 2 (with and without WiFi).
#![cfg_attr(not(feature = "host"), no_std)]
#![cfg_attr(not(feature = "host"), no_main)]

// Compile-time checks: exactly one board must be selected (unless testing with host feature)
#[cfg(all(not(any(feature = "pico1", feature = "pico2")), not(feature = "host")))]
compile_error!("Must enable exactly one board feature: 'pico1' or 'pico2'");

#[cfg(all(feature = "pico1", feature = "pico2"))]
compile_error!("Cannot enable both 'pico1' and 'pico2' features simultaneously");

// Compile-time checks: exactly one architecture must be selected (unless testing with host feature)
#[cfg(all(not(any(feature = "arm", feature = "riscv")), not(feature = "host")))]
compile_error!("Must enable exactly one architecture feature: 'arm' or 'riscv'");

#[cfg(all(feature = "arm", feature = "riscv"))]
compile_error!("Cannot enable both 'arm' and 'riscv' features simultaneously");

// Compile-time check: pico1 only supports ARM
#[cfg(all(feature = "pico1", feature = "riscv"))]
compile_error!("Pico 1 (RP2040) only supports ARM architecture, not RISC-V");

// Only include modules that work without embassy when host feature is enabled
#[cfg(feature = "host")]
pub mod bit_matrix_led4;

// These modules require embassy_rp and are excluded when testing on host
#[cfg(not(feature = "host"))]
pub mod bit_matrix_led4;
#[cfg(not(feature = "host"))]
pub mod button;
#[cfg(not(feature = "host"))]
pub mod char_lcd;
#[cfg(not(feature = "host"))]
pub mod clock;
#[cfg(all(feature = "wifi", not(feature = "host")))]
pub mod dhcp_server;
#[cfg(all(feature = "wifi", not(feature = "host")))]
pub mod dns_server;
#[cfg(not(feature = "host"))]
mod error;
#[cfg(not(feature = "host"))]
pub mod flash_array;
#[cfg(not(feature = "host"))]
pub mod ir;
#[cfg(not(feature = "host"))]
pub mod ir_kepler;
#[cfg(not(feature = "host"))]
pub mod ir_mapping;
#[cfg(not(feature = "host"))]
pub mod led24x4;
#[cfg(not(feature = "host"))]
pub mod led4;
#[cfg(not(feature = "host"))]
pub mod led4_simple;
#[cfg(not(feature = "host"))]
pub mod led_strip;
#[cfg(not(feature = "host"))]
pub mod rfid;
#[cfg(not(feature = "host"))]
pub mod servo;
#[cfg(not(feature = "host"))]
pub mod time_sync;
#[cfg(all(feature = "wifi", not(feature = "host")))]
pub mod time_sync_old;
#[cfg(not(feature = "host"))]
pub mod unix_seconds;
#[cfg(all(feature = "wifi", not(feature = "host")))]
pub mod wifi;
#[cfg(all(feature = "wifi", not(feature = "host")))]
pub mod wifi_auto;
#[cfg(all(feature = "wifi", not(feature = "host")))]
pub mod wifi_config;

// Re-export error types and result (used throughout)
#[cfg(not(feature = "host"))]
pub use error::{Error, Result};
