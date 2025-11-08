use core::num::NonZeroU8;

use embassy_time::Duration;
use heapless::{LinearMap, Vec};

/// The number of cells (digits) in the display.
/// The display is a 4-digit, 8-segment display.
pub const CELL_COUNT_4LED_U8: u8 = 4;
pub const CELL_COUNT_4LED: usize = CELL_COUNT_4LED_U8 as usize;

/// The number of segments per digit in the display.
/// Each digit has 8 segments (7 segments plus a decimal point).
pub const SEGMENT_COUNT_4LED: usize = 8;

/// Duration representing one second.
pub const ONE_SECOND: Duration = Duration::from_secs(1);

/// Duration representing one minute (60 seconds).
pub const ONE_MINUTE: Duration = Duration::from_secs(60);

/// Duration representing one hour (60 minutes).
pub const ONE_HOUR: Duration = Duration::from_secs(60 * 60);

/// Duration representing one day (24 hours).
pub const ONE_DAY: Duration = Duration::from_secs(60 * 60 * 24);

/// Duration representing the number of ticks in one day.
pub const TICKS_IN_ONE_DAY: u64 = ONE_DAY.as_ticks();

/// Debounce delay for the button.
/// This prevents registering multiple presses from a single button press event.
pub const BUTTON_DEBOUNCE_DELAY: Duration = Duration::from_millis(10);

/// Duration representing a long button press.
/// Holding the button for this duration triggers additional functionality.
pub const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);

/// Sleep duration between multiplexing updates.
/// Determines how often the multiplexed display is refreshed.
pub const MULTIPLEX_SLEEP_4LED: Duration = Duration::from_millis(3);

/// Delay for the "off" state during blinking.
/// Controls how long the display or segment remains off during a blink cycle.
pub const BLINK_OFF_DELAY_4LED: Duration = Duration::from_millis(50);

/// Delay for the "on" state during blinking.
/// Controls how long the display or segment remains on during a blink cycle.
pub const BLINK_ON_DELAY_4LED: Duration = Duration::from_millis(150);

/// Speed for editing minutes in the clock.
/// Controls how quickly the minutes increment during manual adjustments.
pub const MINUTE_EDIT_SPEED_4LED: Duration = Duration::from_millis(250);

/// Speed for editing hours in the clock.
/// Controls how quickly the hours increment during manual adjustments.
pub const HOUR_EDIT_SPEED_4LED: Duration = Duration::from_millis(500);

/// A map from patterns to the indexes of the cells that contain that pattern.
///
/// For example, the numeral five has the LED bit pattern `0b_0110_1101`.
/// If the display is " 55 ", then the map will contain just one entry with the key `0b_0110_1101`
/// and the value `[1, 2]` representing the indexes of the cells that contain the numeral five.
///
/// The map is used to multiplex efficiently.
///
/// - The inner vector is statically allocated with a capacity of `CELL_COUNT`, the most possible.
/// The map itself can hold up to `CELL_COUNT_4LED` entries, the most possible.
pub type BitsToIndexes4Led = LinearMap<NonZeroU8, Vec<u8, CELL_COUNT_4LED>, CELL_COUNT_4LED>;
