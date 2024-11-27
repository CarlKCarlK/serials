use embassy_time::Duration;
use heapless::{LinearMap, Vec};

pub const BUTTON_DEBOUNCE_DELAY: Duration = Duration::from_millis(10);
pub const LONG_PRESS_DURATION: Duration = Duration::from_millis(500);
// Display #1 is a 4-digit 8s-segment display
pub const CELL_COUNT0: usize = 4;
pub const SEGMENT_COUNT0: usize = 8;
pub const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);
pub const ONE_MINUTE: Duration = Duration::from_secs(60);
pub const ONE_HOUR: Duration = Duration::from_secs(60 * 60);
pub const BLINK_OFF_DELAY: Duration = Duration::from_millis(50); // const cmk
pub const BLINK_ON_DELAY: Duration = Duration::from_millis(150); // const cmk

pub type BitsToIndexes = LinearMap<u8, Vec<usize, CELL_COUNT0>, CELL_COUNT0>;
