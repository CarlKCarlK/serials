use core::convert::Infallible;

use derive_more::derive::{Display, Error};
use esp_hal_mfrc522::consts::PCDErrorCode;

/// A specialized `Result` where the error is this crate's `Error` type.
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// Define a unified error type for this crate.
#[expect(missing_docs, reason = "The variants are self-explanatory.")]
#[derive(Debug, Display, Error)]
pub enum Error {
    // `#[error(not(source))]` below tells `derive_more` that `embassy_executor::SpawnError` does
    // not implement Rust's `core::error::Error` trait.  `SpawnError` should, but Rust's `Error`
    // only recently moved from `std` (which is not available in bare-metal development) to `core`
    // (which is). Perhaps a future update of `embassy_executor::SpawnError` will implement
    // `core::error::Error` which will make this unnecessary.
    #[cfg(any(feature = "pico1", feature = "pico2"))]
    #[display("{_0:?}")]
    TaskSpawn(#[error(not(source))] embassy_executor::SpawnError),

    #[display("bits_to_indexes does not have enough preallocated space")]
    BitsToIndexesNotEnoughSpace,

    #[display("BitsToIndexes is full")]
    BitsToIndexesFull,

    #[display("Error setting output state")]
    CannotSetOutputState,

    #[display("Index out of bounds")]
    IndexOutOfBounds,

    #[display("MFRC522 initialization failed: {_0:?}")]
    Mfrc522Init(#[error(not(source))] PCDErrorCode),

    #[display("MFRC522 version read failed: {_0:?}")]
    Mfrc522Version(#[error(not(source))] PCDErrorCode),

    #[display("Format error")]
    FormatError,

    #[cfg(all(feature = "wifi", any(feature = "pico1", feature = "pico2")))]
    #[display("Flash operation failed: {_0:?}")]
    Flash(#[error(not(source))] embassy_rp::flash::Error),

    #[cfg(feature = "wifi")]
    #[display("WiFi credential storage is invalid")]
    CredentialStorageCorrupted,
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        Self::CannotSetOutputState
    }
}

impl From<()> for Error {
    fn from(_: ()) -> Self {
        Self::FormatError
    }
}

#[cfg(any(feature = "pico1", feature = "pico2"))]
impl From<embassy_executor::SpawnError> for Error {
    fn from(err: embassy_executor::SpawnError) -> Self {
        Self::TaskSpawn(err)
    }
}
