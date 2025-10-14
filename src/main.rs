#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

const LCD_ADDRESS: u8 = 0x27; // I2C address of PCF8574

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{self, Config as I2cConfig};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_time::Timer;
use heapless::FnvIndexMap;
use lib::{Never, Result};
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

#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(clippy::cast_precision_loss, reason = "TODO")]
#[expect(clippy::assertions_on_constants, reason = "TODO")]
#[expect(clippy::too_many_lines, reason = "TODO")]
#[expect(clippy::cast_sign_loss, reason = "TODO")]
#[expect(clippy::cast_possible_truncation, reason = "TODO")]
async fn inner_main(_spawner: Spawner) -> Result<Never> {
    let p = embassy_rp::init(Default::default());

    // Initialize I2C for LCD (GP4=SDA, GP5=SCL)
    let sda = p.PIN_4;
    let scl = p.PIN_5;
    let mut i2c = i2c::I2c::new_blocking(p.I2C0, scl, sda, I2cConfig::default());
    
    // Initialize LCD using direct I2C commands
    lcd_init(&mut i2c).await;
    lcd_clear(&mut i2c).await;
    lcd_print(&mut i2c, "Hello").await;
    
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
    
    lcd_clear(&mut i2c).await;
    lcd_print(&mut i2c, "Scan card...").await;
    
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
                        let card_name = if let Some(&name) = card_map.get(&uid_key) {
                            // Card seen before
                            name
                        } else if card_map.len() < 4 {
                            // New card, assign next letter (A, B, C, or D)
                            #[expect(clippy::arithmetic_side_effects, reason = "Card count limited to 4")]
                            let name = b'A' + card_map.len() as u8;
                            let _ = card_map.insert(uid_key, name);
                            name
                        } else {
                            // More than 4 cards seen
                            b'?'
                        };
                        
                        // Display result on LCD
                        lcd_clear(&mut i2c).await;
                        
                        if card_name == b'?' {
                            lcd_print(&mut i2c, "Unknown Card").await;
                            lcd_write_byte(&mut i2c, 0xC0, false).await; // Line 2
                            lcd_print(&mut i2c, "Seen").await;
                        } else {
                            lcd_print(&mut i2c, "Card ").await;
                            lcd_write_byte(&mut i2c, card_name, true).await;
                            lcd_print(&mut i2c, " Seen").await;
                        }
                        
                        Timer::after_millis(2000).await;
                        lcd_clear(&mut i2c).await;
                        lcd_print(&mut i2c, "Scan card...").await;
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

// LCD helper functions for PCF8574 I2C backpack
// PCF8574 pin mapping: P0=RS, P1=RW, P2=E, P3=Backlight, P4-P7=Data
const LCD_BACKLIGHT: u8 = 0x08;
const LCD_ENABLE: u8 = 0x04;
const LCD_RS: u8 = 0x01;

#[expect(clippy::arithmetic_side_effects, reason = "Bit operations")]
async fn lcd_write_nibble(i2c: &mut embassy_rp::i2c::I2c<'_, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Blocking>, nibble: u8, rs: bool) {
    let rs_bit = if rs { LCD_RS } else { 0 };
    let data = (nibble << 4) | LCD_BACKLIGHT | rs_bit;
    
    // Write with enable high
    let _ = i2c.blocking_write(LCD_ADDRESS, &[data | LCD_ENABLE]);
    Timer::after_micros(1).await;
    
    // Write with enable low
    let _ = i2c.blocking_write(LCD_ADDRESS, &[data]);
    Timer::after_micros(50).await;
}

async fn lcd_write_byte(i2c: &mut embassy_rp::i2c::I2c<'_, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Blocking>, byte: u8, rs: bool) {
    lcd_write_nibble(i2c, (byte >> 4) & 0x0F, rs).await;
    lcd_write_nibble(i2c, byte & 0x0F, rs).await;
}

async fn lcd_init(i2c: &mut embassy_rp::i2c::I2c<'_, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Blocking>) {
    Timer::after_millis(50).await;
    
    // Initialize in 4-bit mode
    lcd_write_nibble(i2c, 0x03, false).await;
    Timer::after_millis(5).await;
    lcd_write_nibble(i2c, 0x03, false).await;
    Timer::after_micros(150).await;
    lcd_write_nibble(i2c, 0x03, false).await;
    lcd_write_nibble(i2c, 0x02, false).await;
    
    // Function set: 4-bit, 2 lines, 5x8 font
    lcd_write_byte(i2c, 0x28, false).await;
    // Display control: display on, cursor off, blink off
    lcd_write_byte(i2c, 0x0C, false).await;
    // Clear display
    lcd_write_byte(i2c, 0x01, false).await;
    Timer::after_millis(2).await;
    // Entry mode: increment cursor, no shift
    lcd_write_byte(i2c, 0x06, false).await;
}

async fn lcd_clear(i2c: &mut embassy_rp::i2c::I2c<'_, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Blocking>) {
    lcd_write_byte(i2c, 0x01, false).await;
    Timer::after_millis(2).await;
}

async fn lcd_print(i2c: &mut embassy_rp::i2c::I2c<'_, embassy_rp::peripherals::I2C0, embassy_rp::i2c::Blocking>, text: &str) {
    for ch in text.bytes() {
        lcd_write_byte(i2c, ch, true).await;
    }
}

// MFRC522 driver is now used instead of manual implementation

