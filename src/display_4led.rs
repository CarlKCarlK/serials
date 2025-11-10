use core::convert::Infallible;

use crate::bit_matrix_4led::BitMatrix4Led;
use crate::Result;
use crate::led_4seg::Leds;
use crate::led_4seg::OutputArray;
use crate::constants::{BitsToIndexes4Led, CELL_COUNT_4LED, MULTIPLEX_SLEEP_4LED, SEGMENT_COUNT_4LED};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

use crate::blinker_4led::Text4Led;

/// Notifier type for the `Display4Led` device abstraction.
pub type Display4LedNotifier = Signal<CriticalSectionRawMutex, BitMatrix4Led>;

pub const LED_DECIMAL: u8 = 0b_1000_0000;

pub const LED_DIGITS: [u8; 10] = Leds::DIGITS;

pub const LED_ASCII_TABLE: [u8; 128] = Leds::ASCII_TABLE;

/// A device abstraction for a 4-digit 7-segment LED display.
pub struct Display4Led<'a>(&'a Display4LedNotifier);

impl Display4Led<'_> {
    #[must_use]
    pub const fn notifier() -> Display4LedNotifier {
        Signal::new()
    }

    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        cell_pins: OutputArray<'static, CELL_COUNT_4LED>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT_4LED>,
        notifier: &'static Display4LedNotifier,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = device_loop(cell_pins, segment_pins, notifier)?;
        spawner.spawn(token);
        Ok(Self(notifier))
    }

    pub fn write_text(&self, text: crate::blinker_4led::Text4Led) {
        #[cfg(feature = "display-trace")]
        info!("write_chars: {:?}", text);
        self.0.signal(BitMatrix4Led::from_text(&text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    cell_pins: OutputArray<'static, CELL_COUNT_4LED>,
    segment_pins: OutputArray<'static, SEGMENT_COUNT_4LED>,
    notifier: &'static Display4LedNotifier,
) -> ! {
    let err = inner_device_loop(cell_pins, segment_pins, notifier)
        .await
        .unwrap_err();
    panic!("{err}");
}

async fn inner_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT_4LED>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT_4LED>,
    notifier: &'static Display4LedNotifier,
) -> Result<Infallible> {
    let mut bit_matrix = BitMatrix4Led::default();
    let mut bits_to_indexes = BitsToIndexes4Led::default();
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
                        select(Timer::after(MULTIPLEX_SLEEP_4LED), notifier.wait()).await;
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
