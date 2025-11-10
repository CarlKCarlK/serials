//! A device abstraction for a 4-digit, 7-segment LED display with blinking support.
//!
//! This module provides hardware abstractions for controlling common-cathode
//! 4-digit 7-segment LED displays. Supports displaying text and numbers with
//! optional blinking.
//!
//! See [`Led4`] for the main device abstraction and usage examples.

use core::num::NonZeroU8;
use embassy_executor::Spawner;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Duration;
use heapless::{LinearMap, Vec};

use crate::Result;
use crate::led4_simple::{Led4Simple, Led4SimpleNotifier};

#[cfg(feature = "display-trace")]
use defmt::info;

// ============================================================================
// OutputArray Submodule
// ============================================================================

mod output_array;
pub use output_array::OutputArray;

// ============================================================================
// Constants
// ============================================================================

/// The number of cells (digits) in the display.
pub(crate) const CELL_COUNT_U8: u8 = 4;
pub(crate) const CELL_COUNT: usize = CELL_COUNT_U8 as usize;

/// The number of segments per digit in the display.
pub(crate) const SEGMENT_COUNT: usize = 8;

/// Sleep duration between multiplexing updates.
pub(crate) const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);

/// Delay for the "off" state during blinking.
const BLINK_OFF_DELAY: Duration = Duration::from_millis(50);

/// Delay for the "on" state during blinking.
const BLINK_ON_DELAY: Duration = Duration::from_millis(150);

/// Internal type for optimizing multiplexing by grouping digits with identical segment patterns.
///
/// Maps from segment bit patterns to the indexes of digits that share that pattern.
/// This reduces the number of multiplex iterations needed when multiple digits
/// display the same character.
pub(crate) type BitsToIndexes = LinearMap<NonZeroU8, Vec<u8, CELL_COUNT>, CELL_COUNT>;

// ============================================================================
// BlinkState Enum
// ============================================================================

/// Blinking behavior for 4-digit LED displays.
///
/// Used with [`Led4::write_text()`] to control whether the display blinks.
/// See the [`Led4`] documentation for usage examples.
#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

// ============================================================================
// Led4 Virtual Device
// ============================================================================

/// A device abstraction for a 4-digit, 7-segment LED display with blinking support.
///
/// # Hardware Requirements
///
/// This abstraction is designed for common-cathode 7-segment displays where:
/// - Cell pins control which digit is active (LOW = on, HIGH = off)
/// - Segment pins control which segments light up (HIGH = on, LOW = off)
///
/// # Example
///
/// ```no_run
/// #![no_std]
/// #![no_main]
///
/// use embassy_rp::gpio::{Level, Output};
/// use serials::{Error, led4::{BlinkState, Led4, Led4Notifier, OutputArray}};
/// # use embassy_executor::Spawner;
/// # use core::panic::PanicInfo;
/// # #[panic_handler]
/// # fn panic(_: &PanicInfo) -> ! { loop {} }
///
/// async fn example(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<(), Error> {
///     // Set up cell pins (control which digit is active)
///     let cells = OutputArray::new([
///         Output::new(p.PIN_1, Level::High),
///         Output::new(p.PIN_2, Level::High),
///         Output::new(p.PIN_3, Level::High),
///         Output::new(p.PIN_4, Level::High),
///     ]);
///
///     // Set up segment pins (control which segments light up)
///     let segments = OutputArray::new([
///         Output::new(p.PIN_5, Level::Low),  // Segment A
///         Output::new(p.PIN_6, Level::Low),  // Segment B
///         Output::new(p.PIN_7, Level::Low),  // Segment C
///         Output::new(p.PIN_8, Level::Low),  // Segment D
///         Output::new(p.PIN_9, Level::Low),  // Segment E
///         Output::new(p.PIN_10, Level::Low), // Segment F
///         Output::new(p.PIN_11, Level::Low), // Segment G
///         Output::new(p.PIN_12, Level::Low), // Decimal point
///     ]);
///
///     // Create the display
///     static NOTIFIER: Led4Notifier = Led4::notifier();
///     let display = Led4::new(cells, segments, &NOTIFIER, spawner)?;
///
///     // Display "1234" (solid)
///     display.write_text(BlinkState::Solid, ['1', '2', '3', '4']);
///     
///     // Display "rUSt" blinking
///     display.write_text(BlinkState::BlinkingAndOn, ['r', 'U', 'S', 't']);
///     
///     Ok(())
/// }
/// ```
pub struct Led4<'a>(&'a Led4OuterNotifier);

/// Notifier for the [`Led4`] device.
pub type Led4Notifier = (Led4OuterNotifier, Led4SimpleNotifier);

/// Signal for sending blink state and text to the [`Led4`] device.
pub(crate) type Led4OuterNotifier = Signal<CriticalSectionRawMutex, (BlinkState, [char; CELL_COUNT])>;

impl Led4<'_> {
    /// Creates the display device and spawns its background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static Led4Notifier,
        spawner: Spawner,
    ) -> Result<Self> {
        let (outer_notifier, display_notifier) = notifier;
        let display = Led4Simple::new(cell_pins, segment_pins, display_notifier, spawner)?;
        let token = device_loop(outer_notifier, display)?;
        spawner.spawn(token);
        Ok(Self(outer_notifier))
    }

    /// Creates a notifier for the display.
    #[must_use]
    pub const fn notifier() -> Led4Notifier {
        (Signal::new(), Led4Simple::notifier())
    }

    /// Sends text to the display with optional blinking.
    pub fn write_text(&self, blink_state: BlinkState, text: [char; CELL_COUNT]) {
        #[cfg(feature = "display-trace")]
        info!("blink_state: {:?}, text: {:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    outer_notifier: &'static Led4OuterNotifier,
    display: Led4Simple<'static>,
) -> ! {
    let mut blink_state = BlinkState::default();
    let mut text = [' '; CELL_COUNT];
    #[expect(clippy::shadow_unrelated, reason = "False positive; not shadowing")]
    loop {
        (blink_state, text) = blink_state.execute(outer_notifier, &display, text).await;
    }
}

impl BlinkState {
    pub async fn execute(
        self,
        outer_notifier: &'static Led4OuterNotifier,
        display: &Led4Simple<'_>,
        text: [char; CELL_COUNT],
    ) -> (Self, [char; CELL_COUNT]) {
        use embassy_futures::select::{Either, select};
        use embassy_time::Timer;

        match self {
            Self::Solid => {
                display.write_text(text);
                outer_notifier.wait().await
            }
            Self::BlinkingAndOn => {
                display.write_text(text);
                if let Either::First((new_state, new_text)) =
                    select(outer_notifier.wait(), Timer::after(BLINK_ON_DELAY)).await
                {
                    (new_state, new_text)
                } else {
                    (Self::BlinkingButOff, text)
                }
            }
            Self::BlinkingButOff => {
                display.write_text([' '; CELL_COUNT]);
                if let Either::First((new_state, new_text)) =
                    select(outer_notifier.wait(), Timer::after(BLINK_OFF_DELAY)).await
                {
                    (new_state, new_text)
                } else {
                    (Self::BlinkingAndOn, text)
                }
            }
        }
    }
}
