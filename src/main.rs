#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

const HEAP_SIZE: usize = 1024 * 350; // in bytes
const LCD_ADDRESS: u8 = 0x27; // I2C address of PCF8574

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

use alloc_cortex_m::CortexMHeap;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_rp::i2c::{self, Config as I2cConfig};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_time::Timer;
use lib::{Never, Result};
use malachite::num::arithmetic::traits::SquareAssign;
use malachite::Natural;
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

// SPI device wrapper with CS pin for MFRC522
struct SpiDeviceWithCs {
    spi: Spi<'static, embassy_rp::peripherals::SPI0, embassy_rp::spi::Blocking>,
    cs: Output<'static>,
}

impl embedded_hal::spi::ErrorType for SpiDeviceWithCs {
    type Error = embassy_rp::spi::Error;
}

impl embedded_hal::spi::SpiDevice for SpiDeviceWithCs {
    fn transaction(&mut self, operations: &mut [embedded_hal::spi::Operation<'_, u8>]) -> core::result::Result<(), Self::Error> {
        self.cs.set_low();
        for op in operations {
            match op {
                embedded_hal::spi::Operation::Read(buf) => {
                    self.spi.blocking_read(buf)?;
                }
                embedded_hal::spi::Operation::Write(buf) => {
                    self.spi.blocking_write(buf)?;
                }
                embedded_hal::spi::Operation::Transfer(read, write) => {
                    self.spi.blocking_transfer(read, write)?;
                }
                embedded_hal::spi::Operation::TransferInPlace(buf) => {
                    self.spi.blocking_transfer_in_place(buf)?;
                }
                _ => {}
            }
        }
        self.cs.set_high();
        Ok(())
    }
}

#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(unsafe_code, reason = "TODO")]
#[expect(clippy::cast_precision_loss, reason = "TODO")]
#[expect(clippy::assertions_on_constants, reason = "TODO")]
#[expect(clippy::too_many_lines, reason = "TODO")]
#[expect(clippy::cast_sign_loss, reason = "TODO")]
#[expect(clippy::cast_possible_truncation, reason = "TODO")]
async fn inner_main(_spawner: Spawner) -> Result<Never> {
    unsafe { ALLOCATOR.init(cortex_m_rt::heap_start() as usize, HEAP_SIZE) }

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
    let mut spi = Spi::new_blocking(p.SPI0, p.PIN_18, p.PIN_19, p.PIN_16, spi_config);
    
    // MFRC522 "SDA" pin (pin 7) = NSS/CS, connect to GP15 (Pico pin 20)
    let mut nss = Output::new(p.PIN_15, Level::High);  // GP15 = physical pin 20
    
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
                        
                        // Display UID on LCD - handle long UIDs by scrolling or splitting
                        lcd_clear(&mut i2c).await;
                        
                        if uid_bytes.len() <= 4 {
                            // Short UID - show on one line with label
                            lcd_print(&mut i2c, "UID:").await;
                            lcd_write_byte(&mut i2c, 0xC0, false).await; // Move to line 2
                            
                            for (i, byte) in uid_bytes.iter().enumerate() {
                                if i > 0 {
                                    lcd_write_byte(&mut i2c, b' ', true).await;
                                }
                                let hex_chars = format_hex_byte(*byte);
                                lcd_write_byte(&mut i2c, hex_chars.0, true).await;
                                lcd_write_byte(&mut i2c, hex_chars.1, true).await;
                            }
                        } else {
                            // Long UID - split across two lines (first 4 bytes on line 1, rest on line 2)
                            for (i, byte) in uid_bytes.iter().enumerate() {
                                if i == 4 {
                                    lcd_write_byte(&mut i2c, 0xC0, false).await; // Move to line 2
                                } else if i > 0 {
                                    lcd_write_byte(&mut i2c, b' ', true).await;
                                }
                                let hex_chars = format_hex_byte(*byte);
                                lcd_write_byte(&mut i2c, hex_chars.0, true).await;
                                lcd_write_byte(&mut i2c, hex_chars.1, true).await;
                            }
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

// Helper function to format byte as hex
fn format_hex_byte(byte: u8) -> (u8, u8) {
    const HEX: &[u8] = b"0123456789ABCDEF";
    #[expect(clippy::arithmetic_side_effects, reason = "Hex conversion")]
    #[expect(clippy::indexing_slicing, reason = "Always valid for 4-bit values")]
    let high = HEX[(byte >> 4) as usize];
    #[expect(clippy::arithmetic_side_effects, reason = "Hex conversion")]
    #[expect(clippy::indexing_slicing, reason = "Always valid for 4-bit values")]
    let low = HEX[(byte & 0x0F) as usize];
    (high, low)
}

fn fibonacci(n: usize) -> Natural {
    // fib_fast(n).0 // fib_fast(n-1).1
    fib_two_step(n)
}

#[expect(dead_code, reason = "TODO")]
#[expect(clippy::min_ident_chars, reason = "cmk")]
#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(clippy::integer_division_remainder_used, reason = "cmk")]
fn fib_two_step(n: usize) -> Natural {
    if n == 0 {
        return Natural::from(0usize);
    }
    let mut a = Natural::from(0usize);
    let mut b = Natural::from(1usize);
    for _ in 0..((n - 1) / 2) {
        a += &b;
        b += &a;
    }

    if is_even(n) {
        a + b
    } else {
        b
    }
}

#[inline]
const fn is_even(n: usize) -> bool {
    n & 1 == 0
}

const TWO: Natural = Natural::const_from(2);

#[expect(clippy::many_single_char_names, reason = "TODO")]
#[expect(clippy::min_ident_chars, reason = "cmk")]
#[expect(clippy::arithmetic_side_effects, reason = "TODO")]
#[expect(clippy::integer_division_remainder_used, reason = "cmk")]
#[must_use]
pub fn fib_fast(n: usize) -> (Natural, Natural) {
    if n == 0 {
        return (Natural::from(0usize), Natural::from(1usize));
    }

    let (a, mut b) = fib_fast(n / 2);
    let mut c = b.clone();
    c *= TWO;
    c -= &a;
    c *= &a;

    let mut d = a;
    d.square_assign();
    b.square_assign();
    d += &b;

    // let d = &a * &a + &b * &b;
    if n % 2 == 0 {
        (c, d)
    } else {
        c += &d;
        (d, c)
    }
}

// LCD helper functions for PCF8574 I2C backpack
// PCF8574 pin mapping: P0=RS, P1=RW, P2=E, P3=Backlight, P4-P7=Data
const LCD_BACKLIGHT: u8 = 0x08;
const LCD_ENABLE: u8 = 0x04;
const LCD_RW: u8 = 0x02;
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

