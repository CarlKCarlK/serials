//! A device abstraction for a non-blinking 4-digit 7-segment LED display.
//!
//! See [`Led4Simple`] for usage.

use core::convert::Infallible;

use crate::Result;
use crate::bit_matrix_led4::BitMatrixLed4;
use crate::bit_matrix_led4::BitsToIndexes;
use crate::led4::OutputArray;
use crate::led4::{CELL_COUNT, MULTIPLEX_SLEEP, SEGMENT_COUNT};
#[cfg(feature = "display-trace")]
use defmt::info;
use embassy_executor::{SpawnError, Spawner};
use embassy_futures::select::{Either, select};
use embassy_rp::gpio::Level;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::Timer;

/// Static for the [`Led4Simple`] device.
pub struct Led4SimpleStatic(Signal<CriticalSectionRawMutex, BitMatrixLed4>);

impl Led4SimpleStatic {
    pub const fn new() -> Self {
        Self(Signal::new())
    }

    fn signal(&self, bit_matrix: BitMatrixLed4) {
        self.0.signal(bit_matrix);
    }

    async fn wait(&self) -> BitMatrixLed4 {
        self.0.wait().await
    }
}

/// A device abstraction for a non-blinking 4-digit 7-segment LED display.
///
/// Use this if you don't need animation or blinking. For blinking or animation support, use [`Led4`](crate::led4::Led4) instead.
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// use serials::led4_simple::{Led4Simple, Led4SimpleStatic};
/// use serials::led4::OutputArray;
/// # #[panic_handler]
/// # fn panic(_info: &core::panic::PanicInfo) -> ! { loop {} }
///
/// async fn example(p: embassy_rp::Peripherals, spawner: embassy_executor::Spawner) -> Result<(), embassy_executor::SpawnError> {
///     let cells = OutputArray::new([
///         embassy_rp::gpio::Output::new(p.PIN_1, embassy_rp::gpio::Level::High),
///         embassy_rp::gpio::Output::new(p.PIN_2, embassy_rp::gpio::Level::High),
///         embassy_rp::gpio::Output::new(p.PIN_3, embassy_rp::gpio::Level::High),
///         embassy_rp::gpio::Output::new(p.PIN_4, embassy_rp::gpio::Level::High),
///     ]);
///     let segments = OutputArray::new([
///         embassy_rp::gpio::Output::new(p.PIN_5, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_6, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_7, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_8, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_9, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_10, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_11, embassy_rp::gpio::Level::Low),
///         embassy_rp::gpio::Output::new(p.PIN_12, embassy_rp::gpio::Level::Low),
///     ]);
///     
///     static LED4_SIMPLE_STATIC: Led4SimpleStatic = Led4Simple::new_static();
///     let display = Led4Simple::new(&LED4_SIMPLE_STATIC, cells, segments, spawner)?;
///     
///     // Display "1234"
///     display.write_text(['1', '2', '3', '4']);
///     
///     // Display "rUSt"
///     display.write_text(['r', 'U', 'S', 't']);
///     
///     Ok(())
/// }
/// ```
pub struct Led4Simple<'a>(&'a Led4SimpleStatic);

impl Led4Simple<'_> {
    /// Creates static channel resources for the display.
    #[must_use]
    pub const fn new_static() -> Led4SimpleStatic {
        Led4SimpleStatic::new()
    }

    /// Creates the display device and spawns its background task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be spawned.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        led4_simple_static: &'static Led4SimpleStatic,
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        spawner: Spawner,
    ) -> Result<Self, SpawnError> {
        let token = device_loop(cell_pins, segment_pins, led4_simple_static)?;
        spawner.spawn(token);
        Ok(Self(led4_simple_static))
    }

    /// Sends text to the display.
    pub fn write_text(&self, text: [char; CELL_COUNT]) {
        #[cfg(feature = "display-trace")]
        info!("write_chars: {:?}", text);
        self.0.signal(BitMatrixLed4::from_text(&text));
    }
}

#[embassy_executor::task]
async fn device_loop(
    cell_pins: OutputArray<'static, CELL_COUNT>,
    segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    led4_simple_static: &'static Led4SimpleStatic,
) -> ! {
    let err = inner_device_loop(cell_pins, segment_pins, led4_simple_static)
        .await
        .unwrap_err();
    panic!("{err}");
}

async fn inner_device_loop(
    mut cell_pins: OutputArray<'static, CELL_COUNT>,
    mut segment_pins: OutputArray<'static, SEGMENT_COUNT>,
    led4_simple_static: &'static Led4SimpleStatic,
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
            None => bit_matrix = led4_simple_static.wait().await,
            Some((&bits, indexes)) if bits_to_indexes.len() == 1 => {
                segment_pins.set_from_nonzero_bits(bits);
                cell_pins.set_levels_at_indexes(indexes, Level::Low)?;
                bit_matrix = led4_simple_static.wait().await;
                cell_pins.set_levels_at_indexes(indexes, Level::High)?;
            }
            _ => loop {
                for (bits, indexes) in &bits_to_indexes {
                    segment_pins.set_from_nonzero_bits(*bits);
                    cell_pins.set_levels_at_indexes(indexes, Level::Low)?;
                    let timeout_or_signal =
                        select(Timer::after(MULTIPLEX_SLEEP), led4_simple_static.wait()).await;
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
