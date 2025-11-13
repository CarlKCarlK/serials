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
//! Conceptually, flash is treated as a slice of fixed-size erase blocks counted from the end of
//! memory backwards. Code can carve out disjoint partitions at compile time (like `split_at_mut`
//! on slices) and hand those partitions to subsystems that need persistent storage.
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
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
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
const TOTAL_BLOCKS: u32 = (INTERNAL_FLASH_SIZE / ERASE_SIZE) as u32;

/// Internal shared flash peripheral guarded by a mutex for safe concurrent partitions.
struct FlashInner {
    flash: Mutex<
        CriticalSectionRawMutex,
        core::cell::RefCell<
            EmbassyFlash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE>,
        >,
    >,
}

impl FlashInner {
    fn new(peripheral: Peri<'static, FLASH>) -> Self {
        Self {
            flash: Mutex::new(core::cell::RefCell::new(
                EmbassyFlash::new_blocking(peripheral),
            )),
        }
    }

    fn with_flash<R>(
        &self,
        f: impl FnOnce(&mut EmbassyFlash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE>) -> Result<R>,
    ) -> Result<R> {
        self.flash.lock(|flash| {
            let mut flash_ref = flash.borrow_mut();
            f(&mut *flash_ref)
        })
    }
}

/// Notifier type for the `Flash` device abstraction.
pub struct FlashNotifier {
    flash_cell: StaticCell<FlashInner>,
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
/// semantics—reading with a different type than what was saved returns `None`. Conceptually,
/// a `Flash` value is like a mutable slice of blocks; you can `split` it to hand disjoint regions
/// to subsystems that manage their portion of flash independently.
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
/// static FLASH_NOTIFIER: FlashNotifier = Flash::notifier();
/// let flash = Flash::new(&FLASH_NOTIFIER, p.FLASH);
///
/// // Reserve the first block for configuration data and ignore the remainder for now.
/// let (mut config_flash, _) = flash.split(1);
///
/// // Load existing config (block indices are relative to the partition, so 0 here)
/// let mut config = config_flash.load::<DeviceConfig>(0)?.unwrap_or_default();
///
/// // Modify and save
/// config.brightness = 255;
/// config_flash.save(0, &config)?;
///
/// // Can also clear storage with: config_flash.clear(0)?;
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
/// // Save a string to block 3 (relative to the root partition)
/// flash.save(3, &String::<64>::try_from("Hello")?)?;
///
/// // Reading with a different type returns None (whiteboard semantics)
/// let result: Option<u64> = flash.load(3)?;
/// assert!(result.is_none());  // Different type (u64 vs String<64>)!
/// # Ok(())
/// # }
/// ```
/// Partitioned flash view with exclusive access to a contiguous range of blocks.
pub struct Flash {
    inner: &'static FlashInner,
    start_block: u32,
    block_count: u32,
}

impl Flash {
    /// Create flash resources (root partition).
    #[must_use]
    pub const fn notifier() -> FlashNotifier {
        FlashNotifier::notifier()
    }

    /// Create a new Flash manager that spans the whole flash space.
    #[must_use]
    pub fn new(notifier: &'static FlashNotifier, peripheral: Peri<'static, FLASH>) -> Self {
        let inner = notifier.flash_cell.init(FlashInner::new(peripheral));
        Self {
            inner,
            start_block: 0,
            block_count: TOTAL_BLOCKS,
        }
    }

    /// Number of blocks in this partition.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.block_count
    }

    /// Returns `true` if the partition contains no blocks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.block_count == 0
    }

    /// Split this flash partition, returning disjoint left/right regions.
    pub fn split(mut self, left_blocks: u32) -> (Self, Self) {
        assert!(left_blocks <= self.block_count);
        let left = Flash {
            inner: self.inner,
            start_block: self.start_block,
            block_count: left_blocks,
        };
        self.start_block += left_blocks;
        self.block_count -= left_blocks;
        (left, self)
    }

    /// Convenience helper that carves out the first block in the partition.
    pub fn take_first(self) -> (Self, Self) {
        assert!(self.block_count >= 1);
        self.split(1)
    }

    /// Save data to a block relative to this partition (0-based).
    pub fn save<T>(&mut self, block_id: u32, value: &T) -> Result<()>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let absolute_block = self.absolute_block(block_id)?;

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
        let offset = block_offset(absolute_block);
        self.inner.with_flash(|flash| {
            flash
                .blocking_erase(offset, offset + ERASE_SIZE as u32)
                .map_err(Error::Flash)?;
            flash
                .blocking_write(offset, &buffer)
                .map_err(Error::Flash)?;
            Ok(())
        })?;

        info!(
            "Flash: Saved {} bytes to block {} (absolute {})",
            payload_len, block_id, absolute_block
        );
        Ok(())
    }

    /// Load data from a relative block within this partition.
    pub fn load<T>(&mut self, block_id: u32) -> Result<Option<T>>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let absolute_block = self.absolute_block(block_id)?;
        let offset = block_offset(absolute_block);
        let mut buffer = [0u8; ERASE_SIZE];

        self.inner.with_flash(|flash| {
            flash
                .blocking_read(offset, &mut buffer)
                .map_err(Error::Flash)?;
            Ok(())
        })?;

        // Check magic number
        let magic = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
        if magic != MAGIC {
            info!(
                "Flash: No data at block {} (absolute {})",
                block_id, absolute_block
            );
            return Ok(None);
        }

        // Check type hash
        let stored_type_hash = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
        let expected_type_hash = compute_type_hash::<T>();
        if stored_type_hash != expected_type_hash {
            info!(
                "Flash: Type mismatch at block {} (abs {}) (expected hash {}, found {})",
                block_id, absolute_block, expected_type_hash, stored_type_hash
            );
            return Ok(None);
        }

        // Read payload length
        let payload_len = u16::from_le_bytes(buffer[8..10].try_into().unwrap()) as usize;
        if payload_len > MAX_PAYLOAD_SIZE {
            error!(
                "Flash: Invalid payload length {} at block {} (abs {})",
                payload_len, block_id, absolute_block
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
                "Flash: CRC mismatch at block {} (abs {}) (expected {}, found {})",
                block_id, absolute_block, computed_crc, stored_crc
            );
            return Err(Error::StorageCorrupted);
        }

        // Deserialize payload
        let payload = &buffer[HEADER_SIZE..HEADER_SIZE + payload_len];
        let value: T = postcard::from_bytes(payload).map_err(|_| {
            error!(
                "Flash: Deserialization failed at block {} (abs {})",
                block_id, absolute_block
            );
            Error::StorageCorrupted
        })?;

        info!(
            "Flash: Loaded data from block {} (absolute {})",
            block_id, absolute_block
        );
        Ok(Some(value))
    }

    /// Clear a block relative to this partition.
    pub fn clear(&mut self, block_id: u32) -> Result<()> {
        let absolute_block = self.absolute_block(block_id)?;
        let offset = block_offset(absolute_block);
        self.inner.with_flash(|flash| {
            flash
                .blocking_erase(offset, offset + ERASE_SIZE as u32)
                .map_err(Error::Flash)?;
            Ok(())
        })?;
        info!(
            "Flash: Cleared block {} (absolute {})",
            block_id, absolute_block
        );
        Ok(())
    }

    fn absolute_block(&self, block_id: u32) -> Result<u32> {
        if block_id >= self.block_count {
            return Err(Error::IndexOutOfBounds);
        }
        Ok(self.start_block + block_id)
    }
}

/// Blocks are allocated from the end of flash backwards.
fn block_offset(block_id: u32) -> u32 {
    let capacity = INTERNAL_FLASH_SIZE as u32;
    capacity - (block_id + 1) * ERASE_SIZE as u32
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
