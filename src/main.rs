#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use heapless::index_map::FnvIndexMap;
use lib::{CharLcdI2c, Never, Result, SpiMfrc522Notifier, SpiMfrc522Reader};
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

static RFID_NOTIFIER: SpiMfrc522Notifier = SpiMfrc522Reader::notifier();

async fn inner_main(spawner: Spawner) -> Result<Never> {
    let p = embassy_rp::init(Default::default());

    // Initialize LCD (GP4=SDA, GP5=SCL)
    let mut lcd = CharLcdI2c::new(p.I2C0, p.PIN_5, p.PIN_4).await;
    lcd.clear().await;
    lcd.print("Hello").await;
    
    info!("LCD initialized and displaying Hello");

    // Initialize MFRC522 RFID reader device abstraction
    let rfid_reader = SpiMfrc522Reader::new(
        p.SPI0,
        p.PIN_18,
        p.PIN_19,
        p.PIN_16,
        p.DMA_CH0,
        p.DMA_CH1,
        p.PIN_15,
        p.PIN_17,
        &RFID_NOTIFIER,
        spawner,
    ).await?;
    
    lcd.clear().await;
    lcd.print("Scan card...").await;
    
    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();
    
    // Main loop: wait for RFID cards
    loop {
        // Wait for next card (clean async interface - polling happens in background task)
        let uid_key = rfid_reader.next_card().await;
        
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

