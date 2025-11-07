use core::convert::Infallible;

use crate::BitMatrix;
use crate::Result;
use crate::cwf::Leds;
use crate::cwf::output_array::OutputArray;
use crate::cwf::shared_constants::{BitsToIndexes, CELL_COUNT, MULTIPLEX_SLEEP, SEGMENT_COUNT};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

pub struct Display<'a>(&'a DisplayNotifier);

pub type DisplayNotifier = Signal<CriticalSectionRawMutex, BitMatrix>;

pub const LED_DECIMAL: u8 = 0b_1000_0000;

pub const LED_DIGITS: [u8; 10] = Leds::DIGITS;

pub const LED_ASCII_TABLE: [u8; 128] = Leds::ASCII_TABLE;

impl Display<'_> {
    #[must_use]
    pub const fn notifier() -> DisplayNotifier {
        Signal::new()
    }

    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static DisplayNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = device_loop(cell_pins, segment_pins, notifier)?;
        spawner.spawn(token);
        Ok(Self(notifier))
    }

    pub fn write_text(&self, text: crate::cwf::blinker::Text) {
        #[cfg(feature = "display-trace")]
        info!("write_chars: {:?}", text);
        self.0.signal(BitMatrix::from_text(&text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    cell_pins: OutputArray<'static, CELL_COUNT>,
    segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static DisplayNotifier,
) -> ! {
    let err = inner_device_loop(cell_pins, segment_pins, notifier)
        .await
        .unwrap_err();
    panic!("{err}");
}

async fn inner_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static DisplayNotifier,
) -> Result<Infallible> {
    let mut bit_matrix = BitMatrix::default();
    let mut bits_to_indexes = BitsToIndexes::default();
    'outer: loop {
        #[cfg(feature = "display-trace")]
        info!("bit_matrix: {:?}", bit_matrix);
        bit_matrix.bits_to_indexes(&mut bits_to_indexes)?;
        #[cfg(feature = "display-trace")]
        info!("# of unique cell bit_matrix: {:?}", bits_to_indexes.len());

        match bits_to_indexes.iter().next() {
            None => bit_matrix = notifier.wait().await,
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_nonzero_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low)?;
                bit_matrix = notifier.wait().await;
                cell_pins.set_levels_at_indexes(indexes, Level::High)?;
            }
            _ => loop {
                for (bits, indexes) in &bits_to_indexes {
                    segment_pins.set_from_nonzero_bits(*bits);
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
