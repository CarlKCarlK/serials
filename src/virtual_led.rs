use core::array;
use embassy_executor::task;
use embassy_futures::select::select;
use embassy_rp::gpio;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use embedded_hal::digital::OutputPin; // cmk needed?
use heapless::{LinearMap, Vec};

use crate::leds::Leds;

// cmk why not have the channel send the bytes directly?
pub struct VirtualDisplay<const DIGIT_COUNT: usize> {
    mutex_digits: Mutex<CriticalSectionRawMutex, [u8; DIGIT_COUNT]>,
    update_display_channel: Channel<CriticalSectionRawMutex, (), 1>,
}

// Display #1 is a 4-digit 7-segment display
pub const DIGIT_COUNT1: usize = 4;

pub static VIRTUAL_DISPLAY1: VirtualDisplay<DIGIT_COUNT1> = VirtualDisplay {
    mutex_digits: Mutex::new([255; DIGIT_COUNT1]),
    update_display_channel: Channel::new(),
};

#[task]
pub async fn monitor_display1(
    mut digit_pins: [gpio::Output<'static>; DIGIT_COUNT1],
    mut segment_pins: [gpio::Output<'static>; 8],
) {
    VIRTUAL_DISPLAY1
        .monitor(&mut digit_pins, &mut segment_pins)
        .await;
}
// cmk would be nice to have a separate way to turn on decimal points
// cmk would be nice to have a way to pass in 4 chars
impl<const DIGIT_COUNT: usize> VirtualDisplay<DIGIT_COUNT> {
    pub async fn write_text(&'static self, text: &str) {
        let bytes = line_to_u8_array(text);
        self.write_bytes(&bytes).await;
    }
    pub async fn write_bytes(&'static self, bytes_in: &[u8; DIGIT_COUNT]) {
        {
            // inner scope to release the lock
            let mut bytes_out = self.mutex_digits.lock().await;
            for (byte_out, byte_in) in bytes_out.iter_mut().zip(bytes_in.iter()) {
                *byte_out = *byte_in;
            }
        }
        // Say that the display should be updated. If a previous update is
        // still pending, this new update can be ignored.
        let _ = self.update_display_channel.try_send(());
    }

    pub async fn write_number(&'static self, mut number: u16, padding: u8) {
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
        self.write_bytes(&bytes).await;
    }

    #[allow(clippy::needless_range_loop)]
    async fn monitor(
        &self,
        digit_pins: &mut [gpio::Output<'static>; DIGIT_COUNT],
        segment_pins: &mut [gpio::Output<'static>; 8],
    ) {
        loop {
            // How many unique, non-blank digits?
            let mut map: LinearMap<u8, Vec<usize, DIGIT_COUNT>, DIGIT_COUNT> = LinearMap::new();
            {
                // inner scope to release the lock
                let digits = self.mutex_digits.lock().await;
                let digits = digits.iter();
                for (index, byte) in digits.enumerate() {
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
            match map.len() {
                // If the display should be empty, then just wait for the next update
                0 => self.update_display_channel.receive().await,
                // If only one pattern should be displayed (even on multiple digits), display it
                // and wait for the next update
                1 => {
                    // get one and only key and value
                    let (byte, indexes) = map.iter().next().unwrap();
                    // Set the segment pins with the bool iterator
                    bool_iter(*byte).zip(segment_pins.iter_mut()).for_each(
                        |(state, segment_pin)| {
                            segment_pin.set_state(state.into()).unwrap();
                        },
                    );
                    // activate the digits, wait for the next update, and deactivate the digits
                    for digit_index in indexes {
                        digit_pins[*digit_index].set_low(); // Assuming common cathode setup
                    }
                    self.update_display_channel.receive().await;
                    for digit_index in indexes {
                        digit_pins[*digit_index].set_high();
                    }
                }
                // If multiple patterns should be displayed, multiplex them until the next update
                _ => {
                    loop {
                        for (byte, indexes) in &map {
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
                            select(
                                Timer::after(Duration::from_millis(sleep)),
                                self.update_display_channel.receive(),
                            )
                            .await;
                            for digit_index in indexes {
                                digit_pins[*digit_index].set_high();
                            }

                            // // cmk sleep for a bit with all off
                            // let sleep = 3; // cmk 3 is too long
                            // // Sleep (but wake up early if the display should be updated)
                            // select(
                            //     Timer::after(Duration::from_millis(sleep)),
                            //     self.update_display_channel.receive(),
                            // )
                            // .await;
                        }
                        // break out of multiplexing loop if the display should be updated
                        if self.update_display_channel.try_receive().is_err() {
                            break;
                        }
                    }
                }
            }
        }
    }

    // cmk remove
    // /// Turn a u8 into an iterator of bool
    // pub async fn bool_iter(&'static self, digit_index: usize) -> array::IntoIter<bool, 8> {
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
