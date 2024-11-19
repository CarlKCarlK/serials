#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Instant};
use pins::Pins;
use state_machine::{state_to_state, State};
use virtual_display::{VirtualDisplay, DIGIT_COUNT1};
use {defmt_rtt as _, panic_probe as _};

mod leds;
mod pins;
mod state_machine;
mod virtual_display;

// cmk put in Brad's err catcher in place of unwrap!

pub static SIGNAL: Signal<CriticalSectionRawMutex, [u8; DIGIT_COUNT1]> = Signal::new();

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) {
    let (pins, _core1) = Pins::new_and_core1();

    let mut virtual_display = VirtualDisplay::new(pins.digits1, pins.segments1, spawner0, &SIGNAL);

    let mut state = State::First;
    let mut button = pins.button;
    let start = Instant::now();
    let mut offset = Duration::default();
    loop {
        defmt::info!("State: {:?}", state);
        (state, offset) =
            state_to_state(state, &mut virtual_display, &mut button, start, offset).await;
    }
}
