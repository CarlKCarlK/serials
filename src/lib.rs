//! Shared items for the clock project.
#![no_std]
#![no_main]

mod bit_matrix;
mod blinker;
mod button;
mod clock;
mod clock_state;
mod clock_time;
mod display;
mod error;
mod hardware;
mod leds;
mod never;
mod output_array;
mod shared_constants;

// Re-export commonly used items
pub use blinker::{BlinkMode, Blinker, BlinkerNotifier};
pub use button::Button;
pub use clock::{Clock, ClockNotifier, NotifierInner};
pub use clock_state::ClockState;
pub use clock_time::ClockTime;
pub use display::{Display, DisplayNotifier};
pub use error::{Error, Result};
pub use hardware::Hardware;
pub use leds::Leds;
pub use never::Never;
pub use shared_constants::*;
