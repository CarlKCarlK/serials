//! LED 4-segment 7-segment display virtual device
//!
//! This module provides a virtual device abstraction for a 4-digit, 7-segment LED display
//! with support for blinking, multiplexing, and text rendering.

use core::num::NonZeroU8;
use defmt::{info, unwrap};
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::{LinearMap, Vec};

use crate::BitMatrix4Led;
use crate::BlinkState4Led;
use crate::OutputArray;
use crate::Result;

// ============================================================================
// Constants
// ============================================================================

/// The number of cells (digits) in the display.
pub const CELL_COUNT_U8: u8 = 4;
pub const CELL_COUNT: usize = CELL_COUNT_U8 as usize;

/// The number of segments per digit in the display.
pub const SEGMENT_COUNT: usize = 8;

/// Sleep duration between multiplexing updates.
pub const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);

/// Delay for the "off" state during blinking.
pub const BLINK_OFF_DELAY: Duration = Duration::from_millis(50);

/// Delay for the "on" state during blinking.
pub const BLINK_ON_DELAY: Duration = Duration::from_millis(150);

/// Type alias for text to display (4 characters).
pub type Text = [char; CELL_COUNT];

/// A map from patterns to the indexes of the cells that contain that pattern.
pub type BitsToIndexes = LinearMap<NonZeroU8, Vec<u8, CELL_COUNT>, CELL_COUNT>;

// ============================================================================
// LED Constants
// ============================================================================

/// Constants for 7-segment LED displays.
pub struct Leds;

impl Leds {
    /// Segment A of the 7-segment display.
    pub const SEG_A: u8 = 0b_0000_0001;
    /// Segment B of the 7-segment display.
    pub const SEG_B: u8 = 0b_0000_0010;
    /// Segment C of the 7-segment display.
    pub const SEG_C: u8 = 0b_0000_0100;
    /// Segment D of the 7-segment display.
    pub const SEG_D: u8 = 0b_0000_1000;
    /// Segment E of the 7-segment display.
    pub const SEG_E: u8 = 0b_0001_0000;
    /// Segment F of the 7-segment display.
    pub const SEG_F: u8 = 0b_0010_0000;
    /// Segment G of the 7-segment display.
    pub const SEG_G: u8 = 0b_0100_0000;
    /// Decimal point of the 7-segment display.
    pub const DECIMAL: u8 = 0b_1000_0000;

    /// Array representing the segments for digits 0-9 on a 7-segment display.
    pub const DIGITS: [u8; 10] = [
        0b_0011_1111, // Digit 0
        0b_0000_0110, // Digit 1
        0b_0101_1011, // Digit 2
        0b_0100_1111, // Digit 3
        0b_0110_0110, // Digit 4
        0b_0110_1101, // Digit 5
        0b_0111_1101, // Digit 6
        0b_0000_0111, // Digit 7
        0b_0111_1111, // Digit 8
        0b_0110_1111, // Digit 9
    ];

    /// Representation of a blank space on a 7-segment display.
    pub const SPACE: u8 = 0b_0000_0000;

    /// ASCII table mapping characters to their 7-segment display representations.
    pub const ASCII_TABLE: [u8; 128] = [
        // Control characters (0-31) + space (32)
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        0b_0000_0000,
        // Symbols (33-47)
        0b_1000_0110,              // !
        Self::SEG_A | Self::SEG_B, // "
        0b_0000_0000,              // #
        0b_0000_0000,              // $
        0b_0000_0000,              // %
        0b_0000_0000,              // &
        Self::SEG_A,               // '
        Self::SEG_A | Self::SEG_F, // (
        Self::SEG_C | Self::SEG_D, // )
        Self::SEG_D | Self::SEG_E, // *
        0b_0000_0000,              // +
        0b_0000_0000,              // ,
        0b_0100_0000,              // -
        0b_1000_0000,              // .
        0b_0000_0000,              // /
        // Numbers (48-57)
        0b_0011_1111, // 0
        0b_0000_0110, // 1
        0b_0101_1011, // 2
        0b_0100_1111, // 3
        0b_0110_0110, // 4
        0b_0110_1101, // 5
        0b_0111_1101, // 6
        0b_0000_0111, // 7
        0b_0111_1111, // 8
        0b_0110_1111, // 9
        // Symbols (58-64)
        0b_0000_0000,              // :
        0b_0000_0000,              // ;
        Self::SEG_E | Self::SEG_F, // <
        0b_0000_0000,              // =
        Self::SEG_B | Self::SEG_C, // >
        0b_0000_0000,              // ?
        0b_0000_0000,              // @
        // Uppercase letters (65-90)
        0b_0111_0111, // A
        0b_0111_1100, // B
        0b_0011_1001, // C
        0b_0101_1110, // D
        0b_0111_1001, // E
        0b_0111_0001, // F
        0b_0011_1101, // G
        0b_0111_0110, // H
        0b_0000_0110, // I
        0b_0001_1110, // J
        0b_0111_0110, // K
        0b_0011_1000, // L
        0b_0001_0101, // M
        0b_0101_0100, // N
        0b_0011_1111, // O
        0b_0111_0011, // P
        0b_0110_0111, // Q
        0b_0101_0000, // R
        0b_0110_1101, // S
        0b_0111_1000, // T
        0b_0011_1110, // U
        0b_0010_1010, // V
        0b_0001_1101, // W
        0b_0111_0110, // X
        0b_0110_1110, // Y
        0b_0101_1011, // Z
        // Symbols (91-96)
        0b_0011_1001, // [
        0b_0000_0000, // \
        0b_0000_1111, // ]
        0b_0000_0000, // ^
        0b_0000_1000, // _
        0b_0000_0000, // `
        // Lowercase letters (97-122)
        0b_0111_0111, // a
        0b_0111_1100, // b
        0b_0011_1001, // c
        0b_0101_1110, // d
        0b_0111_1001, // e
        0b_0111_0001, // f
        0b_0011_1101, // g
        0b_0111_0100, // h
        0b_0000_0110, // i
        0b_0001_1110, // j
        0b_0111_0110, // k
        0b_0011_1000, // l
        0b_0001_0101, // m
        0b_0101_0100, // n
        0b_0011_1111, // o
        0b_0111_0011, // p
        0b_0110_0111, // q
        0b_0101_0000, // r
        0b_0110_1101, // s
        0b_0111_1000, // t
        0b_0011_1110, // u
        0b_0010_1010, // v
        0b_0001_1101, // w
        0b_0111_0110, // x
        0b_0110_1110, // y
        0b_0101_1011, // z
        // Symbols (123-127)
        0b_0011_1001, // {
        0b_0000_0110, // |
        0b_0000_1111, // }
        0b_0100_0000, // ~
        0b_0000_0000, // delete
    ];
}

// ============================================================================
// Led4Seg Virtual Device
// ============================================================================

/// A 4-digit, 7-segment LED display with blinking support.
pub struct Led4Seg<'a>(&'a Led4SegNotifier);

/// Notifier for sending messages to the Led4Seg device.
pub type Led4SegNotifier = Signal<CriticalSectionRawMutex, (BlinkState4Led, Text)>;

impl Led4Seg<'_> {
    /// Creates a new `Led4SegNotifier`.
    #[must_use]
    pub const fn notifier() -> Led4SegNotifier {
        Signal::new()
    }

    /// Creates a new `Led4Seg` device.
    ///
    /// # Arguments
    ///
    /// * `cell_pins` - The pins that control the cells (digits) of the display.
    /// * `segment_pins` - The pins that control the segments of the display.
    /// * `notifier` - The static notifier that sends messages to the device.
    /// * `spawner` - The Embassy task spawner.
    ///
    /// # Errors
    ///
    /// Returns a `SpawnError` if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static Led4SegNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = unwrap!(led_4seg_device_loop(cell_pins, segment_pins, notifier));
        spawner.spawn(token);
        Ok(Self(notifier))
    }

    /// Writes text to the display with optional blinking.
    pub fn write_text(&self, blink_state: BlinkState4Led, text: Text) {
        info!("Led4Seg: blink_state={:?}, text={:?}", blink_state, text);
        self.0.signal((blink_state, text));
    }
}

#[embassy_executor::task]
async fn led_4seg_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static Led4SegNotifier,
) -> ! {
    // Wait for first command before starting
    let (mut blink_state, mut text) = notifier.wait().await;
    let mut bits_to_indexes = BitsToIndexes::default();

    loop {
        // Handle blink state transitions
        let bit_matrix = match blink_state {
            BlinkState4Led::Solid => {
                let bit_matrix = BitMatrix4Led::from_text(&text);
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
            BlinkState4Led::BlinkingAndOn => BitMatrix4Led::from_text(&text),
            BlinkState4Led::BlinkingButOff => BitMatrix4Led::default(), // All blank
        };

        // Handle blinking with timeout
        if let Some(new_state) = display_with_timeout(
            &mut cell_pins,
            &mut segment_pins,
            &bit_matrix,
            &mut bits_to_indexes,
            notifier,
            if matches!(blink_state, BlinkState4Led::BlinkingAndOn) {
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
                BlinkState4Led::BlinkingAndOn => BlinkState4Led::BlinkingButOff,
                BlinkState4Led::BlinkingButOff => BlinkState4Led::BlinkingAndOn,
                BlinkState4Led::Solid => BlinkState4Led::Solid, // unreachable
            };
        }
    }
}

/// Display the bit matrix until a notification is received.
async fn display_until_notification(
    cell_pins: &mut OutputArray<'static, CELL_COUNT>,
    segment_pins: &mut OutputArray<'static, SEGMENT_COUNT>,
    bit_matrix: &BitMatrix4Led,
    bits_to_indexes: &mut BitsToIndexes,
    notifier: &'static Led4SegNotifier,
) {
    let _ = bit_matrix.bits_to_indexes(bits_to_indexes);

    match bits_to_indexes.iter().next() {
        None => {
            // Display is empty, just wait
            let _: (BlinkState4Led, Text) = notifier.wait().await;
        }
        Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
            // Only one pattern, no multiplexing needed
            segment_pins.set_from_nonzero_bits(bits);
            let _ = cell_pins.set_levels_at_indexes(indexes, Level::Low);
            let _: (BlinkState4Led, Text) = notifier.wait().await;
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
    bit_matrix: &BitMatrix4Led,
    bits_to_indexes: &mut BitsToIndexes,
    notifier: &'static Led4SegNotifier,
    timeout: Duration,
) -> Option<(BlinkState4Led, Text)> {
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
