//! Device abstraction for the SunFounder Kepler Kit IR remote control.
//!
//! See [`IrKepler`] for usage examples.

use embassy_executor::Spawner;
use embassy_rp::Peri;
use embassy_rp::gpio::Pin;

use crate::ir::IrNotifier;
use crate::ir_mapping::IrMapping;
use crate::Result;

/// Button types for the SunFounder Kepler Kit remote control.
#[derive(defmt::Format, Clone, Copy, PartialEq, Eq)]
pub enum KeplerButton {
    /// Power button (top left)
    Power,
    /// Mode button (top center)
    Mode,
    /// Mute button (top right)
    Mute,
    /// Play/Pause button (row 2, left)
    PlayPause,
    /// Previous track button (row 2, center)
    Prev,
    /// Next track button (row 2, right)
    Next,
    /// Equalizer button (row 3, left)
    Eq,
    /// Minus/Volume down button (row 3, center)
    Minus,
    /// Plus/Volume up button (row 3, right)
    Plus,
    /// Number 0 (row 4, left)
    Num0,
    /// Repeat button (row 4, center)
    Repeat,
    /// U/SD button (row 4, right)
    USd,
    /// Number 1 (row 5, left)
    Num1,
    /// Number 2 (row 5, center)
    Num2,
    /// Number 3 (row 5, right)
    Num3,
    /// Number 4 (row 6, left)
    Num4,
    /// Number 5 (row 6, center)
    Num5,
    /// Number 6 (row 6, right)
    Num6,
    /// Number 7 (row 7, left)
    Num7,
    /// Number 8 (row 7, center)
    Num8,
    /// Number 9 (row 7, right)
    Num9,
}

/// Button mapping for the SunFounder Kepler Kit remote (ordered to match physical layout).
pub const KEPLER_MAPPING: [(u16, u8, KeplerButton); 21] = [
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
    (0x0000, 0x16, KeplerButton::Num0),
    (0x0000, 0x19, KeplerButton::Repeat),
    (0x0000, 0x0D, KeplerButton::USd),
    // Row 5: 1, 2, 3
    (0x0000, 0x0C, KeplerButton::Num1),
    (0x0000, 0x18, KeplerButton::Num2),
    (0x0000, 0x5E, KeplerButton::Num3),
    // Row 6: 4, 5, 6
    (0x0000, 0x08, KeplerButton::Num4),
    (0x0000, 0x1C, KeplerButton::Num5),
    (0x0000, 0x5A, KeplerButton::Num6),
    // Row 7: 7, 8, 9
    (0x0000, 0x42, KeplerButton::Num7),
    (0x0000, 0x52, KeplerButton::Num8),
    (0x0000, 0x4A, KeplerButton::Num9),
];

/// Device abstraction for the SunFounder Kepler Kit IR remote.
///
/// This provides a simple interface for the Kepler remote with built-in button mappings.
///
/// # Examples
/// ```no_run
/// # use embassy_executor::Spawner;
/// # use serials::ir_kepler::IrKepler;
/// # async fn example(p: embassy_rp::Peripherals, spawner: Spawner) -> serials::Result<()> {
/// static NOTIFIER: serials::ir::IrNotifier = IrKepler::notifier();
///
/// let remote = IrKepler::new(p.PIN_15, &NOTIFIER, spawner)?;
///
/// loop {
///     let button = remote.wait().await;
///     info!("Button: {:?}", button);
/// }
/// # }
/// ```
pub struct IrKepler<'a> {
    mapping: IrMapping<'a, KeplerButton, 21>,
}

impl<'a> IrKepler<'a> {
    /// Create a new notifier channel for IR events.
    ///
    /// See [`IrKepler`] for usage examples.
    #[must_use]
    pub const fn notifier() -> IrNotifier {
        IrMapping::<KeplerButton, 21>::notifier()
    }

    /// Create a new Kepler remote handler.
    ///
    /// # Parameters
    /// - `pin`: GPIO pin connected to the IR receiver
    /// - `notifier`: Static reference to the notifier channel
    /// - `spawner`: Embassy spawner for background task
    ///
    /// See [`IrKepler`] for usage examples.
    ///
    /// # Errors
    /// Returns an error if the background task cannot be spawned.
    pub fn new<P: Pin>(
        pin: Peri<'static, P>,
        notifier: &'static IrNotifier,
        spawner: Spawner,
    ) -> Result<Self> {
        let mapping = IrMapping::new(pin, &KEPLER_MAPPING, notifier, spawner)?;
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
