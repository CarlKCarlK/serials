//! LCD Display driver for HD44780-compatible displays with PCF8574 I2C backpack

use embassy_rp::i2c::{self, Config as I2cConfig, Instance as I2cInstance};
use embassy_rp::Peripheral;
use embassy_time::Timer;

/// Character LCD Display with I2C interface (HD44780 + PCF8574 backpack)
pub struct CharLcdI2c<'d, T: I2cInstance> {
    i2c: i2c::I2c<'d, T, i2c::Blocking>,
    address: u8,
}

// PCF8574 pin mapping: P0=RS, P1=RW, P2=E, P3=Backlight, P4-P7=Data
const LCD_BACKLIGHT: u8 = 0x08;
const LCD_ENABLE: u8 = 0x04;
const LCD_RS: u8 = 0x01;

impl<'d, T: I2cInstance> CharLcdI2c<'d, T> {
    /// Create a new LCD instance with default I2C address (0x27) and initialize it
    /// 
    /// This uses the most common PCF8574 I2C address. If your display doesn't work,
    /// try `new_with_address()` with 0x3F instead.
    /// 
    /// # Arguments
    /// * `i2c_peripheral` - I2C peripheral (I2C0 or I2C1)
    /// * `scl` - Clock pin (any valid I2C SCL pin for this peripheral)
    /// * `sda` - Data pin (any valid I2C SDA pin for this peripheral)
    pub async fn new<SCL: i2c::SclPin<T>, SDA: i2c::SdaPin<T>>(
        i2c_peripheral: impl Peripheral<P = T> + 'd,
        scl: impl Peripheral<P = SCL> + 'd,
        sda: impl Peripheral<P = SDA> + 'd,
    ) -> Self {
        Self::new_with_address(i2c_peripheral, scl, sda, 0x27).await
    }
    
    /// Create a new LCD instance with custom I2C address and initialize it
    /// 
    /// # Arguments
    /// * `i2c_peripheral` - I2C peripheral (I2C0 or I2C1)
    /// * `scl` - Clock pin (any valid I2C SCL pin for this peripheral)
    /// * `sda` - Data pin (any valid I2C SDA pin for this peripheral)
    /// * `i2c_address` - I2C address of PCF8574 backpack (typically 0x27 or 0x3F)
    pub async fn new_with_address<SCL: i2c::SclPin<T>, SDA: i2c::SdaPin<T>>(
        i2c_peripheral: impl Peripheral<P = T> + 'd,
        scl: impl Peripheral<P = SCL> + 'd,
        sda: impl Peripheral<P = SDA> + 'd,
        i2c_address: u8,
    ) -> Self {
        let mut lcd = Self {
            i2c: i2c::I2c::new_blocking(i2c_peripheral, scl, sda, I2cConfig::default()),
            address: i2c_address,
        };
        
        lcd.init().await;
        lcd
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
    
    async fn write_byte(&mut self, byte: u8, rs: bool) {
        self.write_nibble((byte >> 4) & 0x0F, rs).await;
        self.write_nibble(byte & 0x0F, rs).await;
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
        self.write_byte(0x28, false).await;
        // Display control: display on, cursor off, blink off
        self.write_byte(0x0C, false).await;
        // Clear display
        self.write_byte(0x01, false).await;
        Timer::after_millis(2).await;
        // Entry mode: increment cursor, no shift
        self.write_byte(0x06, false).await;
    }
    
    /// Clear the display
    pub async fn clear(&mut self) {
        self.write_byte(0x01, false).await;
        Timer::after_millis(2).await;
    }
    
    /// Print text to the display at the current cursor position
    pub async fn print(&mut self, text: &str) {
        for ch in text.bytes() {
            self.write_byte(ch, true).await;
        }
    }
    
    /// Send a command byte to the display
    pub async fn write_command(&mut self, cmd: u8) {
        self.write_byte(cmd, false).await;
    }
    
    /// Write a single byte (character or data) to the display
    pub async fn write_byte_public(&mut self, byte: u8, rs: bool) {
        self.write_byte(byte, rs).await;
    }
}
