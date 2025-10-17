#![no_std]
#![no_main]

//! IR Pin Diagnostic - Simple Edge Detection Test
//!
//! Verifies that GPIO6 can detect edges from the IR sensor.
//! If you see "LOW", "HIGH" messages alternating, the pin is working.
//! If nothing happens, check:
//! - IR sensor wiring to GPIO6
//! - IR sensor power (should blink LED when receiving)
//! - Remote battery (pull plastic tab to activate)
//!
//! Run with: `cargo run --example ir_test_pin`

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("IR Pin Diagnostic - GPIO6 Edge Detection Test");

    let mut ir_pin = Input::new(p.PIN_6, Pull::Up);

    info!("Pin initialized. Waiting for edges...");
    info!("Press remote buttons now!");

    for _ in 0..100 {
        // Log current state
        let state = ir_pin.is_low();
        info!("Pin state: {}", if state { "LOW" } else { "HIGH" });

        // Wait for any edge
        ir_pin.wait_for_falling_edge().await;
        info!(">>> FALLING edge detected!");
        Timer::after_millis(10).await;

        ir_pin.wait_for_rising_edge().await;
        info!("<<< RISING edge detected!");
        Timer::after_millis(10).await;
    }

    info!("Test complete (100 edges captured)");

    loop {
        Timer::after_secs(1).await;
    }
}
