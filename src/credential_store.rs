//! Persistent storage for WiFi credentials in flash memory.
//!
//! This module provides functions to save, load, and clear WiFi credentials in the
//! Raspberry Pi Pico's internal flash memory. Credentials are stored using the
//! [`crate::flash`] module with type-safe postcard serialization.
//!
//! You can choose any block ID for storage - just ensure it doesn't conflict with
//! other data you're storing in flash.
//!
//! # Examples
//!
//! ## Saving and loading credentials
//!
//! ```no_run
//! use embassy_rp::flash::{Blocking, Flash};
//! use serials::flash::INTERNAL_FLASH_SIZE;
//! use serials::wifi_config::WifiCredentials;
//! use serials::credential_store;
//!
//! # async fn example() -> serials::Result<()> {
//! let p = embassy_rp::init(Default::default());
//! let mut flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//!
//! // Create credentials
//! let mut ssid = heapless::String::<32>::new();
//! let mut password = heapless::String::<64>::new();
//! ssid.push_str("MyNetwork").unwrap();
//! password.push_str("MyPassword123").unwrap();
//! let credentials = WifiCredentials { ssid, password };
//!
//! // Save to flash (using block_id = 0 for credentials)
//! credential_store::save(&mut flash, &credentials, 0)?
//!
//! // Load from flash
//! if let Some(loaded) = credential_store::load(&mut flash)? {
//!     // Use loaded credentials
//!     assert_eq!(loaded.ssid, "MyNetwork");
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Checking for stored credentials at boot
//!
//! ```no_run
//! use embassy_rp::flash::{Blocking, Flash};
//! use serials::flash::INTERNAL_FLASH_SIZE;
//! use serials::credential_store;
//! use defmt::info;
//!
//! # async fn example() -> serials::Result<()> {
//! let p = embassy_rp::init(Default::default());
//! let mut flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//!
//! match credential_store::load(&mut flash)? {
//!     Some(credentials) => {
//!         // Credentials found - connect to WiFi
//!         info!("Found credentials for SSID: {}", credentials.ssid);
//!     }
//!     None => {
//!         // No credentials - start in AP mode for configuration
//!         info!("No credentials found - starting AP mode");
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## Clearing credentials
//!
//! ```no_run
//! use embassy_rp::flash::{Blocking, Flash};
//! use serials::flash::INTERNAL_FLASH_SIZE;
//! use serials::credential_store;
//!
//! # async fn example() -> serials::Result<()> {
//! let p = embassy_rp::init(Default::default());
//! let mut flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//!
//! // Clear stored credentials (e.g., for factory reset)
//! credential_store::clear(&mut flash, 0)?
//! # Ok(())
//! # }
//! ```
#![cfg(feature = "wifi")]

use embassy_rp::flash::{Blocking, Flash};

use crate::flash::{FlashBlock, INTERNAL_FLASH_SIZE};
use crate::wifi_config::WifiCredentials;
use crate::Result;

/// Load WiFi credentials from flash storage.
///
/// Returns `Ok(Some(credentials))` if valid credentials are found, `Ok(None)` if no
/// credentials are stored, or `Err` if the stored data is corrupted.
///
/// # Arguments
///
/// * `flash` - Flash peripheral
/// * `block_id` - Block ID where credentials are stored (e.g., 0 for last sector)
///
/// See the [module-level documentation](crate::credential_store) for usage examples.
pub fn load(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    block_id: u32,
) -> Result<Option<WifiCredentials>> {
    let mut block: FlashBlock<embassy_rp::peripherals::FLASH, WifiCredentials> = FlashBlock::new(block_id);
    block.load(flash)
}

/// Save WiFi credentials to flash storage.
///
/// Saves the credentials to flash memory with CRC32 validation.
/// This operation erases the sector before writing, so it's relatively slow.
///
/// # Arguments
///
/// * `flash` - Flash peripheral
/// * `credentials` - WiFi credentials to store
/// * `block_id` - Block ID where credentials should be stored (e.g., 0 for last sector)
///
/// # Errors
///
/// Returns `Err(Error::Flash)` if flash operations fail.
///
/// See the [module-level documentation](crate::credential_store) for usage examples.
pub fn save(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    credentials: &WifiCredentials,
    block_id: u32,
) -> Result<()> {
    let mut block: FlashBlock<embassy_rp::peripherals::FLASH, WifiCredentials> = FlashBlock::new(block_id);
    block.save(flash, credentials)
}

/// Remove stored WiFi credentials from flash.
///
/// Erases the flash sector containing credentials. After calling this function,
/// [`load`] will return `Ok(None)`.
///
/// This is useful for:
/// - Factory reset functionality
/// - Clearing invalid credentials after connection failures
/// - User-initiated credential removal
///
/// # Arguments
///
/// * `flash` - Flash peripheral
/// * `block_id` - Block ID where credentials are stored (must match the block used in save/load)
///
/// # Errors
///
/// Returns `Err(Error::Flash)` if the erase operation fails.
///
/// See the [module-level documentation](crate::credential_store) for usage examples.
pub fn clear(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    block_id: u32,
) -> Result<()> {
    let mut block: FlashBlock<embassy_rp::peripherals::FLASH, WifiCredentials> = FlashBlock::new(block_id);
    block.clear(flash)
}

