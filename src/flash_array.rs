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
//! Conceptually, flash is treated as an array of fixed-size erase blocks counted from the end of
//! memory backwards. Code can carve out disjoint partitions at compile time (similar to using
//! `split_at_mut` on slices) and hand those partitions to subsystems that need persistent storage.
//!
//! ⚠️ **Warning**: The RP2040 stores firmware, vector tables, and user data in the same flash
//! device. Only request block handles from regions you have explicitly reserved for storage.
//! Writing to an arbitrary block can erase the running program and leave the device unbootable.
//!
//! **Important**: Users are responsible for avoiding block_id collisions. Using the same
//! block_id for different types will cause type hash mismatches and return `None` on reads.
//!
//! See [`FlashArray`] for usage examples.

use core::array;
use crc32fast::Hasher;
use defmt::{error, info};
use embassy_rp::Peri;
use embassy_rp::flash::{Blocking, ERASE_SIZE, Flash as EmbassyFlash};
use embassy_rp::peripherals::FLASH;
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use portable_atomic::{AtomicU32, Ordering};
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

/// Shared flash manager that owns the hardware driver and allocation cursor.
struct FlashManager {
    flash: Mutex<
        CriticalSectionRawMutex,
        core::cell::RefCell<EmbassyFlash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE>>,
    >,
    next_block: AtomicU32,
}

impl FlashManager {
    fn new(peripheral: Peri<'static, FLASH>) -> Self {
        Self {
            flash: Mutex::new(core::cell::RefCell::new(EmbassyFlash::new_blocking(
                peripheral,
            ))),
            next_block: AtomicU32::new(0),
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

    fn reserve<const N: usize>(&'static self) -> Result<[FlashBlock; N]> {
        let start = self.next_block.fetch_add(N as u32, Ordering::SeqCst);
        let end = start.checked_add(N as u32).ok_or(Error::IndexOutOfBounds)?;
        if end > TOTAL_BLOCKS {
            // rollback
            self.next_block.fetch_sub(N as u32, Ordering::SeqCst);
            return Err(Error::IndexOutOfBounds);
        }
        Ok(array::from_fn(|idx| FlashBlock {
            manager: self,
            block: start + idx as u32,
        }))
    }
}

/// Handle to a single flash erase block.
pub struct FlashBlock {
    manager: &'static FlashManager,
    block: u32,
}

impl FlashBlock {
    /// Load data stored in this block.
    pub fn load<T>(&mut self) -> Result<Option<T>>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        load_block(self.manager, self.block)
    }

    /// Save data to this block.
    pub fn save<T>(&mut self, value: &T) -> Result<()>
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        save_block(self.manager, self.block, value)
    }

    /// Clear this block.
    pub fn clear(&mut self) -> Result<()> {
        clear_block(self.manager, self.block)
    }

    /// Return the absolute block index within flash.
    #[must_use]
    pub const fn block_id(&self) -> u32 {
        self.block
    }
}

/// Static type for constructing flash arrays.
pub struct FlashArrayStatic {
    manager_cell: StaticCell<FlashManager>,
    manager_ref: Mutex<CriticalSectionRawMutex, core::cell::RefCell<Option<&'static FlashManager>>>,
}

impl FlashArrayStatic {
    /// Create flash resources.
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            manager_cell: StaticCell::new(),
            manager_ref: Mutex::new(core::cell::RefCell::new(None)),
        }
    }

    fn manager(&'static self, peripheral: Peri<'static, FLASH>) -> &'static FlashManager {
        self.manager_ref.lock(|slot_cell| {
            let mut slot = slot_cell.borrow_mut();
            if slot.is_none() {
                let manager_mut = self.manager_cell.init(FlashManager::new(peripheral));
                let manager_ref: &'static FlashManager = manager_mut;
                *slot = Some(manager_ref);
            }
            slot.expect("manager initialized")
        })
    }
}

/// A device abstraction for type-safe persistent storage in flash memory.
///
/// This provides type-safe persistent storage using postcard serialization with whiteboard
/// semantics—reading with a different type than what was saved returns `None`. Rather than
/// manually juggling partitions, you reserve a contiguous prefix of flash blocks (0..N-1) and
/// destructure the returned array however you like.
///
/// # Examples
///
/// ## Storing custom device configuration
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use device_kit::flash_array::{FlashArray, FlashArrayStatic};
///
/// // Define your configuration type
/// #[derive(serde::Serialize, serde::Deserialize, Debug, Default)]
/// struct DeviceConfig {
///     brightness: u8,
///     timezone_offset: i16,
///     display_mode: u8,
/// }
///
/// async fn example(p: embassy_rp::Peripherals) -> device_kit::Result<()> {
///     static FLASH_STATIC: FlashArrayStatic = FlashArray::<1>::new_static();
///     let [mut device_config_block] = FlashArray::new(&FLASH_STATIC, p.FLASH)?;
///
///     // Load existing config if present.
///     let mut device_config: DeviceConfig = device_config_block
///         .load()?
///         .unwrap_or_default();
///
///     // Modify and save
///     device_config.brightness = 255;
///     device_config_block.save(&device_config)?;
///
///     // Can also clear storage with: device_config_block.clear()?;
///     Ok(())
/// }
/// ```
///
/// ## Whiteboard semantics demonstration
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use device_kit::flash_array::{FlashArray, FlashArrayStatic};
/// async fn example() -> device_kit::Result<()> {
///     let p = embassy_rp::init(Default::default());
///     static FLASH_STATIC: FlashArrayStatic = FlashArray::<1>::new_static();
///     let [mut string_block] = FlashArray::new(&FLASH_STATIC, p.FLASH)?;
///     string_block.save(&heapless::String::<64>::try_from("Hello")?)?;
///
///     // Reading with a different type returns None (whiteboard semantics)
///     let result: Option<u64> = string_block.load()?;
///     assert!(result.is_none());  // Different type (u64 vs String<64>)!
///     Ok(())
/// }
/// ```
/// Marker type used as a namespace for creating flash-backed arrays of length `N`.
pub struct FlashArray<const N: usize>;

impl<const N: usize> FlashArray<N> {
    /// Get static resources for creating flash arrays.
    #[must_use]
    pub const fn new_static() -> FlashArrayStatic {
        FlashArrayStatic::new_static()
    }

    /// Reserve `N` contiguous blocks (starting from block 0 on the first call) and return them as
    /// an array that you can destructure however you like.
    pub fn new(
        flash_static: &'static FlashArrayStatic,
        peripheral: Peri<'static, FLASH>,
    ) -> Result<[FlashBlock; N]> {
        let manager = flash_static.manager(peripheral);
        manager.reserve::<N>()
    }
}

fn save_block<T>(manager: &'static FlashManager, block: u32, value: &T) -> Result<()>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
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

    let mut buffer = [0xFFu8; ERASE_SIZE];
    buffer[0..4].copy_from_slice(&MAGIC.to_le_bytes());
    buffer[4..8].copy_from_slice(&compute_type_hash::<T>().to_le_bytes());
    buffer[8..10].copy_from_slice(&(payload_len as u16).to_le_bytes());
    buffer[HEADER_SIZE..HEADER_SIZE + payload_len].copy_from_slice(&payload_buffer[..payload_len]);

    let crc_offset = HEADER_SIZE + payload_len;
    let crc = compute_crc(&buffer[0..crc_offset]);
    buffer[crc_offset..crc_offset + CRC_SIZE].copy_from_slice(&crc.to_le_bytes());

    let offset = block_offset(block);
    manager.with_flash(|flash| {
        flash
            .blocking_erase(offset, offset + ERASE_SIZE as u32)
            .map_err(Error::Flash)?;
        flash
            .blocking_write(offset, &buffer)
            .map_err(Error::Flash)?;
        Ok(())
    })?;

    info!("Flash: Saved {} bytes to block {}", payload_len, block);
    Ok(())
}

fn load_block<T>(manager: &'static FlashManager, block: u32) -> Result<Option<T>>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let offset = block_offset(block);
    let mut buffer = [0u8; ERASE_SIZE];

    manager.with_flash(|flash| {
        flash
            .blocking_read(offset, &mut buffer)
            .map_err(Error::Flash)?;
        Ok(())
    })?;

    let magic = u32::from_le_bytes(buffer[0..4].try_into().unwrap());
    if magic != MAGIC {
        info!("Flash: No data at block {}", block);
        return Ok(None);
    }

    let stored_type_hash = u32::from_le_bytes(buffer[4..8].try_into().unwrap());
    let expected_type_hash = compute_type_hash::<T>();
    if stored_type_hash != expected_type_hash {
        info!(
            "Flash: Type mismatch at block {} (expected hash {}, found {})",
            block, expected_type_hash, stored_type_hash
        );
        return Ok(None);
    }

    let payload_len = u16::from_le_bytes(buffer[8..10].try_into().unwrap()) as usize;
    if payload_len > MAX_PAYLOAD_SIZE {
        error!(
            "Flash: Invalid payload length {} at block {}",
            payload_len, block
        );
        return Err(Error::StorageCorrupted);
    }

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
            block, computed_crc, stored_crc
        );
        return Err(Error::StorageCorrupted);
    }

    let payload = &buffer[HEADER_SIZE..HEADER_SIZE + payload_len];
    let value: T = postcard::from_bytes(payload).map_err(|_| {
        error!("Flash: Deserialization failed at block {}", block);
        Error::StorageCorrupted
    })?;

    info!("Flash: Loaded data from block {}", block);
    Ok(Some(value))
}

fn clear_block(manager: &'static FlashManager, block: u32) -> Result<()> {
    let offset = block_offset(block);
    manager.with_flash(|flash| {
        flash
            .blocking_erase(offset, offset + ERASE_SIZE as u32)
            .map_err(Error::Flash)?;
        Ok(())
    })?;
    info!("Flash: Cleared block {}", block);
    Ok(())
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
