use core::convert::Infallible;

use crate::bit_matrix_led4::BitMatrixLed4;
use crate::Result;
use crate::led4::OutputArray;
use crate::led4::{BitsToIndexes, CELL_COUNT, MULTIPLEX_SLEEP, SEGMENT_COUNT};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

/// Notifier type for the `DisplayLed4` device abstraction.
pub type DisplayLed4Notifier = Signal<CriticalSectionRawMutex, BitMatrixLed4>;

/// A device abstraction for a 4-digit 7-segment LED display.
pub struct DisplayLed4<'a>(&'a DisplayLed4Notifier);

impl DisplayLed4<'_> {
    #[must_use]
    pub const fn notifier() -> DisplayLed4Notifier {
        Signal::new()
    }

    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        notifier: &'static DisplayLed4Notifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = device_loop(cell_pins, segment_pins, notifier)?;
        spawner.spawn(token);
        Ok(Self(notifier))
    }

    pub fn write_text(&self, text: crate::blinker_led4::TextLed4) {
        #[cfg(feature = "display-trace")]
        info!("write_chars: {:?}", text);
        self.0.signal(BitMatrixLed4::from_text(&text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    cell_pins: OutputArray<'static, CELL_COUNT>,
    segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static DisplayLed4Notifier,
) -> ! {
    let err = inner_device_loop(cell_pins, segment_pins, notifier)
        .await
        .unwrap_err();
    panic!("{err}");
}

async fn inner_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    notifier: &'static DisplayLed4Notifier,
) -> Result<Infallible> {
    let mut bit_matrix = BitMatrixLed4::default();
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
