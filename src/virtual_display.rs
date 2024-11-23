use defmt::{info, unwrap};
use embassy_executor::Spawner;
use embassy_futures::select::{select, Either};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};

use crate::{bit_matrix::BitMatrix, pins::OutputArray, state_machine::ONE_HOUR};

pub struct VirtualDisplay<const CELL_COUNT: usize>(&'static Notifier<CELL_COUNT>);

pub type Notifier<const CELL_COUNT: usize> =
    Signal<CriticalSectionRawMutex, (BitMatrix<CELL_COUNT>, BlinkMode)>;

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
    pub fn write_chars(&self, chars: [char; CELL_COUNT], blink_mode: BlinkMode) {
        info!("write_chars: {:?}, blink_mode: {:?}", chars, blink_mode);
        self.0.signal((BitMatrix::from_chars(&chars), blink_mode));
    }
}

const BLINK_OFF_DELAY: Duration = Duration::from_millis(50); // const cmk
const BLINK_ON_DELAY: Duration = Duration::from_millis(150); // const cmk

#[embassy_executor::task]
#[allow(clippy::needless_range_loop)]
async fn virtual_display_task(
    // cmk does this need 'static? What does it mean?
    mut cell_pins: OutputArray<CELL_COUNT0>,
    mut segment_pins: OutputArray<SEGMENT_COUNT0>,
    // cmk rename or re-type
    notifier: &'static Notifier<CELL_COUNT0>,
) -> ! {
    let mut blink_mode = BlinkMode::Solid;
    let mut bit_matrix: BitMatrix<CELL_COUNT0> = BitMatrix::default();
    'outer: loop {
        info!("bit_matrix: {:?}", bit_matrix);
        let bits_to_indexes = bit_matrix.bits_to_indexes();
        info!("# of unique cell bit_matrix: {:?}", bits_to_indexes.len());

        match (blink_mode, bits_to_indexes.iter().next()) {
            //
            // If the display should be empty, then just wait for the next notification
            (_, None) => (bit_matrix, blink_mode) = notifier.wait().await,
            //
            // Something to see and blinking but off, then wait for the next notification or end of blink
            (BlinkMode::BlinkingButOff, _) => {
                wait_for_next_event(notifier, &mut bit_matrix, &mut blink_mode, false).await;
            }
            // If only one bit pattern should be displayed (even on multiple cells), display it
            // and wait for the next update
            (_, Some((&bits, indexes))) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low);
                wait_for_next_event(notifier, &mut bit_matrix, &mut blink_mode, false).await;
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
                    if let Either::Second(notification) = timeout_or_signal {
                        (bit_matrix, blink_mode) = notification;
                        continue 'outer;
                    }
                }
            },
        }
        blink_mode = match blink_mode {
            BlinkMode::BlinkingAndOn => BlinkMode::BlinkingButOff,
            BlinkMode::BlinkingButOff => BlinkMode::BlinkingAndOn,
            BlinkMode::Solid => BlinkMode::Solid,
        };
    }
}

async fn wait_for_next_event(
    notifier: &Notifier<CELL_COUNT0>,
    bit_matrix: &mut BitMatrix<CELL_COUNT0>,
    blink_mode: &mut BlinkMode,
    do_multiplex: bool,
) {
    let multiplex_sleep = if do_multiplex {
        MULTIPLEX_SLEEP
    } else {
        ONE_HOUR
    }; // cmk one_hour to max_duration
    let blink_sleep = match blink_mode {
        BlinkMode::BlinkingAndOn => BLINK_ON_DELAY,
        BlinkMode::BlinkingButOff => BLINK_OFF_DELAY,
        BlinkMode::Solid => ONE_HOUR,
    };
    if let Either::First((new_bit_matrix, new_blink_mode)) = select(
        notifier.wait(),
        Timer::after(multiplex_sleep.min(blink_sleep)),
    )
    .await
    {
        *bit_matrix = new_bit_matrix;
        *blink_mode = new_blink_mode;
    }
}

#[derive(Debug, Clone, Copy, defmt::Format)]
pub enum BlinkMode {
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}
