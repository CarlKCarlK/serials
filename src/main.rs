#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use defmt::unwrap;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant};
use leds::Leds;
use pins::Pins;
use state_machine::{state_to_state, State};
use virtual_led::monitor_display1;
use {defmt_rtt as _, panic_probe as _};

mod leds;
mod pins;
mod state_machine;
mod virtual_led;

// cmk put in Brad's err catcher in place of unwrap!

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) {
    let (pins, _core1) = Pins::new_and_core1();

    unwrap!(spawner0.spawn(monitor_display1(pins.digits1, pins.segments1)));

    let mut state = State::First;
    let mut button = pins.button;
    let start = Instant::now();
    let mut offset = Duration::default();
    loop {
        defmt::info!("State: {:?}", state);
        (state, offset) = state_to_state(state, &mut button, start, offset).await;
    }
}
