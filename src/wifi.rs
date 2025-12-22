//! A device abstraction for WiFi connectivity; see [`Wifi`] for details.
//!
//! This module re-exports the WiFi handle and resources from [`crate::wifi_auto`].

#[cfg(all(feature = "wifi", not(feature = "host")))]
pub use crate::wifi_auto::{Wifi, WifiEvent, WifiStatic};
