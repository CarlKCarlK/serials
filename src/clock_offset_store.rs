//! Storage for timezone offset in flash memory.
//!
//! This module provides functions to save, load, and clear timezone offset values
//! in the Raspberry Pi Pico's internal flash memory. The offset is stored using the
//! [`crate::flash_block`] module with type-safe postcard serialization.
//!
//! You can choose any block ID for storage - just ensure it doesn't conflict with
//! other data you're storing in flash.
//!
//! # Examples
//!
//! ## Saving and loading timezone offset
//!
//! ```no_run
//! use embassy_rp::flash::{Blocking, Flash};
//! use serials::flash_block::INTERNAL_FLASH_SIZE;
//! use serials::clock_offset_store;
//!
//! # async fn example() -> serials::Result<()> {
//! let p = embassy_rp::init(Default::default());
//! let mut flash = Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(p.FLASH);
//!
//! // Save timezone offset (e.g., UTC-5 = -300 minutes) to block_id = 1
//! clock_offset_store::save(&mut flash, -300, 1)?
//!
//! // Load timezone offset from block_id = 1
//! if let Some(offset) = clock_offset_store::load(&mut flash, 1)? {
//!     defmt::info!("Timezone offset: {} minutes", offset);
//! }
//! # Ok(())
//! # }
//! ```
#![cfg(feature = "wifi")]

use embassy_rp::flash::{Blocking, Flash};

use crate::flash_block::{FlashBlock, INTERNAL_FLASH_SIZE};
use crate::Result;

/// Minimum timezone offset in minutes (UTC-12).
pub const MIN_OFFSET_MINUTES: i32 = -12 * 60;

/// Maximum timezone offset in minutes (UTC+14).
pub const MAX_OFFSET_MINUTES: i32 = 14 * 60;

/// Newtype wrapper for timezone offset in minutes.
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
struct TimezoneOffset(i32);

/// Load the persisted timezone offset in minutes.
///
/// Returns `Ok(Some(offset))` if a valid offset is found, `Ok(None)` if no offset
/// is stored, or `Err` if the stored data is corrupted.
///
/// # Arguments
///
/// * `flash` - Flash peripheral
/// * `block_id` - Block ID where the timezone offset is stored (e.g., 1)
///
/// See the [module-level documentation](crate::clock_offset_store) for usage examples.
pub fn load(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    block_id: u32,
) -> Result<Option<i32>> {
    let mut block: FlashBlock<embassy_rp::peripherals::FLASH, TimezoneOffset> = FlashBlock::new(block_id);
    Ok(block.load(flash)?.map(|tz: TimezoneOffset| tz.0))
}

/// Persist the timezone offset (in minutes) to flash.
///
/// The offset must be within the range [`MIN_OFFSET_MINUTES`] to [`MAX_OFFSET_MINUTES`].
///
/// # Arguments
///
/// * `flash` - Flash peripheral
/// * `offset_minutes` - Timezone offset in minutes (e.g., UTC-5 = -300)
/// * `block_id` - Block ID where the timezone offset should be stored (e.g., 1)
///
/// # Errors
///
/// Returns `Err(Error::FormatError)` if the offset is out of range.
/// Returns `Err(Error::Flash)` if flash operations fail.
///
/// See the [module-level documentation](crate::clock_offset_store) for usage examples.
pub fn save(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    offset_minutes: i32,
    block_id: u32,
) -> Result<()> {
    use crate::Error;

    if offset_minutes < MIN_OFFSET_MINUTES || offset_minutes > MAX_OFFSET_MINUTES {
        return Err(Error::FormatError);
    }

    let mut block: FlashBlock<embassy_rp::peripherals::FLASH, TimezoneOffset> = FlashBlock::new(block_id);
    block.save(flash, &TimezoneOffset(offset_minutes))
}

/// Remove the persisted timezone offset from flash.
///
/// Erases the flash sector containing the offset. After calling this function,
/// [`load`] will return `Ok(None)`.
///
/// # Arguments
///
/// * `flash` - Flash peripheral
/// * `block_id` - Block ID where the timezone offset is stored (must match the block used in save/load)
///
/// # Errors
///
/// Returns `Err(Error::Flash)` if the erase operation fails.
///
/// See the [module-level documentation](crate::clock_offset_store) for usage examples.
pub fn clear(
    flash: &mut Flash<'_, embassy_rp::peripherals::FLASH, Blocking, INTERNAL_FLASH_SIZE>,
    block_id: u32,
) -> Result<()> {
    let mut block: FlashBlock<embassy_rp::peripherals::FLASH, TimezoneOffset> = FlashBlock::new(block_id);
    block.clear(flash)
}
