//! Flash storage example demonstrating type-safe persistent storage.
//!
//! This example demonstrates:
//! - Storing and loading different data types (String and struct) in separate blocks
//! - Type safety: attempting to read the wrong type returns None
//! - Clearing flash blocks

#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use heapless::String;
use panic_probe as _;
use serde::{Deserialize, Serialize};

use serials::Result;
use serials::flash::{Flash, FlashNotifier};

// ============================================================================
// Test Data Structures
// ============================================================================

/// A simple struct to demonstrate storing custom types in flash
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SensorConfig {
    name: String<32>,
    sample_rate_hz: u32,
    enabled: bool,
}

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let err = inner_main(_spawner).await.unwrap_err();
    panic!("Example failed: {:?}", err);
}

async fn inner_main(_spawner: Spawner) -> Result<()> {
    info!("Flash Storage Example");

    // Initialize hardware
    let p = embassy_rp::init(Default::default());

    // Initialize Flash device using the notifier pattern
    static FLASH_NOTIFIER: FlashNotifier = Flash::notifier();
    let flash = Flash::new(&FLASH_NOTIFIER, p.FLASH);

    // Split the flash into two single-block partitions for this example.
    let (mut string_block, rest) = flash.take_first();
    let (mut config_block, _) = rest.take_first();

    info!("Part 1: Storing data to flash");
    string_block.save(0, &String::<64>::try_from("Hello, Flash Storage!")?)?;
    config_block.save(
        0,
        &SensorConfig {
            name: String::<32>::try_from("Temperature")?,
            sample_rate_hz: 1000,
            enabled: true,
        },
    )?;

    info!("Part 2: Reading data from flash");
    let string: Option<String<64>> = string_block.load(0)?;
    assert!(string.as_deref() == Some("Hello, Flash Storage!"));
    let config: Option<SensorConfig> = config_block.load(0)?;
    assert!(
        config
            == Some(SensorConfig {
                name: String::<32>::try_from("Temperature")?,
                sample_rate_hz: 1000,
                enabled: true,
            })
    );

    info!("Part 3: Reading a different type counts as empty");
    // Try to read the string block as a SensorConfig
    let wrong_type_result: Option<SensorConfig> = string_block.load(0)?;
    assert!(wrong_type_result.is_none());

    info!("Part 4: Clearing flash blocks");
    string_block.clear(0)?;
    config_block.clear(0)?;

    info!("Part 5: Verifying cleared blocks");
    let string: Option<String<64>> = string_block.load(0)?;
    assert!(string.is_none());
    let config: Option<SensorConfig> = config_block.load(0)?;
    assert!(config.is_none());

    info!("Flash Storage Example Complete!");
    loop {
        embassy_time::Timer::after_secs(1).await;
    }
}
