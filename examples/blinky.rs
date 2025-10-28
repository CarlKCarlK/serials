//! Minimal async blink example for Raspberry Pi Pico 2.
//! Emits SOS in Morse code on the onboard LED using Embassy timers.
#![no_std]
#![no_main]

use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Level, Output};
use embassy_time::Timer;
use panic_probe as _;
use defmt::info;


const DOT_MS: u64 = 200;
const DASH_MS: u64 = DOT_MS * 3;
const SYMBOL_GAP_MS: u64 = DOT_MS;
const LETTER_GAP_MS: u64 = DOT_MS * 3;
const WORD_GAP_MS: u64 = DOT_MS * 7;

const SOS_PATTERN: &[(u64, u64)] = &[
    // S: dot dot dot
    (DOT_MS, SYMBOL_GAP_MS),
    (DOT_MS, SYMBOL_GAP_MS),
    (DOT_MS, LETTER_GAP_MS),
    // O: dash dash dash
    (DASH_MS, SYMBOL_GAP_MS),
    (DASH_MS, SYMBOL_GAP_MS),
    (DASH_MS, LETTER_GAP_MS),
    // S: dot dot dot
    (DOT_MS, SYMBOL_GAP_MS),
    (DOT_MS, SYMBOL_GAP_MS),
    (DOT_MS, WORD_GAP_MS),
];

#[embassy_executor::main]
pub async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    let mut led = Output::new(p.PIN_25, Level::Low);

    loop {
        info!("Emitting SOS in Morse code");
        for &(on_ms, off_ms) in SOS_PATTERN {
            led.set_high();
            Timer::after_millis(on_ms).await;
            led.set_low();
            Timer::after_millis(off_ms).await;
        }
    }
}
