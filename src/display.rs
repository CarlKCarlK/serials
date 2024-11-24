use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

use crate::{bit_matrix::BitMatrix, pins::OutputArray};

pub struct Display<const CELL_COUNT: usize>(&'static DisplayNotifier<CELL_COUNT>);
pub type DisplayNotifier<const CELL_COUNT: usize> =
    Signal<CriticalSectionRawMutex, BitMatrix<CELL_COUNT>>;

// Display #1 is a 4-digit 8s-segment display
pub const CELL_COUNT0: usize = 4;
pub const SEGMENT_COUNT0: usize = 8;
pub const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);

// cmk only CELL_COUNT0
impl Display<CELL_COUNT0> {
    pub fn new(
        digit_pins: OutputArray<CELL_COUNT0>,
        segment_pins: OutputArray<SEGMENT_COUNT0>,
        notifier: &'static DisplayNotifier<CELL_COUNT0>,
        spawner: Spawner,
    ) -> Self {
        let display = Self(notifier);
        unwrap!(spawner.spawn(device_loop(digit_pins, segment_pins, notifier)));
        display
    }

    pub const fn notifier() -> DisplayNotifier<CELL_COUNT0> {
        Signal::new()
    }
}

impl<const CELL_COUNT: usize> Display<CELL_COUNT> {
    pub fn write_chars(&self, chars: [char; CELL_COUNT]) {
        info!("write_chars: {:?}", chars);
        self.0.signal(BitMatrix::from_chars(&chars));
    }
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn device_loop(
    mut cell_pins: OutputArray<CELL_COUNT0>,
    mut segment_pins: OutputArray<SEGMENT_COUNT0>,
    notifier: &'static DisplayNotifier<CELL_COUNT0>,
) -> ! {
    let mut bit_matrix: BitMatrix<CELL_COUNT0> = BitMatrix::default();
    'outer: loop {
        info!("bit_matrix: {:?}", bit_matrix);
        let bits_to_indexes = bit_matrix.bits_to_indexes();
        info!("# of unique cell bit_matrix: {:?}", bits_to_indexes.len());

        match bits_to_indexes.iter().next() {
            // If the display should be empty, then just wait for the next notification
            None => bit_matrix = notifier.wait().await,
            // If only one bit pattern should be displayed (even on multiple cells), display it
            // and wait for the next notification
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low);
                bit_matrix = notifier.wait().await;
                cell_pins.set_levels_at_indexes(indexes, Level::High);
            }
            // If multiple patterns should be displayed, multiplex them until the next notification
            _ => loop {
                for (bytes, indexes) in &bits_to_indexes {
                    segment_pins.set_from_bits(*bytes);
                    cell_pins.set_levels_at_indexes(indexes, Level::Low);
                    let timeout_or_signal =
                        select(Timer::after(MULTIPLEX_SLEEP), notifier.wait()).await;
                    cell_pins.set_levels_at_indexes(indexes, Level::High);
                    if let Either::Second(notification) = timeout_or_signal {
                        bit_matrix = notification;
                        continue 'outer;
                    }
                }
            },
        }
    }
}
