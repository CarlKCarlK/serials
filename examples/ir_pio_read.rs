#![no_std]
#![no_main]

//! IR Receiver Reading Experiment using PIO
//! 
//! Goal: Read raw IR signals using PIO (Programmable I/O) on RP2040
//! to understand IR protocol and potentially replace GPIO polling.
//! 
//! This is a side project/experiment - run with:
//! ```
//! cargo run --example ir_pio_read
//! ```

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(_spawner: Spawner) -> ! {
    let p = embassy_rp::init(Default::default());

    info!("IR PIO Read Example - Starting");

    // For now, use simple GPIO polling as a baseline
    // TODO: Replace with PIO-based approach
    let mut ir_pin = Input::new(p.PIN_6, Pull::Up);

    info!("IR pin initialized on GPIO 6");
    info!("Waiting for IR signals...");

    loop {
        // Wait for falling edge (IR sensor goes low when receiving)
        ir_pin.wait_for_low().await;
        info!("IR LOW detected");

        // Wait for rising edge
        ir_pin.wait_for_high().await;
        info!("IR HIGH detected");

        Timer::after_millis(100).await;
    }
}
