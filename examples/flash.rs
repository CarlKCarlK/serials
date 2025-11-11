//! Flash storage example demonstrating type-safe persistent storage.
//!
//! This example demonstrates:
//! - Storing and loading different data types (String and struct) in separate blocks
//! - Type safety: attempting to read the wrong type returns None
//! - Clearing flash blocks
//!
//! The example uses blocks 3 and 4 to avoid conflicts with WiFi credentials (block 0)
//! and timezone offset (block 1).

#![no_std]
#![no_main]

use defmt::{info, assert_eq};
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::flash::{Blocking, Flash};
use embassy_rp::peripherals::FLASH;
use panic_probe as _;
use serde::{Deserialize, Serialize};
use static_cell::StaticCell;

use serials::flash_block::{FlashBlock, INTERNAL_FLASH_SIZE};
use serials::Result;

// ============================================================================
// Test Data Structures
// ============================================================================

/// A simple struct to demonstrate storing custom types in flash
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, defmt::Format)]
struct SensorConfig {
    name: heapless::String<32>,
    sample_rate_hz: u32,
    enabled: bool,
}

// ============================================================================
// Main
// ============================================================================

static FLASH_STORAGE: StaticCell<Flash<'static, FLASH, Blocking, INTERNAL_FLASH_SIZE>> =
    StaticCell::new();

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let err = inner_main(_spawner).await.unwrap_err();
    panic!("Example failed: {:?}", err);
}

async fn inner_main(_spawner: Spawner) -> Result<()> {
    info!("==========================================================");
    info!("Flash Storage Example");
    info!("==========================================================");
    info!("");

    // Initialize hardware
    let p = embassy_rp::init(Default::default());
    let flash = FLASH_STORAGE.init(Flash::<_, Blocking, INTERNAL_FLASH_SIZE>::new_blocking(
        p.FLASH,
    ));

    // ========================================================================
    // Part 1: Store data to flash blocks
    // ========================================================================
    
    info!("Part 1: Storing data to flash");
    info!("------------------------------");

    // Store a string in block 3
    let test_string = heapless::String::<64>::try_from("Hello, Flash Storage!").unwrap();
    let mut block3: FlashBlock<FLASH, heapless::String<64>> = FlashBlock::new(3);
    block3.save(&mut *flash, &test_string)?;
    info!("Stored string to block 3: \"{}\"", test_string.as_str());

    // Store a struct in block 4
    let sensor_config = SensorConfig {
        name: heapless::String::try_from("Temperature").unwrap(),
        sample_rate_hz: 1000,
        enabled: true,
    };
    let mut block4: FlashBlock<FLASH, SensorConfig> = FlashBlock::new(4);
    block4.save(&mut *flash, &sensor_config)?;
    info!("Stored SensorConfig to block 4: {:?}", sensor_config);
    info!("");

    // ========================================================================
    // Part 2: Read data from flash blocks
    // ========================================================================
    
    info!("Part 2: Reading data from flash");
    info!("--------------------------------");

    // Read string from block 3
    let mut block3_read: FlashBlock<FLASH, heapless::String<64>> = FlashBlock::new(3);
    let loaded_string = block3_read.load(&mut *flash)?;
    info!("Read from block 3: {:?}", loaded_string);
    assert_eq!(loaded_string, Some(test_string.clone()));
    info!("✓ String matches expected value");

    // Read struct from block 4
    let mut block4_read: FlashBlock<FLASH, SensorConfig> = FlashBlock::new(4);
    let loaded_config = block4_read.load(&mut *flash)?;
    info!("Read from block 4: {:?}", loaded_config);
    assert_eq!(loaded_config, Some(sensor_config.clone()));
    info!("✓ SensorConfig matches expected value");
    info!("");

    // ========================================================================
    // Part 3: Type safety - attempt to read wrong type
    // ========================================================================
    
    info!("Part 3: Testing type safety");
    info!("----------------------------");

    // Try to read block 3 (which contains a String) as a SensorConfig
    let mut block3_wrong_type: FlashBlock<FLASH, SensorConfig> = FlashBlock::new(3);
    let wrong_type_result = block3_wrong_type.load(&mut *flash)?;
    info!("Attempted to read block 3 as SensorConfig: {:?}", wrong_type_result);
    assert_eq!(wrong_type_result, None);
    info!("✓ Type mismatch correctly returns None (type safety working!)");
    info!("");

    // ========================================================================
    // Part 4: Clear flash blocks
    // ========================================================================
    
    info!("Part 4: Clearing flash blocks");
    info!("------------------------------");

    block3.clear(&mut *flash)?;
    info!("Cleared block 3");

    block4.clear(&mut *flash)?;
    info!("Cleared block 4");
    info!("");

    // ========================================================================
    // Part 5: Verify blocks are empty
    // ========================================================================
    
    info!("Part 5: Verifying cleared blocks");
    info!("---------------------------------");

    // Read from cleared block 3
    let cleared_string = block3_read.load(&mut *flash)?;
    info!("Read from cleared block 3: {:?}", cleared_string);
    assert_eq!(cleared_string, None);
    info!("✓ Block 3 is empty as expected");

    // Read from cleared block 4
    let cleared_config = block4_read.load(&mut *flash)?;
    info!("Read from cleared block 4: {:?}", cleared_config);
    assert_eq!(cleared_config, None);
    info!("✓ Block 4 is empty as expected");
    info!("");

    // ========================================================================
    // Summary
    // ========================================================================
    
    info!("==========================================================");
    info!("Flash Storage Example Complete!");
    info!("==========================================================");
    info!("✓ All tests passed");
    info!("  - Stored and retrieved String from block 3");
    info!("  - Stored and retrieved SensorConfig from block 4");
    info!("  - Type safety verified (wrong type returns None)");
    info!("  - Cleared blocks verified (both empty)");
    info!("");
    info!("Example demonstrates:");
    info!("  • Type-safe flash storage with compile-time guarantees");
    info!("  • Automatic serialization/deserialization");
    info!("  • CRC32 validation and type hash verification");
    info!("  • Independent block management");
    info!("");
    
    loop {
        embassy_time::Timer::after_secs(1).await;
    }
}
