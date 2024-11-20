use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::{LinearMap, Vec};

use crate::{leds::Leds, pins::OutputArray};

// cmk #[derive(Debug, defmt::Format)]
#[derive(defmt::Format)]
pub struct BitMatrix<const CELL_COUNT: usize>([u8; CELL_COUNT]);

impl<const CELL_COUNT: usize> BitMatrix<CELL_COUNT> {
    pub fn new(bits: [u8; CELL_COUNT]) -> Self {
        Self(bits)
    }
    pub fn from_bits(bits: u8) -> Self {
        Self([bits; CELL_COUNT])
    }

    pub fn iter(&self) -> impl Iterator<Item = &u8> {
        self.0.iter()
    }
}

// default
impl<const CELL_COUNT: usize> Default for BitMatrix<CELL_COUNT> {
    fn default() -> Self {
        Self([0; CELL_COUNT])
    }
}

// implement into_iter for BitMatrix and &BitMatrix
impl<const CELL_COUNT: usize> IntoIterator for BitMatrix<CELL_COUNT> {
    type Item = u8;
    type IntoIter = core::array::IntoIter<u8, CELL_COUNT>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<'a, const CELL_COUNT: usize> IntoIterator for &'a BitMatrix<CELL_COUNT> {
    type Item = &'a u8;
    type IntoIter = core::slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

// and &mut BitMatrix
impl<'a, const CELL_COUNT: usize> IntoIterator for &'a mut BitMatrix<CELL_COUNT> {
    type Item = &'a mut u8;
    type IntoIter = core::slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

// implement index for BitMatrix and &BitMatrix
impl<const CELL_COUNT: usize> core::ops::Index<usize> for BitMatrix<CELL_COUNT> {
    type Output = u8;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

// index that you can assign to
impl<const CELL_COUNT: usize> core::ops::IndexMut<usize> for BitMatrix<CELL_COUNT> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

pub struct VirtualDisplay<const CELL_COUNT: usize> {
    signal: &'static Signal<CriticalSectionRawMutex, BitMatrix<CELL_COUNT>>,
}

// cmk only CELL_COUNT1
impl VirtualDisplay<CELL_COUNT1> {
    pub fn new(
        digit_pins: OutputArray<CELL_COUNT1>,
        segment_pins: OutputArray<SEGMENT_COUNT1>,
        spawner: Spawner,
        signal: &'static Signal<CriticalSectionRawMutex, BitMatrix<CELL_COUNT1>>,
    ) -> Self {
        let virtual_display = Self { signal };
        unwrap!(spawner.spawn(monitor(digit_pins, segment_pins, signal)));
        virtual_display
    }
}

// Display #1 is a 4-digit 8s-segment display
pub const CELL_COUNT1: usize = 4;
pub const SEGMENT_COUNT1: usize = 8;
pub const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);

impl<const CELL_COUNT: usize> VirtualDisplay<CELL_COUNT> {
    pub fn write_text(&self, text: &str) {
        info!("write_text: {}", text);
        let bit_matrix = Self::text_to_bit_matrix(text);
        self.write_bit_matrix(bit_matrix);
    }
    // cmk make bit_matrix a type
    pub fn write_bit_matrix(&self, bit_matrix: BitMatrix<CELL_COUNT>) {
        info!("write_bit_matrix: {:?}", bit_matrix);
        self.signal.signal(bit_matrix);
    }
    pub fn write_number(&self, mut number: u16, padding: u8) {
        info!("write_number: {}", number);
        let mut bit_matrix = BitMatrix::from_bits(padding);

        for i in (0..CELL_COUNT).rev() {
            let digit = (number % 10) as usize; // Get the last digit
            bit_matrix[i] = Leds::DIGITS[digit];
            number /= 10; // Remove the last digit
            if number == 0 {
                break;
            }
        }
        // If the original number was out of range, turn on all decimal points
        if number > 0 {
            for bits in &mut bit_matrix {
                *bits |= Leds::DECIMAL;
            }
        }
        self.write_bit_matrix(bit_matrix);
    }

    // If too long, turn on all decimal points
    fn text_to_bit_matrix(text: &str) -> BitMatrix<CELL_COUNT> {
        let mut result = BitMatrix::default();
        (0..CELL_COUNT).zip(text.chars()).for_each(|(i, c)| {
            result[i] = Leds::ASCII_TABLE[c as usize];
        });
        if text.len() > CELL_COUNT {
            for byte in &mut result {
                *byte |= Leds::DECIMAL;
            }
        }
        result
    }
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn monitor(
    // cmk does this need 'static? What does it mean?
    mut cell_pins: OutputArray<CELL_COUNT1>,
    mut segment_pins: OutputArray<SEGMENT_COUNT1>,
    signal: &'static Signal<CriticalSectionRawMutex, BitMatrix<CELL_COUNT1>>,
) -> ! {
    let mut bit_matrix: BitMatrix<CELL_COUNT1> = BitMatrix::default();
    'outer: loop {
        info!("bit_matrix: {:?}", bit_matrix);
        let bits_to_indexes = bit_matrix_to_indexes(&bit_matrix);
        info!("# of unique cell bit_matrix: {:?}", bits_to_indexes.len());
        match bits_to_indexes.iter().next() {
            // If the display should be empty, then just wait for the next update
            None => bit_matrix = signal.wait().await,

            // If only one bit pattern should be displayed (even on multiple cells), display it
            // and wait for the next update
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low);
                bit_matrix = signal.wait().await; // cmk rename signal
                cell_pins.set_levels_at_indexes(indexes, Level::High);
            }
            // If multiple patterns should be displayed, multiplex them until the next update
            _ => loop {
                for (bytes, indexes) in &bits_to_indexes {
                    segment_pins.set_from_bits(*bytes);
                    cell_pins.set_levels_at_indexes(indexes, Level::Low);
                    let timeout_or_signal =
                        select(Timer::after(MULTIPLEX_SLEEP), signal.wait()).await;
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

fn bit_matrix_to_indexes<const CELL_COUNT: usize>(
    bit_matrix: &BitMatrix<CELL_COUNT>,
) -> LinearMap<u8, Vec<usize, CELL_COUNT>, CELL_COUNT> {
    bit_matrix
        .iter()
        .enumerate()
        .filter(|(_, &bits)| bits != 0) // Filter out zero bits
        .fold(
            LinearMap::new(),
            |mut acc: LinearMap<u8, Vec<usize, CELL_COUNT>, CELL_COUNT>, (index, &bits)| {
                if let Some(vec) = acc.get_mut(&bits) {
                    vec.push(index).unwrap();
                } else {
                    let vec = Vec::from_slice(&[index]).unwrap();
                    acc.insert(bits, vec).unwrap();
                }
                acc
            },
        )
}
