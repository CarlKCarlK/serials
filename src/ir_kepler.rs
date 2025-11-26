//! A device abstraction for the SunFounder Kepler Kit IR remote control.
//!
//! See [`IrKepler`] for usage examples.

use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::gpio::Pin;

use crate::Result;
use crate::ir_mapping::{IrMapping, IrMappingStatic};

/// Button types for the SunFounder Kepler Kit remote control.
#[derive(defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum KeplerButton {
    Power,
    Mode,
    Mute,
    PlayPause,
    Prev,
    Next,
    Eq,
    Minus,
    Plus,
    Num(u8),
    Repeat,
    USd,
}

/// Static resources for Kepler IR remote events.
///
/// See [`IrKepler`] for usage examples.
pub struct IrKeplerStatic(IrMappingStatic);

impl IrKeplerStatic {
    /// Create static resources for the Kepler remote.
    #[must_use]
    pub const fn new() -> Self {
        Self(IrMappingStatic::new())
    }

    pub(crate) const fn inner(&self) -> &IrMappingStatic {
        &self.0
    }
}

/// Type alias for the Kepler button mapping.
///
/// See [`IrKepler`] for usage examples.
type IrKeplerMapping<'a> = IrMapping<'a, KeplerButton, 21>;

/// Button mapping for the SunFounder Kepler Kit remote (ordered to match physical layout).
const KEPLER_MAPPING: [(u16, u8, KeplerButton); 21] = [
    // Row 1: Power, Mode, Mute
    (0x0000, 0x45, KeplerButton::Power),
    (0x0000, 0x46, KeplerButton::Mode),
    (0x0000, 0x47, KeplerButton::Mute),
    // Row 2: PlayPause, Prev, Next
    (0x0000, 0x44, KeplerButton::PlayPause),
    (0x0000, 0x40, KeplerButton::Prev),
    (0x0000, 0x43, KeplerButton::Next),
    // Row 3: EQ, Minus, Plus
    (0x0000, 0x07, KeplerButton::Eq),
    (0x0000, 0x15, KeplerButton::Minus),
    (0x0000, 0x09, KeplerButton::Plus),
    // Row 4: 0, Repeat, U/SD
    (0x0000, 0x16, KeplerButton::Num(0)),
    (0x0000, 0x19, KeplerButton::Repeat),
    (0x0000, 0x0D, KeplerButton::USd),
    // Row 5: 1, 2, 3
    (0x0000, 0x0C, KeplerButton::Num(1)),
    (0x0000, 0x18, KeplerButton::Num(2)),
    (0x0000, 0x5E, KeplerButton::Num(3)),
    // Row 6: 4, 5, 6
    (0x0000, 0x08, KeplerButton::Num(4)),
    (0x0000, 0x1C, KeplerButton::Num(5)),
    (0x0000, 0x5A, KeplerButton::Num(6)),
    // Row 7: 7, 8, 9
    (0x0000, 0x42, KeplerButton::Num(7)),
    (0x0000, 0x52, KeplerButton::Num(8)),
    (0x0000, 0x4A, KeplerButton::Num(9)),
];

/// A device abstraction for the SunFounder Kepler Kit IR remote.
///
/// This provides a simple interface for the Kepler remote with built-in button mappings.
///
/// # Examples
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use serials::ir_kepler::{IrKepler, IrKeplerStatic};
///
/// async fn example(
///     p: embassy_rp::Peripherals,
///     spawner: embassy_executor::Spawner,
/// ) -> serials::Result<()> {
///     static IR_KEPLER_STATIC: IrKeplerStatic = IrKepler::new_static();
///     let ir_kepler = IrKepler::new(&IR_KEPLER_STATIC, p.PIN_15, spawner)?;
///
///     loop {
///         let button = ir_kepler.wait().await;
///         defmt::info!("Button: {:?}", button);
///     }
/// }
/// ```
pub struct IrKepler<'a> {
    mapping: IrKeplerMapping<'a>,
}

impl<'a> IrKepler<'a> {
    /// Create static channel resources for IR events.
    ///
    /// See [`IrKepler`] for usage examples.
    #[must_use]
    pub const fn new_static() -> IrKeplerStatic {
        IrKeplerStatic::new()
    }

    /// Create a new Kepler remote handler.
    ///
    /// # Parameters
    /// - `ir_kepler_static`: Static reference to the channel resources
    /// - `pin`: GPIO pin connected to the IR receiver
    /// - `spawner`: Embassy spawner for background task
    ///
    /// See [`IrKepler`] for usage examples.
    ///
    /// # Errors
    /// Returns an error if the background task cannot be spawned.
    pub fn new<P: Pin>(
        ir_kepler_static: &'static IrKeplerStatic,
        pin: Peri<'static, P>,
        spawner: Spawner,
    ) -> Result<Self> {
        let mapping = IrMapping::new(ir_kepler_static.inner(), pin, &KEPLER_MAPPING, spawner)?;
        Ok(Self { mapping })
    }

    /// Wait for the next button press.
    ///
    /// Ignores button presses that are not recognized by the Kepler remote.
    ///
    /// See [`IrKepler`] for usage examples.
    pub async fn wait(&self) -> KeplerButton {
        self.mapping.wait().await
    }
}
