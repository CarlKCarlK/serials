#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_time::Timer;
use heapless::FnvIndexMap;
use lib::{CharLcdI2c, Never, Result};
use mfrc522::comm::eh02::spi::SpiInterface;
use mfrc522::Mfrc522;
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

    // Initialize SPI for RFID (GP18=SCK, GP19=MOSI, GP16=MISO)
    // MFRC522 pin labeled "SDA" is actually NSS/CS (chip select)
    let mut spi_config = SpiConfig::default();
    spi_config.frequency = 1_000_000; // 1 MHz
    spi_config.polarity = embassy_rp::spi::Polarity::IdleLow;
    spi_config.phase = embassy_rp::spi::Phase::CaptureOnFirstTransition;
    let spi = Spi::new_blocking(p.SPI0, p.PIN_18, p.PIN_19, p.PIN_16, spi_config);
    
    // MFRC522 "SDA" pin (pin 7) = NSS/CS, connect to GP15 (Pico pin 20)
    let nss = Output::new(p.PIN_15, Level::High);  // GP15 = physical pin 20
    
    // Reset RFID module
    let mut rst = Output::new(p.PIN_17, Level::High);  // GP17 = physical pin 22
    rst.set_low();
    Timer::after_millis(10).await;
    rst.set_high();
    Timer::after_millis(50).await;
    
    // Initialize MFRC522 using driver - mfrc522 crate uses embedded-hal 0.2
    // Pass SPI and NSS separately to SpiInterface
    let spi_iface = SpiInterface::new(spi).with_nss(nss);
    let mut mfrc522 = match Mfrc522::new(spi_iface).init() {
        Ok(m) => {
            info!("MFRC522 driver initialized successfully");
            m
        }
        Err(_e) => {
            info!("MFRC522 init error");
            panic!("Failed to initialize MFRC522");
        }
    };
    
    // Check version
    match mfrc522.version() {
        Ok(v) => info!("MFRC522 Version: 0x{:02X}", v),
        Err(_e) => info!("Version read error"),
    }
    
    lcd.clear().await;
    lcd.print("Scan card...").await;
    
    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();
    
    // Main loop: check for RFID cards
    loop {
        // Try to detect a card
        match mfrc522.reqa() {
            Ok(atqa) => {
                info!("Card detected!");
                
                // Try to read UID
                match mfrc522.select(&atqa) {
                    Ok(uid) => {
                        let uid_bytes = uid.as_bytes();
                        info!("UID read successfully ({} bytes)", uid_bytes.len());
                        
                        // Create fixed-size UID key (pad with zeros if shorter than 10 bytes)
                        let mut uid_key = [0u8; 10];
                        #[expect(clippy::indexing_slicing, reason = "Length checked")]
                        for (i, &byte) in uid_bytes.iter().enumerate() {
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
                    }
                    Err(_e) => {
                        info!("UID read error");
                    }
                }
            }
            Err(_) => {
                // No card detected, silently continue
            }
        }
        
        Timer::after_millis(100).await;
    }
}

