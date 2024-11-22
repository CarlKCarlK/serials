#![no_std]
#![no_main]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
use adjustable_clock::AdjustableClock;
use button::Button;
use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use pins::Pins;
use state_machine::State;
use virtual_clock::{ClockMode, ClockNotifier, VirtualClock};
use virtual_display::{Notifier, VirtualDisplay, CELL_COUNT0};
use {defmt_rtt as _, panic_probe as _};

mod adjustable_clock;
mod bit_matrix;
mod button;
mod leds;
mod pins;
mod state_machine;
mod virtual_clock;
mod virtual_display;

// cmk put in Brad's err catcher in place of unwrap!

#[embassy_executor::main]
async fn main(#[allow(clippy::used_underscore_binding)] spawner0: Spawner) {
    let (pins, _core1) = Pins::new_and_core1();

    // cmk what would it look like to have another virtual display? Do we need CellCount0 here? should define a macro?
    static NOTIFIER0: Notifier<CELL_COUNT0> = VirtualDisplay::new_notifier();
    let virtual_display = VirtualDisplay::new(pins.cells0, pins.segments0, &NOTIFIER0, spawner0);
    info!("VirtualDisplay created");
    static CLOCK_NOTIFIER0: ClockNotifier = VirtualClock::new_notifier();
    let _virtual_clock = VirtualClock::new(virtual_display, &CLOCK_NOTIFIER0, spawner0);
    info!("VirtualClock created");

    // sleep forever
    loop {
        Timer::after(Duration::from_secs(5)).await;
        CLOCK_NOTIFIER0.signal(ClockMode::HhMm);
        Timer::after(Duration::from_secs(3)).await;
        CLOCK_NOTIFIER0.signal(ClockMode::MmSs);
    }
}

// async fn old_code_cmk() {
//     todo!();
//     let (pins, _core1) = Pins::new_and_core1();
//     static NOTIFIER0: Notifier<CELL_COUNT0> = VirtualDisplay::new_notifier();
//     let virtual_display = VirtualDisplay::new(pins.cells0, pins.segments0, &NOTIFIER0, spawner0);
//     let mut button = Button::new(pins.button);
//     let mut adjustable_clock = AdjustableClock::default();
//     let mut state = State::default();
//     loop {
//         defmt::info!("State: {:?}", state);
//         state = state
//             .next_state(&mut virtual_display, &mut button, &mut adjustable_clock)
//             .await;
//     }
// }
