//! A device abstraction for HD44780-compatible character LCDs (e.g., 16x2, 20x2, 20x4).

use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::i2c::{self, Config as I2cConfig, SclPin, SdaPin};
use embassy_rp::peripherals::I2C0;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Timer;
use heapless::String;

use crate::{Error, Result};

/// Messages sent to the character LCD device.
#[derive(Clone, Debug)]
pub enum CharLcdMessage {
    /// Display a message for the specified duration (0 = until next message)
    Display {
        text: String<64>, // 64 chars supports up to 20x4 displays (80 chars)
        duration_ms: u32,
    },
}

/// Notifier type for the `CharLcd` device abstraction.
pub type CharLcdNotifier = Channel<CriticalSectionRawMutex, CharLcdMessage, 8>;

/// A device abstraction for an HD44780-compatible character LCD.
pub struct CharLcd {
    notifier: &'static CharLcdNotifier,
}

impl CharLcd {
    /// Create CharLcd resources
    #[must_use]
    pub const fn notifier() -> CharLcdNotifier {
        Channel::new()
    }

    /// Create a new CharLcd device
    ///
    /// Note: Hardcoded to I2C0 peripheral (like WiFi's internal pins).
    /// However, SCL and SDA can be any pins compatible with I2C0.
    pub fn new<SCL, SDA>(
        i2c_peripheral: Peri<'static, I2C0>,
        scl: Peri<'static, SCL>,
        sda: Peri<'static, SDA>,
        char_lcd_notifier: &'static CharLcdNotifier,
        spawner: Spawner,
    ) -> Result<Self>
    where
        SCL: SclPin<I2C0>,
        SDA: SdaPin<I2C0>,
    {
        // Create the I2C instance and pass it to the task
        let i2c = i2c::I2c::new_blocking(i2c_peripheral, scl, sda, I2cConfig::default());
        let token = lcd_task(i2c, char_lcd_notifier).map_err(Error::TaskSpawn)?;
        spawner.spawn(token);
        Ok(Self {
            notifier: char_lcd_notifier,
        })
    }

    /// Send a message to the LCD (async, waits until queued)
    pub async fn display(&self, text: String<64>, duration_ms: u32) {
        self.notifier
            .send(CharLcdMessage::Display { text, duration_ms })
            .await;
    }
}

// Internal LCD driver implementation (used by the background task)
struct LcdDriver {
    i2c: i2c::I2c<'static, I2C0, i2c::Blocking>,
    address: u8,
}

// PCF8574 pin mapping: P0=RS, P1=RW, P2=E, P3=Backlight, P4-P7=Data
const LCD_BACKLIGHT: u8 = 0x08;
const LCD_ENABLE: u8 = 0x04;
const LCD_RS: u8 = 0x01;

impl LcdDriver {
    fn new(i2c: i2c::I2c<'static, I2C0, i2c::Blocking>) -> Self {
        Self { i2c, address: 0x27 }
    }

    async fn init(&mut self) {
        Timer::after_millis(50).await;

        // Initialize in 4-bit mode
        self.write_nibble(0x03, false).await;
        Timer::after_millis(5).await;
        self.write_nibble(0x03, false).await;
        Timer::after_micros(150).await;
        self.write_nibble(0x03, false).await;
        self.write_nibble(0x02, false).await;

        // Function set: 4-bit, 2 lines, 5x8 font
        self.write_byte_internal(0x28, false).await;
        // Display control: display on, cursor off, blink off
        self.write_byte_internal(0x0C, false).await;
        // Clear display
        self.write_byte_internal(0x01, false).await;
        Timer::after_millis(2).await;
        // Entry mode: increment cursor, no shift
        self.write_byte_internal(0x06, false).await;
    }

    #[expect(clippy::arithmetic_side_effects, reason = "Bit operations")]
    async fn write_nibble(&mut self, nibble: u8, rs: bool) {
        let rs_bit = if rs { LCD_RS } else { 0 };
        let data = (nibble << 4) | LCD_BACKLIGHT | rs_bit;

        // Write with enable high
        let _ = self.i2c.blocking_write(self.address, &[data | LCD_ENABLE]);
        Timer::after_micros(1).await;

        // Write with enable low
        let _ = self.i2c.blocking_write(self.address, &[data]);
        Timer::after_micros(50).await;
    }

    async fn write_byte_internal(&mut self, byte: u8, rs: bool) {
        self.write_nibble((byte >> 4) & 0x0F, rs).await;
        self.write_nibble(byte & 0x0F, rs).await;
    }

    async fn clear(&mut self) {
        self.write_byte_internal(0x01, false).await;
        Timer::after_millis(2).await;
    }

    #[expect(clippy::arithmetic_side_effects, reason = "Row/col values are small")]
    async fn set_cursor(&mut self, row: u8, col: u8) {
        let address = match row {
            0 => 0x00 + col,
            1 => 0x40 + col,
            2 => 0x14 + col,
            3 => 0x54 + col,
            _ => 0x00,
        };
        self.write_byte_internal(0x80 | address, false).await;
    }

    async fn print(&mut self, s: &str) {
        for ch in s.bytes() {
            self.write_byte_internal(ch, true).await;
        }
    }
}

#[embassy_executor::task]
async fn lcd_task(
    i2c: i2c::I2c<'static, I2C0, i2c::Blocking>,
    commands: &'static CharLcdNotifier,
) -> ! {
    let mut lcd = LcdDriver::new(i2c);
    lcd.init().await;

    loop {
        let msg = commands.receive().await;
        match msg {
            CharLcdMessage::Display { text, duration_ms } => {
                // Clear and display the text
                lcd.clear().await;

                // Split text by newline and display on separate lines
                let text_str = text.as_str();
                if let Some(newline_pos) = text_str.find('\n') {
                    // Two-line display
                    let (line1, rest) = text_str.split_at(newline_pos);
                    let line2 = &rest[1..]; // Skip the \n character

                    // Display line 1
                    lcd.print(line1).await;
                    // Move to line 2
                    lcd.set_cursor(1, 0).await;
                    // Display line 2
                    lcd.print(line2).await;
                } else {
                    // Single-line display
                    lcd.print(text_str).await;
                }

                // Wait for the minimum display duration
                if duration_ms > 0 {
                    Timer::after_millis(duration_ms.into()).await;
                }
            }
        }
    }
}
