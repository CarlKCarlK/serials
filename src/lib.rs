//! Shared items for the clock project.
#![no_std]
#![no_main]

mod error;
mod hardware;
mod never;
mod output_array;
mod shared_constants;

// Re-export commonly used items
pub use error::{Error, Result};
pub use hardware::Hardware;
pub use never::Never;
pub use shared_constants::*;
