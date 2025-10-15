#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_time::{Instant, Timer};
use embedded_hal_bus::spi::ExclusiveDevice;
use esp_hal_mfrc522::consts::{PCDErrorCode, UidSize};
use esp_hal_mfrc522::drivers::SpiDriver;
use esp_hal_mfrc522::MFRC522;
use heapless::FnvIndexMap;
use lib::{CharLcdI2c, Never, Result};
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<Never> {
    let p = embassy_rp::init(Default::default());

    // Initialize LCD (GP4=SDA, GP5=SCL)
    let mut lcd = CharLcdI2c::new(p.I2C0, p.PIN_5, p.PIN_4).await;
    lcd.clear().await;
    lcd.print("Hello").await;
    
    info!("LCD initialized and displaying Hello");

    // Initialize async SPI for RFID (GP18=SCK, GP19=MOSI, GP16=MISO)
    let mut spi_config = SpiConfig::default();
    spi_config.frequency = 1_000_000; // 1 MHz
    spi_config.polarity = embassy_rp::spi::Polarity::IdleLow;
    spi_config.phase = embassy_rp::spi::Phase::CaptureOnFirstTransition;
    let spi = Spi::new(p.SPI0, p.PIN_18, p.PIN_19, p.PIN_16, p.DMA_CH0, p.DMA_CH1, spi_config);
    
    // CS pin for MFRC522
    let cs = Output::new(p.PIN_15, Level::High);  // GP15 = physical pin 20
    
    // Reset RFID module
    let mut rst = Output::new(p.PIN_17, Level::High);  // GP17 = physical pin 22
    rst.set_low();
    Timer::after_millis(10).await;
    rst.set_high();
    Timer::after_millis(50).await;
    
    // Initialize MFRC522 using async driver
    // Wrap SPI+CS in ExclusiveDevice to implement SpiDevice trait
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs).expect("CS pin is infallible");
    let spi_driver = SpiDriver::new(spi_device);
    let mut mfrc522 = MFRC522::new(spi_driver, || {
        Instant::now().as_millis()
    });
    
    let _: Result<(), PCDErrorCode> = mfrc522.pcd_init().await;
    info!("MFRC522 initialized");
    
    match mfrc522.pcd_get_version().await {
        Ok(_v) => info!("MFRC522 Version read successfully"),
        Err(_e) => info!("Version read error"),
    }
    
    lcd.clear().await;
    lcd.print("Scan card...").await;
    
    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();
    
    // Main loop: check for RFID cards
    loop {
        // Try to detect a card (async!)
        let Ok(()) = mfrc522.picc_is_new_card_present().await else {
            Timer::after_millis(100).await;
            continue;
        };
        
        info!("Card detected!");
        
        // Try to read UID (async!)
        let Ok(uid) = mfrc522.get_card(UidSize::Four).await else {
            info!("UID read error");
            Timer::after_millis(100).await;
            continue;
        };
        
        info!("UID read successfully ({} bytes)", uid.uid_bytes.len());
        
        // Create fixed-size UID key (pad with zeros if shorter than 10 bytes)
        let mut uid_key = [0u8; 10];
        #[expect(clippy::indexing_slicing, reason = "Length checked")]
        for (i, &byte) in uid.uid_bytes.iter().enumerate() {
            if i < 10 {
                uid_key[i] = byte;
            }
        }
        
        // Look up or assign card name
        let card_name = card_map.get(&uid_key).copied().or_else(|| {
            // Try to assign next letter (A, B, C, D...)
            #[expect(clippy::arithmetic_side_effects, reason = "Card count limited by map capacity")]
            let name = b'A' + card_map.len() as u8;
            card_map.insert(uid_key, name).ok().map(|_| name)
        });
        
        // Display result on LCD
        lcd.clear().await;
        
        if let Some(name) = card_name {
            lcd.print("Card ").await;
            lcd.write_byte(name).await;
            lcd.print(" Seen").await;
        } else {
            lcd.print("Unknown Card").await;
            lcd.set_cursor(1, 0).await;
            lcd.print("Seen").await;
        }
        
        Timer::after_millis(2000).await;
        lcd.clear().await;
        lcd.print("Scan card...").await;
        
        Timer::after_millis(100).await;
    }
}

