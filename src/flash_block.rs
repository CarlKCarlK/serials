//! Type-safe persistent storage in flash memory using postcard serialization.
//!
//! This module provides a generic flash block storage system that allows storing any
//! `serde`-compatible type in Raspberry Pi Pico's internal flash memory. Each block
//! is stored in a separate 4KB flash sector with automatic type checking via compile-time
//! type hashing.
//!
//! # Features
//!
//! - **Type safety**: Reading wrong type from a block returns `None` (type hash mismatch)
//! - **CRC validation**: Detects data corruption with CRC32 checksums
//! - **Postcard serialization**: Compact, no_std-compatible binary format
//! - **Device Abstraction pattern**: Follows the same pattern as other peripherals in this crate
//!
//! # Block Allocation
//!
//! Blocks are allocated from the end of flash memory backwards. Users choose unique `block_id`
//! values (0, 1, 2, ...) to identify each block.
//!
//! **Important**: Users are responsible for avoiding block_id collisions. Using the same
//! block_id for different types will cause type hash mismatches and return `None` on reads.
//!
//! The [`crate::credential_store`] and [`crate::clock_offset_store`] modules provide
//! convenient wrappers around `FlashBlock` for common use cases.
//!
//! # Storage Format
//!
//! Each 4KB block contains:
//! - Magic number (4 bytes): `0x424C4B53` ('BLKS')
//! - Type hash (4 bytes): FNV-1a hash of the type name
//! - Payload length (2 bytes): Length of serialized data
//! - Payload (up to 3900 bytes): Postcard-serialized data
//! - CRC32 (4 bytes): Checksum of entire block
//!
//! # Examples
//!
//! ## Storing custom device configuration
//!
//! ```no_run
//! use serde::{Serialize, Deserialize};
//! use embassy_rp::flash::{Blocking, Flash};
//! use serials::flash_block::{FlashBlock, INTERNAL_FLASH_SIZE};
//!
//! // Define your configuration type
//! #[derive(Serialize, Deserialize, Debug)]
//! struct DeviceConfig {
//!     brightness: u8,
//!     timezone_offset: i16,
//!     display_mode: u8,
//! }
//!
//! # async fn example() -> serials::Result<()> {
//! let p = embassy_rp::init(Default::default());
//!
//! // Create flash block notifier (static storage)
//! let flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//! let mut config_block = FlashBlock::new(flash, 2);
//!
//! // Try to load existing config
//! match config_block.load()? {
//!     Some(config) => {
//!         defmt::info!("Loaded config: brightness={}", config.brightness);
//!     }
//!     None => {
//!         // No config found, use defaults
//!         let default_config = DeviceConfig {
//!             brightness: 128,
//!             timezone_offset: 0,
//!             display_mode: 1,
//!         };
//!         config_block.save(&default_config)?;
//!         defmt::info!("Saved default config");
//!     }
//! }
//!
//! // Update and save
//! let mut config = config_block.load()?.unwrap();
//! config.brightness = 255;
//! config_block.save(&config)?;
//!
//! // Clear storage
//! config_block.clear()?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Type safety demonstration
//!
//! ```no_run
//! # use serde::{Serialize, Deserialize};
//! # use embassy_rp::flash::{Blocking, Flash};
//! # use serials::flash_block::{FlashBlock, INTERNAL_FLASH_SIZE};
//! #[derive(Serialize, Deserialize)]
//! struct TypeA { value: u32 }
//!
//! #[derive(Serialize, Deserialize)]
//! struct TypeB { value: u32 }
//!
//! # async fn example() -> serials::Result<()> {
//! # let p = embassy_rp::init(Default::default());
//! # let flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//! let mut block_a = FlashBlock::new(flash, 2);
//! block_a.save(&TypeA { value: 42 })?;
//!
//! // Reading with wrong type returns None due to type hash mismatch
//! let flash2 = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//! let mut block_b = FlashBlock::new(flash2, 2);
//! assert!(block_b.load()?.is_none());  // Type mismatch!
//! # Ok(())
//! # }
//! ```

use core::marker::PhantomData;

use crc32fast::Hasher;
use defmt::{error, info};
use embassy_rp::flash::{Blocking, Flash, Instance, ERASE_SIZE};
use serde::{Deserialize, Serialize};

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

const MAGIC: u32 = 0x424C_4B53; // 'BLKS'
const HEADER_SIZE: usize = 4 + 4 + 2; // Magic + TypeHash + PayloadLen
const CRC_SIZE: usize = 4;
const MAX_PAYLOAD_SIZE: usize = ERASE_SIZE - HEADER_SIZE - CRC_SIZE; // 3900 bytes

/// A device abstraction for type-safe persistent storage in flash memory.
///
/// See the [module-level documentation](crate::flash_block) for usage examples.
pub struct FlashBlock<I: Instance + 'static, T, const N: usize = INTERNAL_FLASH_SIZE> {
    block_id: u32,
    _phantom: PhantomData<(fn() -> T, *const I)>,
}

impl<I: Instance + 'static, T, const N: usize> FlashBlock<I, T, N>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    /// Create a new FlashBlock device.
    ///
    /// # Arguments
    ///
    /// * `block_id` - Unique identifier for this block (0-based from end of flash)
    ///
    /// # Block Allocation
    ///
    /// - Block 0: Reserved for WiFi credentials
    /// - Block 1: Reserved for timezone offset
    /// - Block 2+: Available for user applications
    ///
    /// See the [module-level documentation](crate::flash_block) for usage examples.
    #[must_use]
    pub fn new(
        block_id: u32,
    ) -> Self {
        Self {
            block_id,
            _phantom: PhantomData,
        }
    }

    /// Load data from flash.
    ///
    /// Returns `Ok(Some(value))` if valid data of the correct type is found,
    /// `Ok(None)` if no data is stored or type mismatch occurs, or `Err` if
    /// the stored data is corrupted.
    ///
    /// Type safety: If the stored data was saved with a different type, the type
    /// hash will mismatch and this returns `Ok(None)`.
    ///
    /// See the [module-level documentation](crate::flash_block) for usage examples.
    pub fn load(&mut self, flash: &mut Flash<'_, I, Blocking, N>) -> Result<Option<T>> {
        let offset = self.block_offset(flash);
        let mut buffer = [0u8; ERASE_SIZE];

        flash
            .blocking_read(offset, &mut buffer)
            .map_err(Error::Flash)?;

        // Check magic number
        let magic = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if magic != MAGIC {
            info!("FlashBlock: No data at block {}", self.block_id);
            return Ok(None);
        }

        // Check type hash
        let stored_type_hash = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
        let expected_type_hash = compute_type_hash::<T>();
        if stored_type_hash != expected_type_hash {
            info!(
                "FlashBlock: Type mismatch at block {} (expected hash {}, found {})",
                self.block_id, expected_type_hash, stored_type_hash
            );
            return Ok(None);
        }

        // Read payload length
        let payload_len = u16::from_le_bytes(buffer[8..10].try_into().unwrap()) as usize;
        if payload_len > MAX_PAYLOAD_SIZE {
            error!(
                "FlashBlock: Invalid payload length {} at block {}",
                payload_len, self.block_id
            );
            return Err(Error::CredentialStorageCorrupted);
        }

        // Verify CRC
        let crc_offset = HEADER_SIZE + payload_len;
        let stored_crc = u32::from_le_bytes(
            buffer[crc_offset..crc_offset + CRC_SIZE]
                .try_into()
                .unwrap(),
        );
        let computed_crc = compute_crc(&buffer[0..crc_offset]);
        if stored_crc != computed_crc {
            error!(
                "FlashBlock: CRC mismatch at block {} (expected {}, found {})",
                self.block_id, computed_crc, stored_crc
            );
            return Err(Error::CredentialStorageCorrupted);
        }

        // Deserialize payload
        let payload = &buffer[HEADER_SIZE..HEADER_SIZE + payload_len];
        let value: T = postcard::from_bytes(payload).map_err(|_| {
            error!(
                "FlashBlock: Deserialization failed at block {}",
                self.block_id
            );
            Error::CredentialStorageCorrupted
        })?;

        info!("FlashBlock: Loaded data from block {}", self.block_id);
        Ok(Some(value))
    }

    /// Save data to flash.
    ///
    /// This operation erases the flash sector before writing, so it's relatively slow
    /// (typically 100-200ms).
    ///
    /// # Errors
    ///
    /// Returns `Err(Error::FormatError)` if the serialized data exceeds 3900 bytes.
    /// Returns `Err(Error::Flash)` if flash operations fail.
    ///
    /// See the [module-level documentation](crate::flash_block) for usage examples.
    pub fn save(&mut self, flash: &mut Flash<'_, I, Blocking, N>, value: &T) -> Result<()> {
        // Serialize to temporary buffer
        let mut payload_buffer = [0u8; MAX_PAYLOAD_SIZE];
        let payload_len = postcard::to_slice(value, &mut payload_buffer)
            .map_err(|_| {
                error!(
                    "FlashBlock: Serialization failed or data too large (max {} bytes)",
                    MAX_PAYLOAD_SIZE
                );
                Error::FormatError
            })?
            .len();

        // Build block buffer
        let mut buffer = [0xFFu8; ERASE_SIZE];

        // Write header
        buffer[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        buffer[4..8].copy_from_slice(&compute_type_hash::<T>().to_le_bytes());
        buffer[8..10].copy_from_slice(&(payload_len as u16).to_le_bytes());

        // Write payload
        buffer[HEADER_SIZE..HEADER_SIZE + payload_len]
            .copy_from_slice(&payload_buffer[..payload_len]);

        // Compute and write CRC
        let crc_offset = HEADER_SIZE + payload_len;
        let crc = compute_crc(&buffer[0..crc_offset]);
        buffer[crc_offset..crc_offset + CRC_SIZE].copy_from_slice(&crc.to_le_bytes());

        // Write to flash
        let offset = self.block_offset(flash);
        flash
            .blocking_erase(offset, offset + ERASE_SIZE as u32)
            .map_err(Error::Flash)?;
        flash
            .blocking_write(offset, &buffer)
            .map_err(Error::Flash)?;

        info!(
            "FlashBlock: Saved {} bytes to block {}",
            payload_len, self.block_id
        );
        Ok(())
    }

    /// Clear data from flash.
    ///
    /// Erases the flash sector. After calling this, [`load`](Self::load) will return `Ok(None)`.
    ///
    /// See the [module-level documentation](crate::flash_block) for usage examples.
    pub fn clear(&mut self, flash: &mut Flash<'_, I, Blocking, N>) -> Result<()> {
        let offset = self.block_offset(flash);
        flash
            .blocking_erase(offset, offset + ERASE_SIZE as u32)
            .map_err(Error::Flash)?;
        info!("FlashBlock: Cleared block {}", self.block_id);
        Ok(())
    }

    /// Calculate the flash offset for this block.
    ///
    /// Blocks are allocated from the end of flash backwards.
    fn block_offset(&self, flash: &Flash<'_, I, Blocking, N>) -> u32 {
        let capacity = flash.capacity() as u32;
        capacity - (self.block_id + 1) * ERASE_SIZE as u32
    }
}

/// Compute FNV-1a hash of the type name for type safety.
fn compute_type_hash<T>() -> u32 {
    const FNV_PRIME: u32 = 16_777_619;
    const FNV_OFFSET: u32 = 2_166_136_261;

    let type_name = core::any::type_name::<T>();
    let mut hash = FNV_OFFSET;

    for byte in type_name.bytes() {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    hash
}

/// Compute CRC32 checksum.
fn compute_crc(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}
