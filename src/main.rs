#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use defmt::unwrap;
use embassy_executor::{Executor, Spawner};
use embassy_rp::multicore::{spawn_core1, Stack};
use embassy_time::{Duration, Instant};
use leds::Leds;
use pins::Pins;
use state_machine::{state_to_state, State};
use static_cell::StaticCell;
use virtual_led::monitor_display1;
use {defmt_rtt as _, panic_probe as _};

mod leds;
mod pins;
mod state_machine;
mod virtual_led;

// cmk put in Brad's err catcher

static mut CORE1_STACK: Stack<4096> = Stack::new();
static EXECUTOR1: StaticCell<Executor> = StaticCell::new();

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] _spawner0: Spawner) {
    let (pins, core1) = Pins::new_and_core1();

    // Spawn 'multiplex_display1' on core1
    spawn_core1(
        core1,
        unsafe { &mut *core::ptr::addr_of_mut!(CORE1_STACK) },
        move || {
            let executor1 = EXECUTOR1.init(Executor::new());
            executor1.run(|spawner1| {
                unwrap!(spawner1.spawn(monitor_display1(pins.digits1, pins.segments1)));
            });
        },
    );

    let mut state = State::First;
    let button = pins.button;
    let start = Instant::now();
    let mut offset = Duration::default();
    loop {
        defmt::info!("State: {:?}", state);
        (state, offset) = state_to_state(state, button, start, offset).await;
    }
}
