//! Storage for timezone offset in flash memory.
#![cfg(feature = "wifi")]

use crc32fast::Hasher;
use embassy_rp::flash::Instance;
use embassy_rp::flash::{Blocking, ERASE_SIZE, Flash};

use crate::credential_store::INTERNAL_FLASH_SIZE;
use crate::{Error, Result};

pub const MIN_OFFSET_MINUTES: i32 = -12 * 60;
pub const MAX_OFFSET_MINUTES: i32 = 14 * 60;

const STORAGE_SIZE: usize = ERASE_SIZE;
const MAGIC: u32 = 0x545A_4F46; // 'TZOF'
const VERSION: u16 = 1;
const VERSION_OFFSET: usize = 4;
const RESERVED_OFFSET: usize = 6;
const OFFSET_OFFSET: usize = 8;
const CRC_OFFSET: usize = 12;

/// Load the persisted timezone offset in minutes.
pub fn load<'d, T: Instance>(
    flash: &mut Flash<'d, T, Blocking, INTERNAL_FLASH_SIZE>,
) -> Result<Option<i32>> {
    let offset = storage_offset(flash);
    let mut buffer = [0u8; STORAGE_SIZE];
    flash
        .blocking_read(offset, &mut buffer)
        .map_err(Error::Flash)?;

    if u32::from_le_bytes(buffer[..VERSION_OFFSET].try_into().unwrap()) != MAGIC {
        return Ok(None);
    }

    let version = u16::from_le_bytes(buffer[VERSION_OFFSET..RESERVED_OFFSET].try_into().unwrap());
    if version != VERSION {
        return Ok(None);
    }

    let crc_stored = u32::from_le_bytes(buffer[CRC_OFFSET..CRC_OFFSET + 4].try_into().unwrap());
    let crc = compute_crc(&buffer[VERSION_OFFSET..CRC_OFFSET]);
    if crc != crc_stored {
        return Err(Error::CredentialStorageCorrupted);
    }

    let offset_minutes = i32::from_le_bytes(buffer[OFFSET_OFFSET..CRC_OFFSET].try_into().unwrap());
    Ok(Some(offset_minutes))
}

/// Persist the timezone offset (in minutes) to flash.
pub fn save<'d, T: Instance>(
    flash: &mut Flash<'d, T, Blocking, INTERNAL_FLASH_SIZE>,
    offset_minutes: i32,
) -> Result<()> {
    if offset_minutes < MIN_OFFSET_MINUTES || offset_minutes > MAX_OFFSET_MINUTES {
        return Err(Error::FormatError);
    }

    let offset = storage_offset(flash);
    let mut buffer = [0xFFu8; STORAGE_SIZE];
    buffer[..VERSION_OFFSET].copy_from_slice(&MAGIC.to_le_bytes());
    buffer[VERSION_OFFSET..RESERVED_OFFSET].copy_from_slice(&VERSION.to_le_bytes());
    buffer[RESERVED_OFFSET..OFFSET_OFFSET].fill(0);
    buffer[OFFSET_OFFSET..CRC_OFFSET].copy_from_slice(&offset_minutes.to_le_bytes());

    let crc = compute_crc(&buffer[VERSION_OFFSET..CRC_OFFSET]);
    buffer[CRC_OFFSET..CRC_OFFSET + 4].copy_from_slice(&crc.to_le_bytes());

    flash
        .blocking_erase(offset, offset + STORAGE_SIZE as u32)
        .map_err(Error::Flash)?;
    flash
        .blocking_write(offset, &buffer)
        .map_err(Error::Flash)?;
    Ok(())
}

/// Remove the persisted timezone offset from flash.
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
    capacity - (STORAGE_SIZE as u32 * 2)
}

fn compute_crc(data: &[u8]) -> u32 {
    let mut hasher = Hasher::new();
    hasher.update(data);
    hasher.finalize()
}
