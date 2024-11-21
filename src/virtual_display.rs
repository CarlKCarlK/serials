use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

use crate::{bit_matrix::BitMatrix, pins::OutputArray};

pub struct VirtualDisplay<const CELL_COUNT: usize>(&'static Notifier<CELL_COUNT>);

pub type Notifier<const CELL_COUNT: usize> = Signal<CriticalSectionRawMutex, BitMatrix<CELL_COUNT>>;

// Display #1 is a 4-digit 8s-segment display
pub const CELL_COUNT0: usize = 4;
pub const SEGMENT_COUNT0: usize = 8;
pub const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);

// cmk only CELL_COUNT0
impl VirtualDisplay<CELL_COUNT0> {
    pub fn new(
        digit_pins: OutputArray<CELL_COUNT0>,
        segment_pins: OutputArray<SEGMENT_COUNT0>,
        notifier: &'static Notifier<CELL_COUNT0>,
        spawner: Spawner,
    ) -> Self {
        let virtual_display = Self(notifier);
        unwrap!(spawner.spawn(virtual_display_task(digit_pins, segment_pins, notifier)));
        virtual_display
    }

    pub const fn new_notifier() -> Notifier<CELL_COUNT0> {
        Signal::new()
    }
}

impl<const CELL_COUNT: usize> VirtualDisplay<CELL_COUNT> {
    // cmk write_, print_, display_, ????
    pub fn write_text(&self, text: &str) {
        info!("write_text: {}", text);
        self.write_bit_matrix(BitMatrix::from_str(text));
    }
    // cmk make bit_matrix a type
    pub fn write_bit_matrix(&self, bit_matrix: BitMatrix<CELL_COUNT>) {
        info!("write_bit_matrix: {:?}", bit_matrix);
        self.0.signal(bit_matrix);
    }
    pub fn write_number(&self, number: u16, padding: u8) {
        info!("write_number: {}", number);
        self.write_bit_matrix(BitMatrix::from_number(number, padding));
    }
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn virtual_display_task(
    // cmk does this need 'static? What does it mean?
    mut cell_pins: OutputArray<CELL_COUNT0>,
    mut segment_pins: OutputArray<SEGMENT_COUNT0>,
    // cmk rename or re-type
    notifier: &'static Notifier<CELL_COUNT0>,
) -> ! {
    let mut bit_matrix: BitMatrix<CELL_COUNT0> = BitMatrix::default();
    'outer: loop {
        info!("bit_matrix: {:?}", bit_matrix);
        let bits_to_indexes = bit_matrix.bits_to_indexes();
        info!("# of unique cell bit_matrix: {:?}", bits_to_indexes.len());
        match bits_to_indexes.iter().next() {
            // If the display should be empty, then just wait for the next update
            None => bit_matrix = notifier.wait().await,

            // If only one bit pattern should be displayed (even on multiple cells), display it
            // and wait for the next update
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low);
                bit_matrix = notifier.wait().await; // cmk rename signal
                cell_pins.set_levels_at_indexes(indexes, Level::High);
            }
            // If multiple patterns should be displayed, multiplex them until the next update
            _ => loop {
                for (bytes, indexes) in &bits_to_indexes {
                    segment_pins.set_from_bits(*bytes);
                    cell_pins.set_levels_at_indexes(indexes, Level::Low);
                    let timeout_or_signal =
                        select(Timer::after(MULTIPLEX_SLEEP), notifier.wait()).await;
                    cell_pins.set_levels_at_indexes(indexes, Level::High);
                    if let Either::Second(new_bit_matrix) = timeout_or_signal {
                        bit_matrix = new_bit_matrix;
                        continue 'outer;
                    }
                }
            },
        }
    }
}
