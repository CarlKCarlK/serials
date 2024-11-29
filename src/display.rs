use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

use crate::{
    bit_matrix::BitMatrix,
    error, never,
    output_array::OutputArray,
    shared_constants::{CELL_COUNT, MULTIPLEX_SLEEP, SEGMENT_COUNT},
};
use error::Result;
use never::Never;

/// A struct representing the display.
pub struct Display<'a>(&'a DisplayNotifier);
/// A type alias for the notifier that sends messages to the `Display`.
pub type DisplayNotifier = Signal<CriticalSectionRawMutex, BitMatrix>;

impl Display<'_> {
    /// Create a new `Display`, which entails starting an Embassy task.
    ///
    /// # Arguments
    ///
    /// * `cell_pins` - The pins that control the cells (digits) of the display.
    /// * `segment_pins` - The pins that control the segments of the display.
    /// * `notifier` - The static notifier that sends messages to the `Display`.
    ///          This notifier is created with the `Display::notifier()` method.
    /// * `spawner` - The spawner that will spawn the task that controls the display.
    ///
    /// # Errors
    ///
    /// Returns a `SpawnError` if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static DisplayNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let display = Self(notifier);
        spawner.spawn(device_loop(cell_pins, segment_pins, notifier))?;
        Ok(display)
    }

    #[must_use]
    /// Creates a new `DisplayNotifier`.
    ///
    /// This notifier is used to send messages to the `Display`.
    ///
    /// This should be assigned to a static variable and passed to the `Display::new()` method.
    pub const fn notifier() -> DisplayNotifier {
        Signal::new()
    }
    /// Writes characters to the display.
    ///
    /// The characters can be be any Unicode character but
    /// unknown or hard to display characters will be displayed as blanks.
    pub fn write_chars(&self, chars: [char; CELL_COUNT]) {
        info!("write_chars: {:?}", chars);
        self.0.signal(BitMatrix::from_chars(&chars));
    }
}

#[embassy_executor::task]
async fn device_loop(
    cell_pins: OutputArray<'static, CELL_COUNT>,
    segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static DisplayNotifier,
) -> ! {
    // should never return
    let err = inner_device_loop(cell_pins, segment_pins, notifier).await;
    panic!("{:?}", err);
}

async fn inner_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static DisplayNotifier,
) -> Result<Never> {
    let mut bit_matrix: BitMatrix = BitMatrix::default();
    'outer: loop {
        info!("bit_matrix: {:?}", bit_matrix);
        let bits_to_indexes = bit_matrix.bits_to_indexes()?;
        info!("# of unique cell bit_matrix: {:?}", bits_to_indexes.len());

        match bits_to_indexes.iter().next() {
            // If the display should be empty, then just wait for the next notification
            None => bit_matrix = notifier.wait().await,
            // If only one bit pattern should be displayed (even on multiple cells), display it
            // and wait for the next notification
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low)?;
                bit_matrix = notifier.wait().await;
                cell_pins.set_levels_at_indexes(indexes, Level::High)?;
            }
            // If multiple patterns should be displayed, multiplex them until the next notification
            _ => loop {
                for (bytes, indexes) in &bits_to_indexes {
                    segment_pins.set_from_bits(*bytes);
                    cell_pins.set_levels_at_indexes(indexes, Level::Low)?;
                    let timeout_or_signal =
                        select(Timer::after(MULTIPLEX_SLEEP), notifier.wait()).await;
                    cell_pins.set_levels_at_indexes(indexes, Level::High)?;
                    if let Either::Second(notification) = timeout_or_signal {
                        bit_matrix = notification;
                        continue 'outer;
                    }
                }
            },
        }
    }
}
