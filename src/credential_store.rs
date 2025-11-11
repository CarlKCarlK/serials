//! Persistent storage for WiFi credentials in flash memory.
//!
//! This module provides functions to save, load, and clear WiFi credentials in the
//! Raspberry Pi Pico's internal flash memory. Credentials are stored in the last
//! flash sector with CRC32 validation to detect corruption.
//!
//! The storage format includes:
//! - Magic number for validation
//! - Version number for future compatibility
//! - CRC32 checksum
//! - SSID (up to 32 bytes)
//! - Password (up to 64 bytes)
//!
//! # Examples
//!
//! ## Saving and loading credentials
//!
//! ```no_run
//! use embassy_rp::flash::{Blocking, Flash};
//! use serials::credential_store::{self, INTERNAL_FLASH_SIZE};
//! use serials::wifi_config::WifiCredentials;
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
//! // Save to flash
//! credential_store::save(&mut flash, &credentials)?;
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
//! use serials::credential_store::{self, INTERNAL_FLASH_SIZE};
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
//! use serials::credential_store::{self, INTERNAL_FLASH_SIZE};
//!
//! # async fn example() -> serials::Result<()> {
//! let p = embassy_rp::init(Default::default());
//! let mut flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//!
//! // Clear stored credentials (e.g., for factory reset)
//! credential_store::clear(&mut flash)?;
//! # Ok(())
//! # }
//! ```
#![cfg(feature = "wifi")]

use crc32fast::Hasher;
use embassy_rp::flash::Instance;
use embassy_rp::flash::{Blocking, ERASE_SIZE, Flash};

use crate::wifi_config::WifiCredentials;
use crate::{Error, Result};

/// Internal flash size for Raspberry Pi Pico 2 (4 MB).
#[cfg(feature = "pico2")]
pub const INTERNAL_FLASH_SIZE: usize = 4 * 1024 * 1024;

/// Internal flash size for Raspberry Pi Pico 1 W (2 MB).
#[cfg(all(not(feature = "pico2"), feature = "pico1"))]
pub const INTERNAL_FLASH_SIZE: usize = 2 * 1024 * 1024;

/// Internal flash size fallback (2 MB).
#[cfg(all(not(feature = "pico2"), not(feature = "pico1")))]
pub const INTERNAL_FLASH_SIZE: usize = 2 * 1024 * 1024;

const STORAGE_SIZE: usize = ERASE_SIZE;
const MAGIC: u32 = 0x5749_4649; // 'WIFI'
const VERSION: u16 = 1;
const CRC_OFFSET: usize = 4;
const VERSION_OFFSET: usize = 8;
const LENGTHS_OFFSET: usize = 10;
const RESERVED_OFFSET: usize = 12;
const SSID_OFFSET: usize = 16;
const SSID_CAPACITY: usize = 32;
const PASSWORD_OFFSET: usize = SSID_OFFSET + SSID_CAPACITY;
const PASSWORD_CAPACITY: usize = 64;
const DATA_END: usize = PASSWORD_OFFSET + PASSWORD_CAPACITY;

/// Load WiFi credentials from reserved flash storage.
///
/// Returns `Ok(Some(credentials))` if valid credentials are found, `Ok(None)` if no
/// credentials are stored, or `Err` if the stored data is corrupted.
///
/// The function validates:
/// - Magic number to confirm credential presence
/// - Version number for compatibility
/// - CRC32 checksum to detect corruption
/// - UTF-8 encoding of SSID and password
///
/// See the [module-level documentation](crate::credential_store) for usage examples.
pub fn load<'d, T: Instance>(
    flash: &mut Flash<'d, T, Blocking, INTERNAL_FLASH_SIZE>,
) -> Result<Option<WifiCredentials>> {
    let offset = storage_offset(flash);
    let mut buffer = [0u8; STORAGE_SIZE];
    flash
        .blocking_read(offset, &mut buffer)
        .map_err(Error::Flash)?;

    if u32::from_le_bytes(buffer[..CRC_OFFSET].try_into().unwrap()) != MAGIC {
        return Ok(None);
    }

    let stored_crc = u32::from_le_bytes(buffer[CRC_OFFSET..VERSION_OFFSET].try_into().unwrap());
    let version = u16::from_le_bytes(buffer[VERSION_OFFSET..LENGTHS_OFFSET].try_into().unwrap());

    if version != VERSION {
        return Ok(None);
    }

    let ssid_len = buffer[LENGTHS_OFFSET] as usize;
    let password_len = buffer[LENGTHS_OFFSET + 1] as usize;

    if ssid_len == 0 || ssid_len > SSID_CAPACITY || password_len > PASSWORD_CAPACITY {
        return Err(Error::CredentialStorageCorrupted);
    }

    let crc = compute_crc(&buffer[VERSION_OFFSET..DATA_END]);
    if crc != stored_crc {
        return Err(Error::CredentialStorageCorrupted);
    }

    let ssid_bytes = &buffer[SSID_OFFSET..SSID_OFFSET + ssid_len];
    let password_bytes = &buffer[PASSWORD_OFFSET..PASSWORD_OFFSET + password_len];

    let ssid_str =
        core::str::from_utf8(ssid_bytes).map_err(|_| Error::CredentialStorageCorrupted)?;
    let password_str =
        core::str::from_utf8(password_bytes).map_err(|_| Error::CredentialStorageCorrupted)?;

    let mut ssid = heapless::String::<SSID_CAPACITY>::new();
    let mut password = heapless::String::<PASSWORD_CAPACITY>::new();
    ssid.push_str(ssid_str)
        .map_err(|_| Error::CredentialStorageCorrupted)?;
    password
        .push_str(password_str)
        .map_err(|_| Error::CredentialStorageCorrupted)?;

    Ok(Some(WifiCredentials { ssid, password }))
}

/// Persist WiFi credentials into reserved flash storage.
///
/// Saves the credentials to the last sector of flash memory with CRC32 validation.
/// This operation erases the sector before writing, so it's relatively slow.
///
/// # Arguments
///
/// * `flash` - Mutable reference to the flash peripheral
/// * `credentials` - WiFi credentials to store
///
/// # Errors
///
/// Returns `Err(Error::FormatError)` if:
/// - SSID is empty or longer than 32 bytes
/// - Password is longer than 64 bytes
///
/// Returns `Err(Error::Flash)` if flash operations fail.
///
/// See the [module-level documentation](crate::credential_store) for usage examples.
pub fn save<'d, T: Instance>(
    flash: &mut Flash<'d, T, Blocking, INTERNAL_FLASH_SIZE>,
    credentials: &WifiCredentials,
) -> Result<()> {
    let offset = storage_offset(flash);

    let ssid_len = credentials.ssid.len();
    let password_len = credentials.password.len();

    if ssid_len == 0 || ssid_len > SSID_CAPACITY || password_len > PASSWORD_CAPACITY {
        return Err(Error::FormatError);
    }

    let mut buffer = [0xFFu8; STORAGE_SIZE];
    buffer[..CRC_OFFSET].copy_from_slice(&MAGIC.to_le_bytes());
    buffer[VERSION_OFFSET..LENGTHS_OFFSET].copy_from_slice(&VERSION.to_le_bytes());
    buffer[LENGTHS_OFFSET] = ssid_len as u8;
    buffer[LENGTHS_OFFSET + 1] = password_len as u8;
    buffer[RESERVED_OFFSET..SSID_OFFSET].fill(0);

    buffer[SSID_OFFSET..SSID_OFFSET + ssid_len].copy_from_slice(credentials.ssid.as_bytes());
    buffer[PASSWORD_OFFSET..PASSWORD_OFFSET + password_len]
        .copy_from_slice(credentials.password.as_bytes());

    let crc = compute_crc(&buffer[VERSION_OFFSET..DATA_END]);
    buffer[CRC_OFFSET..VERSION_OFFSET].copy_from_slice(&crc.to_le_bytes());

    flash
        .blocking_erase(offset, offset + STORAGE_SIZE as u32)
        .map_err(Error::Flash)?;
    flash
        .blocking_write(offset, &buffer)
        .map_err(Error::Flash)?;
    Ok(())
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
/// # Errors
///
/// Returns `Err(Error::Flash)` if the erase operation fails.
///
/// See the [module-level documentation](crate::credential_store) for usage examples.
pub fn clear<'d, T: Instance>(
    flash: &mut Flash<'d, T, Blocking, INTERNAL_FLASH_SIZE>,
) -> Result<()> {
    let offset = storage_offset(flash);
    flash
        .blocking_erase(offset, offset + STORAGE_SIZE as u32)
        .map_err(Error::Flash)
}

fn storage_offset<'d, T: Instance>(flash: &Flash<'d, T, Blocking, INTERNAL_FLASH_SIZE>) -> u32 {
    let capacity = flash.capacity() as u32;
    capacity - STORAGE_SIZE as u32
}

fn compute_crc(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}
