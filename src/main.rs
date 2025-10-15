#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use esp_hal_mfrc522::consts::UidSize;
use heapless::index_map::FnvIndexMap;
use lib::{new_spi_mfrc522, CharLcdI2c, Never, Result};
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

    // Initialize MFRC522 RFID reader
    let mut mfrc522 = new_spi_mfrc522(
        p.SPI0,
        p.PIN_18,
        p.PIN_19,
        p.PIN_16,
        p.DMA_CH0,
        p.DMA_CH1,
        p.PIN_15,
        p.PIN_17,
    ).await;
    
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

