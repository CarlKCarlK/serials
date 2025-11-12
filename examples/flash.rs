//! Flash storage example demonstrating type-safe persistent storage.
//!
//! This example demonstrates:
//! - Storing and loading different data types (String and struct) in separate blocks
//! - Type safety: attempting to read the wrong type returns None
//! - Clearing flash blocks

#![no_std]
#![no_main]

use heapless::String;
use defmt::{assert_eq, info};
use defmt_rtt as _;
use embassy_executor::Spawner;
use panic_probe as _;
use serde::{Deserialize, Serialize};

use serials::Result;
use serials::flash::{Flash, FlashNotifier};

// ============================================================================
// Test Data Structures
// ============================================================================

/// A simple struct to demonstrate storing custom types in flash
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, defmt::Format)]
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
    let mut flash = Flash::new(&FLASH_NOTIFIER, p.FLASH);

    info!("Part 1: Storing data to flash");
    flash.save(3, &String::<64>::try_from("Hello, Flash Storage!")?)?;
    flash.save(
        4,
        &SensorConfig {
            name: String::<32>::try_from("Temperature")?,
            sample_rate_hz: 1000,
            enabled: true,
        },
    )?;

    info!("Part 2: Reading data from flash");
    let string: Option<String<64>> = flash.load(3)?;
    assert_eq!(string.as_deref(), Some("Hello, Flash Storage!"));
    let config: Option<SensorConfig> = flash.load(4)?;
    assert_eq!(config, Some(SensorConfig {
        name: String::<32>::try_from("Temperature")?,
        sample_rate_hz: 1000,
        enabled: true,
    }));

    info!("Part 3: Reading a different type counts as empty");
    // Try to read block 3 (which contains a String) as a SensorConfig
    let wrong_type_result: Option<SensorConfig> = flash.load(3)?;
    assert_eq!(wrong_type_result, None);

    info!("Part 4: Clearing flash blocks");
    flash.clear(3)?;
    flash.clear(4)?;

    info!("Part 5: Verifying cleared blocks");
    let string: Option<String<64>> = flash.load(3)?;
    assert_eq!(string, None);
    let config: Option<SensorConfig> = flash.load(4)?;
    assert_eq!(config, None);

    info!("Flash Storage Example Complete!");
    loop {
        embassy_time::Timer::after_secs(1).await;
    }
}
