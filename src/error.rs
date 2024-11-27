use core::convert::Infallible;

use derive_more::derive::{Display, Error, From};

pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Define a unified error type for this crate.
#[derive(Debug, Display, Error, From)]
pub enum Error {
    // `#[error(not(source))]` below tells `derive_more` that `embassy_executor::SpawnError` does
    // not implement Rust's `core::error::Error` trait.  `SpawnError` should, but Rust's `Error`
    // only recently moved from `std` (which is not available in bare-metal development) to `core`
    // (which is). Perhaps a future update of `embassy_executor::SpawnError` will implement
    // `core::error::Error` which will make this unnecessary.
    #[display("{_0:?}")]
    TaskSpawn(#[error(not(source))] embassy_executor::SpawnError),

    #[display("bits_to_indexes does not have enough preallocated space")]
    BitsToIndexesNotEnoughSpace,

    #[display("Error setting output state")]
    CannotSetOutputState,
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        Self::CannotSetOutputState
    }
}
