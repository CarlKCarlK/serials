//! A device abstraction for type-safe persistent storage in flash memory.
//!
//! This module provides a generic flash block storage system that allows storing any
//! `serde`-compatible type in Raspberry Pi Pico's internal flash memory using postcard
//! serialization.
//!
//! # Features
//!
//! - **"Type safety"**: Uses whiteboard semantics—any data left over from other types or
//!   past runs is treated as empty from the current type's perspective. Reading with a
//!   different type than what was saved returns `None` (type hash mismatch).
//! - **Postcard serialization**: Compact, no_std-compatible binary format
//!
//! # Block Allocation
//!
//! Blocks are allocated from the end of flash memory backwards. Users choose unique `block_id`
//! values (0, 1, 2, ...) to identify each block.
//!
//! **Important**: Users are responsible for avoiding block_id collisions. Using the same
//! block_id for different types will cause type hash mismatches and return `None` on reads.
//!
//! See [`Flash`] for usage examples.

use crc32fast::Hasher;
use defmt::{error, info};
use embassy_rp::Peri;
use embassy_rp::flash::{Blocking, ERASE_SIZE, Flash as EmbassyFlash};
use embassy_rp::peripherals::FLASH;
use serde::{Deserialize, Serialize};
use static_cell::StaticCell;

use crate::{Error, Result};

// Internal flash size for Raspberry Pi Pico 2 (4 MB).
#[cfg(feature = "pico2")]
const INTERNAL_FLASH_SIZE: usize = 4 * 1024 * 1024;

// Internal flash size for Raspberry Pi Pico 1 W (2 MB).
#[cfg(all(not(feature = "pico2"), feature = "pico1"))]
const INTERNAL_FLASH_SIZE: usize = 2 * 1024 * 1024;

// Internal flash size fallback (2 MB).
#[cfg(all(not(feature = "pico2"), not(feature = "pico1")))]
pub const INTERNAL_FLASH_SIZE: usize = 2 * 1024 * 1024;

const MAGIC: u32 = 0x424C_4B53; // 'BLKS'
const HEADER_SIZE: usize = 4 + 4 + 2; // Magic + TypeHash + PayloadLen
const CRC_SIZE: usize = 4;
const MAX_PAYLOAD_SIZE: usize = ERASE_SIZE - HEADER_SIZE - CRC_SIZE; // 3900 bytes

/// Notifier type for the `Flash` device abstraction.
pub struct FlashNotifier {
    flash_cell: StaticCell<EmbassyFlash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE>>,
}

impl FlashNotifier {
    /// Create flash resources.
    #[must_use]
    pub const fn notifier() -> Self {
        Self {
            flash_cell: StaticCell::new(),
        }
    }
}

/// A device abstraction for type-safe persistent storage in flash memory.
///
/// This provides type-safe persistent storage using postcard serialization with whiteboard
/// semantics—reading with a different type than what was saved returns `None`.
///
/// # Examples
///
/// ## Storing custom device configuration
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use serde::{Serialize, Deserialize};
/// use serials::flash::{Flash, FlashNotifier};
///
/// // Define your configuration type
/// #[derive(Serialize, Deserialize, Debug, Default)]
/// struct DeviceConfig {
///     brightness: u8,
///     timezone_offset: i16,
///     display_mode: u8,
/// }
///
/// # async fn example() -> serials::Result<()> {
/// let p = embassy_rp::init(Default::default());
///
/// // Initialize Flash device using the notifier pattern
/// static FLASH_NOTIFIER: FlashNotifier = Flash::notifier();
/// let mut flash = Flash::new(&FLASH_NOTIFIER, p.FLASH);
///
/// // Load existing config from block 2, or use defaults
/// let mut config = flash.load::<DeviceConfig>(2)?.unwrap_or_default();
///
/// // Modify and save
/// config.brightness = 255;
/// flash.save(2, &config)?;
///
/// // Can also clear storage with: flash.clear(2)?;
/// # Ok(())
/// # }
/// ```
///
/// ## Whiteboard semantics demonstration
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// # use heapless::String;
/// # use serials::flash::{Flash, FlashNotifier};
/// # async fn example() -> serials::Result<()> {
/// # let p = embassy_rp::init(Default::default());
/// # static FLASH_NOTIFIER: FlashNotifier = Flash::notifier();
/// # let mut flash = Flash::new(&FLASH_NOTIFIER, p.FLASH);
/// // Save a string to block 3
/// flash.save(3, &String::<64>::try_from("Hello")?)?;
///
/// // Reading with a different type returns None (whiteboard semantics)
/// let result: Option<u64> = flash.load(3)?;
/// assert!(result.is_none());  // Different type (u64 vs String<64>)!
/// # Ok(())
/// # }
/// ```
pub struct Flash {
    flash: &'static mut EmbassyFlash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE>,
}

impl Flash {
    /// Create a new Flash device abstraction.
    ///
    /// This initializes the Flash peripheral and returns a device abstraction
    /// that can be used to create FlashBlock instances.
    ///
    /// # Arguments
    ///
    /// * `notifier` - Static notifier created with `Flash::notifier()`
    /// * `peripheral` - The FLASH peripheral from `embassy_rp::init()`
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # use panic_probe as _;
    /// # use embassy_executor::Spawner;
    /// # use serials::flash::Flash;
    /// # async fn example(p: embassy_rp::Peripherals) {
    /// static FLASH_NOTIFIER: serials::flash::FlashNotifier = Flash::notifier();
    /// let flash = Flash::new(&FLASH_NOTIFIER, p.FLASH);
    /// # }
    /// ```
    #[must_use]
    pub const fn notifier() -> FlashNotifier {
        FlashNotifier::notifier()
    }

    /// Create a new Flash device.
    #[must_use]
    pub fn new(notifier: &'static FlashNotifier, peripheral: Peri<'static, FLASH>) -> Self {
        let flash = notifier
            .flash_cell
            .init(EmbassyFlash::new_blocking(peripheral));
        Self { flash }
    }

    /// Save data to a flash block.
    ///
    /// # Arguments
    ///
    /// * `block_id` - Unique identifier for this block (0-based from end of flash)
    /// * `value` - The data to save
    ///
    /// See the [module-level documentation](crate::flash) for usage examples.
    pub fn save<T>(&mut self, block_id: u32, value: &T) -> Result<()>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        // Serialize to temporary buffer
        let mut payload_buffer = [0u8; MAX_PAYLOAD_SIZE];
        let payload_len = postcard::to_slice(value, &mut payload_buffer)
            .map_err(|_| {
                error!(
                    "Flash: Serialization failed or data too large (max {} bytes)",
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
        let offset = Self::block_offset(self.flash.capacity(), block_id);
        self.flash
            .blocking_erase(offset, offset + ERASE_SIZE as u32)
            .map_err(Error::Flash)?;
        self.flash
            .blocking_write(offset, &buffer)
            .map_err(Error::Flash)?;

        info!("Flash: Saved {} bytes to block {}", payload_len, block_id);
        Ok(())
    }

    /// Load data from a flash block.
    ///
    /// Returns `Ok(Some(value))` if valid data of the correct type is found,
    /// `Ok(None)` if no data is stored or type mismatch occurs, or `Err` if
    /// the stored data is corrupted.
    ///
    /// Type safety: If the stored data was saved with a different type, the type
    /// hash will mismatch and this returns `Ok(None)`.
    ///
    /// See the [module-level documentation](crate::flash) for usage examples.
    pub fn load<T>(&mut self, block_id: u32) -> Result<Option<T>>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let offset = Self::block_offset(self.flash.capacity(), block_id);
        let mut buffer = [0u8; ERASE_SIZE];

        self.flash
            .blocking_read(offset, &mut buffer)
            .map_err(Error::Flash)?;

        // Check magic number
        let magic = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if magic != MAGIC {
            info!("Flash: No data at block {}", block_id);
            return Ok(None);
        }

        // Check type hash
        let stored_type_hash = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
        let expected_type_hash = compute_type_hash::<T>();
        if stored_type_hash != expected_type_hash {
            info!(
                "Flash: Type mismatch at block {} (expected hash {}, found {})",
                block_id, expected_type_hash, stored_type_hash
            );
            return Ok(None);
        }

        // Read payload length
        let payload_len = u16::from_le_bytes(buffer[8..10].try_into().unwrap()) as usize;
        if payload_len > MAX_PAYLOAD_SIZE {
            error!(
                "Flash: Invalid payload length {} at block {}",
                payload_len, block_id
            );
            return Err(Error::StorageCorrupted);
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
                "Flash: CRC mismatch at block {} (expected {}, found {})",
                block_id, computed_crc, stored_crc
            );
            return Err(Error::StorageCorrupted);
        }

        // Deserialize payload
        let payload = &buffer[HEADER_SIZE..HEADER_SIZE + payload_len];
        let value: T = postcard::from_bytes(payload).map_err(|_| {
            error!("Flash: Deserialization failed at block {}", block_id);
            Error::StorageCorrupted
        })?;

        info!("Flash: Loaded data from block {}", block_id);
        Ok(Some(value))
    }

    /// Clear a flash block, erasing all stored data.
    ///
    /// See the [module-level documentation](crate::flash) for usage examples.
    pub fn clear(&mut self, block_id: u32) -> Result<()> {
        let offset = Self::block_offset(self.flash.capacity(), block_id);
        self.flash
            .blocking_erase(offset, offset + ERASE_SIZE as u32)
            .map_err(Error::Flash)?;
        info!("Flash: Cleared block {}", block_id);
        Ok(())
    }

    /// Calculate the flash offset for a block.
    ///
    /// Blocks are allocated from the end of flash backwards.
    fn block_offset(capacity: usize, block_id: u32) -> u32 {
        let capacity = capacity as u32;
        capacity - (block_id + 1) * ERASE_SIZE as u32
    }

    /// Get a mutable reference to the underlying Flash peripheral.
    ///
    /// This is used by FlashBlock instances to perform read/write/erase operations.
    #[must_use]
    pub fn peripheral(
        &mut self,
    ) -> &mut EmbassyFlash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE> {
        self.flash
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
