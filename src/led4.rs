//! A device abstraction for a 4-digit, 7-segment LED display with blinking support.
//!
//! This module provides hardware abstractions for controlling common-cathode
//! 4-digit 7-segment LED displays. Supports displaying text and numbers with
//! optional blinking.
//!
//! See [`Led4`] for the main device abstraction and usage examples.

use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::Vec;

use crate::Result;
use crate::led4_simple::{Led4Simple, Led4SimpleStatic};

#[cfg(feature = "display-trace")]
use defmt::info;

// ============================================================================
// OutputArray Submodule
// ============================================================================

mod output_array;
pub use output_array::OutputArray;

// ============================================================================
// Constants
// ============================================================================

/// The number of cells (digits) in the display.
pub(crate) const CELL_COUNT_U8: u8 = 4;
pub(crate) const CELL_COUNT: usize = CELL_COUNT_U8 as usize;

/// The number of segments per digit in the display.
pub(crate) const SEGMENT_COUNT: usize = 8;

/// Sleep duration between multiplexing updates.
pub(crate) const MULTIPLEX_SLEEP: Duration = Duration::from_millis(3);

/// Delay for the "off" state during blinking.
const BLINK_OFF_DELAY: Duration = Duration::from_millis(50);

/// Delay for the "on" state during blinking.
const BLINK_ON_DELAY: Duration = Duration::from_millis(150);

const ANIMATION_MAX_FRAMES: usize = 16;

// ============================================================================
// BlinkState Enum
// ============================================================================

/// Blinking behavior for 4-digit LED displays.
///
/// Used with [`Led4::write_text()`] to control whether the display blinks.
/// See the [`Led4`] documentation for usage examples.
#[derive(Debug, Clone, Copy, defmt::Format, Default)]
pub enum BlinkState {
    #[default]
    Solid,
    BlinkingAndOn,
    BlinkingButOff,
}

#[derive(Clone)]
pub enum Led4Command {
    Text {
        blink_state: BlinkState,
        text: [char; CELL_COUNT],
    },
    Animation(Led4Animation),
}

#[derive(Clone, Copy)]
pub struct AnimationFrame {
    pub text: [char; CELL_COUNT],
    pub duration: Duration,
}

impl AnimationFrame {
    #[must_use]
    pub const fn new(text: [char; CELL_COUNT], duration: Duration) -> Self {
        Self { text, duration }
    }
}

pub type Led4Animation = Vec<AnimationFrame, ANIMATION_MAX_FRAMES>;

// ============================================================================
// Led4 Virtual Device
// ============================================================================

/// A device abstraction for a 4-digit, 7-segment LED display with blinking support.
///
/// # Hardware Requirements
///
/// This abstraction is designed for common-cathode 7-segment displays where:
/// - Cell pins control which digit is active (LOW = on, HIGH = off)
/// - Segment pins control which segments light up (HIGH = on, LOW = off)
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use embassy_rp::gpio::{Level, Output};
/// use serials::{Error, led4::{BlinkState, Led4, Led4Static, OutputArray}};
/// # use embassy_executor::Spawner;
///
/// async fn example(p: embassy_rp::Peripherals, spawner: Spawner) -> Result<(), Error> {
///     // Set up cell pins (control which digit is active)
///     let cells = OutputArray::new([
///         Output::new(p.PIN_1, Level::High),
///         Output::new(p.PIN_2, Level::High),
///         Output::new(p.PIN_3, Level::High),
///         Output::new(p.PIN_4, Level::High),
///     ]);
///
///     // Set up segment pins (control which segments light up)
///     let segments = OutputArray::new([
///         Output::new(p.PIN_5, Level::Low),  // Segment A
///         Output::new(p.PIN_6, Level::Low),  // Segment B
///         Output::new(p.PIN_7, Level::Low),  // Segment C
///         Output::new(p.PIN_8, Level::Low),  // Segment D
///         Output::new(p.PIN_9, Level::Low),  // Segment E
///         Output::new(p.PIN_10, Level::Low), // Segment F
///         Output::new(p.PIN_11, Level::Low), // Segment G
///         Output::new(p.PIN_12, Level::Low), // Decimal point
///     ]);
///
///     // Create the display
///     static LED4_STATIC: Led4Static = Led4::new_static();
///     let display = Led4::new(&LED4_STATIC, cells, segments, spawner)?;
///
///     // Display "1234" (solid)
///     display.write_text(BlinkState::Solid, ['1', '2', '3', '4']);
///     
///     // Display "rUSt" blinking
///     display.write_text(BlinkState::BlinkingAndOn, ['r', 'U', 'S', 't']);
///     
///     Ok(())
/// }
/// ```
///
/// Beyond simple text, the driver can loop animations via [`Led4::animate_text`].
/// The struct owns the background task and signal wiring; create it once with
/// [`Led4::new`] and use the returned handle for all display updates.
pub struct Led4<'a>(&'a Led4OuterStatic);

/// Static for the [`Led4`] device.
pub type Led4Static = (Led4OuterStatic, Led4SimpleStatic);

/// Signal for sending display commands to the [`Led4`] device.
pub(crate) type Led4OuterStatic = Signal<CriticalSectionRawMutex, Led4Command>;

impl Led4<'_> {
    /// Creates the display device and spawns its background task; see [`Led4`] docs.
    #[must_use = "Must be used to manage the spawned task"]
    pub fn new(
        led4_static: &'static Led4Static,
        cell_pins: OutputArray<'static, CELL_COUNT>,
        segment_pins: OutputArray<'static, SEGMENT_COUNT>,
        spawner: Spawner,
    ) -> Result<Self> {
        let (outer_static, display_static) = led4_static;
        let display = Led4Simple::new(display_static, cell_pins, segment_pins, spawner)?;
        let token = device_loop(outer_static, display)?;
        spawner.spawn(token);
        Ok(Self(outer_static))
    }

    /// Creates static channel resources for [`Led4::new`]; see [`Led4`] docs.
    #[must_use]
    pub const fn new_static() -> Led4Static {
        (Signal::new(), Led4Simple::new_static())
    }

    /// Sends text to the display with optional blinking.
    pub fn write_text(&self, blink_state: BlinkState, text: [char; CELL_COUNT]) {
        #[cfg(feature = "display-trace")]
        info!("blink_state: {:?}, text: {:?}", blink_state, text);
        self.0.signal(Led4Command::Text { blink_state, text });
    }

    /// Plays a looped text animation using the provided frames.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # #![no_std]
    /// # #![no_main]
    /// # use panic_probe as _;
    /// # use embassy_rp::gpio::{Level, Output};
    /// # use embassy_executor::Spawner;
    /// # use serials::led4::{Led4, Led4Static, OutputArray, AnimationFrame, Led4Animation};
    /// # async fn demo(peripherals: embassy_rp::Peripherals, spawner: Spawner) -> serials::Result<()> {
    /// let cells = OutputArray::new([
    ///     Output::new(peripherals.PIN_1, Level::High),
    ///     Output::new(peripherals.PIN_2, Level::High),
    ///     Output::new(peripherals.PIN_3, Level::High),
    ///     Output::new(peripherals.PIN_4, Level::High),
    /// ]);
    /// let segments = OutputArray::new([
    ///     Output::new(peripherals.PIN_5, Level::Low),
    ///     Output::new(peripherals.PIN_6, Level::Low),
    ///     Output::new(peripherals.PIN_7, Level::Low),
    ///     Output::new(peripherals.PIN_8, Level::Low),
    ///     Output::new(peripherals.PIN_9, Level::Low),
    ///     Output::new(peripherals.PIN_10, Level::Low),
    ///     Output::new(peripherals.PIN_11, Level::Low),
    ///     Output::new(peripherals.PIN_12, Level::Low),
    /// ]);
    /// static LED4_STATIC: Led4Static = Led4::new_static();
    /// let display = Led4::new(&LED4_STATIC, cells, segments, spawner)?;
    /// let mut animation = Led4Animation::new();
    /// animation.push(AnimationFrame::new(['-', '-', '-', '-'], embassy_time::Duration::from_millis(100))).ok();
    /// animation.push(AnimationFrame::new([' ', ' ', ' ', ' '], embassy_time::Duration::from_millis(100))).ok();
    /// display.animate_text(animation);
    /// # Ok(()) }
    /// ```
    pub fn animate_text(&self, animation: Led4Animation) {
        self.0.signal(Led4Command::Animation(animation));
    }
}

#[embassy_executor::task]
async fn device_loop(outer_static: &'static Led4OuterStatic, display: Led4Simple<'static>) -> ! {
    let mut command = Led4Command::Text {
        blink_state: BlinkState::default(),
        text: [' '; CELL_COUNT],
    };

    loop {
        command = match command {
            Led4Command::Text { blink_state, text } => {
                run_text_loop(blink_state, text, outer_static, &display).await
            }
            Led4Command::Animation(animation) => {
                run_animation_loop(animation, outer_static, &display).await
            }
        };
    }
}

async fn run_text_loop(
    mut blink_state: BlinkState,
    text: [char; CELL_COUNT],
    outer_static: &'static Led4OuterStatic,
    display: &Led4Simple<'_>,
) -> Led4Command {
    loop {
        match blink_state {
            BlinkState::Solid => {
                display.write_text(text);
                return outer_static.wait().await;
            }
            BlinkState::BlinkingAndOn => {
                display.write_text(text);
                match select(outer_static.wait(), Timer::after(BLINK_ON_DELAY)).await {
                    Either::First(command) => return command,
                    Either::Second(()) => blink_state = BlinkState::BlinkingButOff,
                }
            }
            BlinkState::BlinkingButOff => {
                display.write_text([' '; CELL_COUNT]);
                match select(outer_static.wait(), Timer::after(BLINK_OFF_DELAY)).await {
                    Either::First(command) => return command,
                    Either::Second(()) => blink_state = BlinkState::BlinkingAndOn,
                }
            }
        }
    }
}

async fn run_animation_loop(
    animation: Led4Animation,
    outer_static: &'static Led4OuterStatic,
    display: &Led4Simple<'_>,
) -> Led4Command {
    if animation.is_empty() {
        return outer_static.wait().await;
    }

    let frames = animation;
    let len = frames.len();
    let mut index = 0;

    loop {
        let frame = frames[index];
        display.write_text(frame.text);
        match select(outer_static.wait(), Timer::after(frame.duration)).await {
            Either::First(command) => return command,
            Either::Second(()) => {
                index = (index + 1) % len;
            }
        }
    }
}
