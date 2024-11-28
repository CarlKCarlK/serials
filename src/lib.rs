//! Shared items for the clock project.
#![warn(
    clippy::pedantic,
    clippy::nursery,
     clippy::use_self,
     unused_lifetimes,
    missing_docs,
     single_use_lifetimes,
     unreachable_pub,
    // cmk clippy::cargo,
    clippy::perf,
    clippy::style,
    clippy::complexity,
    clippy::correctness,
    clippy::must_use_candidate,
    // // // cmk0 clippy::cargo_common_metadata
    clippy::unwrap_used, clippy::unwrap_used, // Warns if you're using .unwrap() or .expect(), which can be a sign of inadequate error handling.
    clippy::panic_in_result_fn, // Ensures functions that return Result do not contain panic!, which could be inappropriate in production code.
)]
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
pub use blinker::{Blinker, BlinkerNotifier};
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
