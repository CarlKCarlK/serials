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

// cmk why not have the channel send the bytes directly?
pub struct VirtualDisplay<const DIGIT_COUNT: usize> {
    signal: &'static Signal<CriticalSectionRawMutex, [u8; DIGIT_COUNT]>,
}

// cmk only DIGIT_COUNT1
impl VirtualDisplay<DIGIT_COUNT1> {
    pub fn new(
        digit_pins: [gpio::Output<'static>; DIGIT_COUNT1],
        segment_pins: [gpio::Output<'static>; 8],
        spawner: Spawner,
        signal: &'static Signal<CriticalSectionRawMutex, [u8; DIGIT_COUNT1]>,
    ) -> Self {
        let virtual_display = Self { signal };
        unwrap!(spawner.spawn(monitor(digit_pins, segment_pins, signal)));
        virtual_display
    }
}

// Display #1 is a 4-digit 7-segment display
pub const DIGIT_COUNT1: usize = 4;

// pub static VIRTUAL_DISPLAY1: VirtualDisplay<DIGIT_COUNT1> = VirtualDisplay {
//     signal: Signal::new(),
//     digits: [0; DIGIT_COUNT1],
// };

// #[task]
// pub async fn monitor_display1(
//     mut digit_pins: [gpio::Output<'static>; DIGIT_COUNT1],
//     mut segment_pins: [gpio::Output<'static>; 8],
//     signal: &Signal<CriticalSectionRawMutex, [u8; DIGIT_COUNT1]>,
// ) {
//     monitor(&mut VIRTUAL_DISPLAY1, &mut digit_pins, &mut segment_pins).await;
// }
// cmk would be nice to have a separate way to turn on decimal points
// cmk would be nice to have a way to pass in 4 chars
impl<const DIGIT_COUNT: usize> VirtualDisplay<DIGIT_COUNT> {
    pub fn write_text(&self, text: &str) {
        info!("write_text: {}", text);
        let bytes = line_to_u8_array(text);
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

    // cmk remove
    // /// Turn a u8 into an iterator of bool
    // pub async fn bool_iter(&self, digit_index: usize) -> array::IntoIter<bool, 8> {
    //     // inner scope to release the lock
    //     let byte: u8;
    //     {
    //         let digit_array = self.mutex_digits.lock().await;
    //         byte = digit_array[digit_index];
    //     }
    //     bool_iter(byte)
    // }
}

#[inline]
/// Turn a u8 into an iterator of bool
pub fn bool_iter(mut byte: u8) -> array::IntoIter<bool, 8> {
    // turn a u8 into an iterator of bool
    let mut bools_out = [false; 8];
    for bool_out in &mut bools_out {
        *bool_out = byte & 1 == 1;
        byte >>= 1;
    }
    bools_out.into_iter()
}

fn line_to_u8_array<const DIGIT_COUNT: usize>(line: &str) -> [u8; DIGIT_COUNT] {
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

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn monitor(
    mut digit_pins: [gpio::Output<'static>; DIGIT_COUNT1],
    mut segment_pins: [gpio::Output<'static>; 8],
    signal: &'static Signal<CriticalSectionRawMutex, [u8; DIGIT_COUNT1]>,
) -> ! {
    let mut digits: [u8; DIGIT_COUNT1] = [0; DIGIT_COUNT1];
    loop {
        info!("received_bytes: {:?}", digits);
        // How many unique, non-blank digits?
        let mut map: LinearMap<u8, Vec<usize, DIGIT_COUNT1>, DIGIT_COUNT1> = LinearMap::new();
        {
            // inner scope to release the lock
            for (index, byte) in digits.iter().enumerate() {
                if *byte != 0 {
                    if let Some(vec) = map.get_mut(byte) {
                        vec.push(index).unwrap();
                    } else {
                        let mut vec = Vec::default();
                        vec.push(index).unwrap();
                        map.insert(*byte, vec).unwrap();
                    }
                }
            }
        }
        info!("map.len(): {:?}", map.len());
        match map.len() {
            // If the display should be empty, then just wait for the next update
            0 => digits = signal.wait().await,

            // If only one pattern should be displayed (even on multiple digits), display it
            // and wait for the next update
            1 => {
                // get one and only key and value
                let (byte, indexes) = map.iter().next().unwrap();
                // Set the segment pins with the bool iterator
                bool_iter(*byte)
                    .zip(segment_pins.iter_mut())
                    .for_each(|(state, segment_pin)| {
                        segment_pin.set_state(state.into()).unwrap();
                    });
                // activate the digits, wait for the next update, and deactivate the digits
                for digit_index in indexes {
                    digit_pins[*digit_index].set_low(); // Assuming common cathode setup
                }
                digits = signal.wait().await;
                for digit_index in indexes {
                    digit_pins[*digit_index].set_high();
                }
            }
            // If multiple patterns should be displayed, multiplex them until the next update
            _ => {
                'outer: loop {
                    for (byte, indexes) in &map {
                        info!(
                            "byte: {:?}, indexes: {:?},{:?},{:?},{:?}",
                            byte,
                            if !indexes.is_empty() { indexes[0] } else { 10 },
                            if indexes.len() > 1 { indexes[1] } else { 10 },
                            if indexes.len() > 2 { indexes[2] } else { 10 },
                            if indexes.len() > 3 { indexes[3] } else { 10 }
                        );
                        // Set the segment pins with the bool iterator
                        bool_iter(*byte).zip(segment_pins.iter_mut()).for_each(
                            |(state, segment_pin)| {
                                segment_pin.set_state(state.into()).unwrap();
                            },
                        );
                        // Activate, pause, and deactivate the digits
                        for digit_index in indexes {
                            digit_pins[*digit_index].set_low(); // Assuming common cathode setup
                        }
                        let sleep = 3; // cmk maybe this should depend on the # of digits
                                       // Sleep (but wake up early if the display should be updated)
                        if let Either::Second(new_digits) =
                            select(Timer::after(Duration::from_millis(sleep)), signal.wait()).await
                        {
                            digits = new_digits;
                            for digit_index in indexes {
                                digit_pins[*digit_index].set_high();
                            }
                            break 'outer;
                        }
                        for digit_index in indexes {
                            digit_pins[*digit_index].set_high();
                        }
                    }
                }
            }
        }
    }
}
