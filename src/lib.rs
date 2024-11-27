//! Share the types and modules defined below across the crate.
#![no_std]
#![no_main]

mod bit_matrix;
mod blinker;
mod button;
mod clock;
mod display;
mod error;
mod leds;
mod never;
mod offset_time;
mod pins;
mod shared_constants;
mod state_machine;

// Re-export commonly used items
pub use button::Button;
pub use clock::{Clock, ClockNotifier};
pub use error::{Error, Result};
pub use never::Never;
pub use pins::Pins;
pub use shared_constants::*;
pub use state_machine::State;
