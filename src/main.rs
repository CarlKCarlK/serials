#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

const HEAP_SIZE: usize = 1024 * 350; // in bytes
const TIME_LIMIT_MICROS: u64 = 1_000_000; // 1 second in microseconds
const LCD_ADDRESS: u8 = 0x27; // I2C address of PCF8574

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

use alloc_cortex_m::CortexMHeap;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::i2c::{self, Config as I2cConfig};
use embassy_time::Timer;
use lib::{Never, Result, ONE_DAY};
use malachite::num::arithmetic::traits::CeilingLogBase2;
use malachite::num::arithmetic::traits::SquareAssign;
use malachite::Natural;
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
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

    let start = embassy_time::Instant::now();
    let mut low = 0;
    let mut high = 1;

    // Exponential search to find an upper bound
    while start.elapsed().as_micros() < TIME_LIMIT_MICROS {
        let loop_start = embassy_time::Instant::now();
        let result = fibonacci(high);
        let elapsed = loop_start.elapsed();
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} s)",
            high,
            result.ceiling_log_base_2(),
            elapsed.as_micros() as f64 / 1_000_000.0
        );
        if elapsed.as_micros() >= TIME_LIMIT_MICROS {
            break;
        }
        high *= 2;
    }

    // Binary search to find the largest Fibonacci number that can be generated TIME_LIMIT
    while low < high {
        #[expect(clippy::integer_division_remainder_used, reason = "cmk")]
        let mid = (low + high) / 2;
        let mid_start = embassy_time::Instant::now();
        let result = fibonacci(mid);
        let elapsed = mid_start.elapsed();
        info!(
            "Fibonacci number at index {}: {} bits (computed in {} s)",
            mid,
            result.ceiling_log_base_2(),
            elapsed.as_micros() as f64 / 1_000_000.0
        );
        if elapsed.as_micros() < TIME_LIMIT_MICROS {
            low = mid + 1;
        } else {
            high = mid;
        }
    }

    info!(
        "Largest Fibonacci number index that can be generated in less than {} s: {}",
        TIME_LIMIT_MICROS as f64 / 1_000_000.0,
        (low - 1)
    );

    // sleep forever
    loop {
        Timer::after(ONE_DAY).await;
    }
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

