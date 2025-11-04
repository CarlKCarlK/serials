//! Info World - Simple defmt logging test
//!
//! Logs "Hello World 0", "Hello World 1", etc. once per second to verify
//! that defmt logging works on Pico 2 W (without WiFi).
//!
//! Run with: cargo info_world

#![no_std]
#![no_main]

use defmt::*;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) {
    let _p = embassy_rp::init(Default::default());
    
    info!("Info World Example Started!");
    
    let mut counter: u32 = 0;
    loop {
        info!("Hello World {}", counter);
        counter = counter.wrapping_add(1);
        Timer::after_secs(1).await;
    }
}
