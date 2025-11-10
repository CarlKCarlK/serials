//! A device abstraction for 4-digit 7-segment LED displays.
//!
//! This module provides hardware abstractions for controlling common-cathode
//! 4-digit 7-segment LED displays. Supports displaying text and numbers with
//! optional blinking.
//!
//! See [`Led4`] for the main device abstraction and usage examples.

use core::num::NonZeroU8;
use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::{LinearMap, Vec};

mod output_array;
pub use output_array::OutputArray;

use crate::bit_matrix_led4::BitMatrixLed4;
pub use crate::blinker_led4::BlinkState;
use crate::Result;

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
// Led4 Virtual Device
// ============================================================================

// ============================================================================
// Led4 Virtual Device
// ============================================================================

/// A device abstraction for a 4-digit, 7-segment LED display.
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
pub struct Led4<'a>(&'a Led4Notifier);

/// Notifier for sending display updates to the [`Led4`] background task.
///
/// See the [`Led4`] example for usage.
pub type Led4Notifier = Signal<CriticalSectionRawMutex, (BlinkState, [char; 4])>;

impl Led4<'_> {
    /// Creates a notifier for the display.
    ///
    /// See the [`Led4`] example for usage.
    #[must_use]
    pub const fn notifier() -> Led4Notifier {
        Signal::new()
    }

    /// Creates the display device and spawns its background task.
    ///
    /// See the [`Led4`] example for usage.
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
        let token = unwrap!(led4_device_loop(cell_pins, segment_pins, notifier));
        spawner.spawn(token);
        Ok(Self(notifier))
    }

    /// Sends text to the display with optional blinking.
    ///
    /// See the [`Led4`] example for usage.
    pub fn write_text(&self, blink_state: BlinkState, text: [char; 4]) {
        info!("Led4: blink_state={:?}, text={:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

#[embassy_executor::task]
async fn led4_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static Led4Notifier,
) -> ! {
    // Wait for first command before starting
    let (mut blink_state, mut text) = notifier.wait().await;
    let mut bits_to_indexes = BitsToIndexes::default();

    loop {
        // Handle blink state transitions
        let bit_matrix = match blink_state {
            BlinkState::Solid => {
                let bit_matrix = BitMatrixLed4::from_text(&text);
                display_until_notification(
                    &mut cell_pins,
                    &mut segment_pins,
                    &bit_matrix,
                    &mut bits_to_indexes,
                    notifier,
                )
                .await;
                (blink_state, text) = notifier.wait().await;
                continue;
            }
            BlinkState::BlinkingAndOn => BitMatrixLed4::from_text(&text),
            BlinkState::BlinkingButOff => BitMatrixLed4::default(), // All blank
        };

        // Handle blinking with timeout
        if let Some(new_state) = display_with_timeout(
            &mut cell_pins,
            &mut segment_pins,
            &bit_matrix,
            &mut bits_to_indexes,
            notifier,
            if matches!(blink_state, BlinkState::BlinkingAndOn) {
                BLINK_ON_DELAY
            } else {
                BLINK_OFF_DELAY
            },
        )
        .await
        {
            (blink_state, text) = new_state;
        } else {
            blink_state = match blink_state {
                BlinkState::BlinkingAndOn => BlinkState::BlinkingButOff,
                BlinkState::BlinkingButOff => BlinkState::BlinkingAndOn,
                BlinkState::Solid => BlinkState::Solid, // unreachable
            };
        }
    }
}

/// Display the bit matrix until a notification is received.
async fn display_until_notification(
    cell_pins: &mut OutputArray<'static, CELL_COUNT>,
    segment_pins: &mut OutputArray<'static, SEGMENT_COUNT>,
    bit_matrix: &BitMatrixLed4,
    bits_to_indexes: &mut BitsToIndexes,
    notifier: &'static Led4Notifier,
) {
    let _ = bit_matrix.bits_to_indexes(bits_to_indexes);

    match bits_to_indexes.iter().next() {
        None => {
            // Display is empty, just wait
            let _: (BlinkState, [char; 4]) = notifier.wait().await;
        }
        Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
            // Only one pattern, no multiplexing needed
            segment_pins.set_from_nonzero_bits(bits);
            let _ = cell_pins.set_levels_at_indexes(indexes, Level::Low);
            let _: (BlinkState, [char; 4]) = notifier.wait().await;
            let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
        }
        _ => {
            // Multiple patterns, multiplex
            'multiplex: loop {
                for (bits, indexes) in bits_to_indexes.iter() {
                    segment_pins.set_from_nonzero_bits(*bits);
                    let _ = cell_pins.set_levels_at_indexes(indexes, Level::Low);
                    if let Either::First(_) =
                        select(notifier.wait(), Timer::after(MULTIPLEX_SLEEP)).await
                    {
                        let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
                        break 'multiplex;
                    }
                    let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
                }
            }
        }
    }
}

/// Display the bit matrix with a timeout, returning Some if interrupted.
async fn display_with_timeout(
    cell_pins: &mut OutputArray<'static, CELL_COUNT>,
    segment_pins: &mut OutputArray<'static, SEGMENT_COUNT>,
    bit_matrix: &BitMatrixLed4,
    bits_to_indexes: &mut BitsToIndexes,
    notifier: &'static Led4Notifier,
    timeout: Duration,
) -> Option<(BlinkState, [char; 4])> {
    let _ = bit_matrix.bits_to_indexes(bits_to_indexes);

    match bits_to_indexes.iter().next() {
        None => {
            // Display is empty, just wait for timeout
            if let Either::First(new_state) = select(notifier.wait(), Timer::after(timeout)).await {
                Some(new_state)
            } else {
                None
            }
        }
        Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
            // Only one pattern
            segment_pins.set_from_nonzero_bits(bits);
            let _ = cell_pins.set_levels_at_indexes(indexes, Level::Low);
            let result = if let Either::First(new_state) =
                select(notifier.wait(), Timer::after(timeout)).await
            {
                Some(new_state)
            } else {
                None
            };
            let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
            result
        }
        _ => {
            // Multiple patterns, multiplex with timeout
            let start = embassy_time::Instant::now();

            loop {
                for (bits, indexes) in bits_to_indexes.iter() {
                    segment_pins.set_from_nonzero_bits(*bits);
                    let _ = cell_pins.set_levels_at_indexes(indexes, Level::Low);

                    // Check if timeout expired
                    if embassy_time::Instant::now().duration_since(start) >= timeout {
                        let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
                        return None;
                    }

                    match select(notifier.wait(), Timer::after(MULTIPLEX_SLEEP)).await {
                        Either::First(new_state) => {
                            let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
                            return Some(new_state);
                        }
                        Either::Second(()) => {
                            let _ = cell_pins.set_levels_at_indexes(indexes, Level::High);
                            // Continue multiplexing
                        }
                    }
                }
            }
        }
    }
}
