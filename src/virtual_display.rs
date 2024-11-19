use core::array;
use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use embedded_hal::digital::OutputPin; // cmk needed?
use heapless::{LinearMap, Vec};

use crate::leds::Leds;

pub struct VirtualDisplay<const DIGIT_COUNT: usize> {
    signal: &'static Signal<CriticalSectionRawMutex, [u8; DIGIT_COUNT]>,
}

// cmk only DIGIT_COUNT1
impl VirtualDisplay<CELL_COUNT1> {
    pub fn new(
        digit_pins: [gpio::Output<'static>; CELL_COUNT1],
        segment_pins: [gpio::Output<'static>; 8],
        spawner: Spawner,
        signal: &'static Signal<CriticalSectionRawMutex, [u8; CELL_COUNT1]>,
    ) -> Self {
        let virtual_display = Self { signal };
        unwrap!(spawner.spawn(monitor(digit_pins, segment_pins, signal)));
        virtual_display
    }
}

// Display #1 is a 4-digit 7-segment display
pub const CELL_COUNT1: usize = 4;

impl<const DIGIT_COUNT: usize> VirtualDisplay<DIGIT_COUNT> {
    pub fn write_text(&self, text: &str) {
        info!("write_text: {}", text);
        let bytes = Self::line_to_u8_array(text);
        self.write_bytes(&bytes);
    }
    pub fn write_bytes(&self, bytes_in: &[u8; DIGIT_COUNT]) {
        info!("write_bytes: {:?}", bytes_in);
        self.signal.signal(*bytes_in);
    }
    pub fn write_number(&self, mut number: u16, padding: u8) {
        info!("write_number: {}", number);
        let mut bytes = [padding; DIGIT_COUNT];

        for i in (0..DIGIT_COUNT).rev() {
            let digit = (number % 10) as usize; // Get the last digit
            bytes[i] = Leds::DIGITS[digit];
            number /= 10; // Remove the last digit
            if number == 0 {
                break;
            }
        }
        // If the original number was out of range, turn on all decimal points
        if number > 0 {
            for byte in &mut bytes {
                *byte |= Leds::DECIMAL;
            }
        }
        self.write_bytes(&bytes);
    }

    fn line_to_u8_array(line: &str) -> [u8; DIGIT_COUNT] {
        let mut result = [0; DIGIT_COUNT];
        (0..DIGIT_COUNT).zip(line.chars()).for_each(|(i, c)| {
            result[i] = Leds::ASCII_TABLE[c as usize];
        });
        if line.len() > DIGIT_COUNT {
            for byte in &mut result {
                *byte |= Leds::DECIMAL;
            }
        }
        result
    }
}

/// Turn a u8 into an iterator of bool
#[inline]
pub fn bool_iter(mut byte: u8) -> array::IntoIter<bool, 8> {
    // turn a u8 into an iterator of bool
    let mut bools_out = [false; 8];
    for bool_out in &mut bools_out {
        *bool_out = byte & 1 == 1;
        byte >>= 1;
    }
    bools_out.into_iter()
}

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn monitor(
    // cmk does this need 'static? What does it mean?
    mut cell_pins: [gpio::Output<'static>; CELL_COUNT1],
    mut segment_pins: [gpio::Output<'static>; 8],
    signal: &'static Signal<CriticalSectionRawMutex, [u8; CELL_COUNT1]>,
) -> ! {
    let mut cell_bits: [u8; CELL_COUNT1] = [0; CELL_COUNT1];
    loop {
        info!("cell_bits: {:?}", cell_bits);
        let bits_to_indexes = bits_to_indexes(&cell_bits);
        info!("# of unique cell bits: {:?}", bits_to_indexes.len());
        match bits_to_indexes.iter().next() {
            // If the display should be empty, then just wait for the next update
            None => cell_bits = signal.wait().await,

            // If only one bit pattern should be displayed (even on multiple cells), display it
            // and wait for the next update
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                // Set the segment pins with the bool iterator

                // cmk refactor
                bool_iter(bits)
                    .zip(segment_pins.iter_mut())
                    .for_each(|(state, segment_pin)| {
                        segment_pin.set_state(state.into()).unwrap();
                    });
                // activate the digits, wait for the next update, and deactivate the digits

                // cmk refactor
                for digit_index in indexes {
                    cell_pins[*digit_index].set_low(); // Assuming common cathode setup
                }
                cell_bits = signal.wait().await;
                for digit_index in indexes {
                    cell_pins[*digit_index].set_high();
                }
            }
            // If multiple patterns should be displayed, multiplex them until the next update
            _ => {
                'outer: loop {
                    for (bits, indexes) in &bits_to_indexes {
                        // Set the segment pins with the bool iterator
                        bool_iter(*bits).zip(segment_pins.iter_mut()).for_each(
                            |(state, segment_pin)| {
                                segment_pin.set_state(state.into()).unwrap();
                            },
                        );
                        // Activate, pause, and deactivate the digits
                        for digit_index in indexes {
                            cell_pins[*digit_index].set_low(); // Assuming common cathode setup
                        }
                        let sleep = 3; // cmk maybe this should depend on the # of digits

                        // Sleep (but wake up early if the display should be updated)
                        let what_happened =
                            select(Timer::after(Duration::from_millis(sleep)), signal.wait()).await;
                        for digit_index in indexes {
                            cell_pins[*digit_index].set_high();
                        }
                        if let Either::Second(new_digits) = what_happened {
                            cell_bits = new_digits;
                            break 'outer;
                        }
                    }
                }
            }
        }
    }
}

fn bits_to_indexes<const CELL_COUNT: usize>(
    cell_bits: &[u8; CELL_COUNT],
) -> LinearMap<u8, Vec<usize, CELL_COUNT>, CELL_COUNT> {
    cell_bits
        .iter()
        .filter(|&bits| *bits != 0) // Filter out zero bits
        .enumerate()
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
