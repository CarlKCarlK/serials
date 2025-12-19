//! A device abstraction for 4-character LED matrix displays (12x4 pixels).
//!
//! See [`Led12x4`] for the main usage example.

use core::convert::Infallible;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use smart_leds::RGB8;

use crate::{Result, bit_matrix3x4, led2d};

pub use crate::led_strip_simple::Milliamps;

/// Predefined RGB color constants (RED, GREEN, BLUE, etc.).
pub use smart_leds::colors;

/// Display size in pixels
pub const COLS: usize = 12;
pub const ROWS: usize = 4;
const N: usize = COLS * ROWS;

/// Serpentine column-major mapping for 12x4 displays.
const MAPPING: [u16; N] = led2d::serpentine_column_major_mapping::<N, ROWS, COLS>();
/// Number of LEDs along the outer perimeter of the display.
// cmk need to be public?
pub const PERIMETER_LENGTH: usize = (COLS * 2) + ((ROWS - 2) * 2);
// cmk isn't this font defined elsewhere?

// cmk does this need to be limited and public
/// Maximum frames supported by [`Led12x4::animate`].
pub const ANIMATION_MAX_FRAMES: usize = 32;

type Led12x4CommandSignal = Signal<CriticalSectionRawMutex, Command>;
type Led12x4CompletionSignal = Signal<CriticalSectionRawMutex, ()>;

// cmk why public?
#[derive(Clone)]
pub enum Command {
    DisplayStatic([RGB8; N]),
    DisplayChars { chars: [char; 4], colors: [RGB8; 4] },
    Animate(Vec<Frame, ANIMATION_MAX_FRAMES>),
}

/// Frame of animation for [`Led12x4::animate`]. See [`Led12x4`] for usage.
pub type Frame = led2d::Frame<ROWS, COLS>;

/// Signal resources for [`Led12x4`].
pub struct Led12x4Static {
    command_signal: Led12x4CommandSignal,
    completion_signal: Led12x4CompletionSignal,
    led_strip_simple: crate::led_strip_simple::LedStripSimpleStatic<N>,
    led2d_static: led2d::Led2dStatic<N>,
}

/// Trait for LED strip drivers that can render a full 48-pixel frame.
pub trait LedStrip<const N: usize> {
    /// Update all pixels at once.
    async fn update_pixels(&mut self, pixels: &[smart_leds::RGB8; N]) -> Result<()>;
}

/// Unified LED strip type supporting all backends for Led12x4.
pub enum Led12x4Strip {
    SimplePio0(crate::led_strip_simple::LedStripSimple<'static, embassy_rp::peripherals::PIO0, N>),
    SimplePio1(crate::led_strip_simple::LedStripSimple<'static, embassy_rp::peripherals::PIO1, N>),
    #[cfg(feature = "pico2")]
    SimplePio2(crate::led_strip_simple::LedStripSimple<'static, embassy_rp::peripherals::PIO2, N>),
    Multi(crate::led_strip::LedStrip<N>),
}

impl LedStrip<N> for Led12x4Strip {
    async fn update_pixels(&mut self, pixels: &[RGB8; N]) -> Result<()> {
        match self {
            Self::SimplePio0(strip) => strip.update_pixels(pixels).await,
            Self::SimplePio1(strip) => strip.update_pixels(pixels).await,
            #[cfg(feature = "pico2")]
            Self::SimplePio2(strip) => strip.update_pixels(pixels).await,
            Self::Multi(strip) => strip.update_pixels(pixels).await,
        }
    }
}

impl Led12x4Static {
    pub const fn new() -> Self {
        Self {
            command_signal: Signal::new(),
            completion_signal: Signal::new(),
            led_strip_simple: crate::led_strip_simple::LedStripSimpleStatic::new_static(),
            led2d_static: led2d::Led2dStatic::new_static(),
        }
    }

    #[must_use]
    pub const fn new_static() -> Self {
        Self::new()
    }

    fn command_signal(&self) -> &Led12x4CommandSignal {
        &self.command_signal
    }

    fn completion_signal(&self) -> &Led12x4CompletionSignal {
        &self.completion_signal
    }

    fn led_strip_simple(&self) -> &crate::led_strip_simple::LedStripSimpleStatic<N> {
        &self.led_strip_simple
    }

    fn led2d_static(&self) -> &led2d::Led2dStatic<N> {
        &self.led2d_static
    }
}

/// A device abstraction for a 4-character LED matrix display (12x4 pixels) built on LED strips.
///
/// ```no_run
/// # #![no_std]
/// # use panic_probe as _;
/// # fn main() {}
/// use embassy_time::Duration;
/// use serials::led12x4::{Led12x4Static, Milliamps, colors, new_led12x4, perimeter_chase_animation};
///
/// async fn example(
///     p: embassy_rp::Peripherals,
///     spawner: embassy_executor::Spawner,
/// ) -> serials::Result<()> {
///     static LED_12X4_STATIC: Led12x4Static = Led12x4Static::new_static();
///     let led_12x4 = new_led12x4!(
///         &LED_12X4_STATIC,
///         PIN_3,
///         p.PIO1,
///         Milliamps(500),
///         spawner
///     ).await?;
///
///     led_12x4.write_text(['1', '2', '3', '4'], [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW]).await?;
///
///     // Perimeter chase animation
///     let frames = perimeter_chase_animation(true, colors::WHITE, Duration::from_millis(50));
///     led_12x4.animate(&frames).await?;
///
///     Ok(())
/// }
/// ```
pub struct Led12x4 {
    #[allow(dead_code)]
    led2d: led2d::Led2d<'static, N>,
    command_signal: &'static Led12x4CommandSignal,
    completion_signal: &'static Led12x4CompletionSignal,
}

impl Led12x4 {
    /// Number of rows in the display.
    pub const ROWS: usize = ROWS;
    /// Number of columns in the display.
    pub const COLS: usize = COLS;
    /// Total number of LEDs (ROWS * COLS).
    pub const N: usize = N;

    /// Creates static channel resources for the display.
    #[must_use]
    pub const fn new_static() -> Led12x4Static {
        Led12x4Static::new()
    }

    /// Create Led12x4 from a pre-created LED strip and spawn the background task.
    ///
    /// This is useful when you need to create the strip separately, such as when using
    /// the multi-strip driver. For simple cases with `LedStripSimple`, prefer the
    /// [`new_led12x4!`](crate::led12x4::new_led12x4) macro.
    ///
    /// # Example with multi-strip driver
    /// See the compile-only test `test_multi_strip_compiles` for usage.
    pub fn from(
        led12x4_static: &'static Led12x4Static,
        strip: Led12x4Strip,
        spawner: Spawner,
    ) -> Result<Self> {
        let command_signal = led12x4_static.command_signal();
        let completion_signal = led12x4_static.completion_signal();
        let token = led12x4_device_loop(command_signal, completion_signal, strip)?;
        spawner.spawn(token);
        let led2d = led2d::Led2d::new(led12x4_static.led2d_static(), &MAPPING, COLS);
        Ok(Self {
            led2d,
            command_signal,
            completion_signal,
        })
    }

    /// Render a fully defined frame to the display.
    ///
    /// Frame is a 2D array in row-major order where `frame[row][col]` is the pixel at (col, row).
    pub async fn write_frame(&self, frame: [[RGB8; COLS]; ROWS]) -> Result<()> {
        // Convert 2D to 1D using mapping
        let mut frame_1d = [RGB8::new(0, 0, 0); N];
        for row_index in 0..ROWS {
            for column_index in 0..COLS {
                let led_index = xy_to_index(column_index, row_index);
                frame_1d[led_index] = frame[row_index][column_index];
            }
        }
        self.command_signal.signal(Command::DisplayStatic(frame_1d));
        self.completion_signal.wait().await;
        Ok(())
    }

    // cmk update comment
    /// Render four characters with four colors.
    ///
    /// `chars` is an array of 4 characters. Supported:
    /// - `' '` (space) = blank
    /// - `'0'..'9'` = digits from the built-in font
    /// - `'A'`, `'B'`, `'C'`, `'D'`, `'E'`, `'F'`, `'I'`, `'L'`, `'N'`, `'O'`, `'R'`, `'S'`, `'T'`, `'U'` (and lowercase) = letter glyphs
    /// - any other char = solid 3×4 block
    ///
    /// Builds the entire frame and updates all pixels at once.
    pub async fn write_text(&self, chars: [char; 4], colors: [RGB8; 4]) -> Result<()> {
        self.command_signal
            .signal(Command::DisplayChars { chars, colors });
        self.completion_signal.wait().await;
        Ok(())
    }

    // cmk what is this?
    /// Loop through a sequence of animation frames until interrupted by another command.
    pub async fn animate(&self, frames: &[Frame]) -> Result<()> {
        assert!(!frames.is_empty(), "animation requires at least one frame");
        let mut sequence: Vec<Frame, ANIMATION_MAX_FRAMES> = Vec::new();
        for frame in frames {
            assert!(
                frame.duration.as_micros() > 0,
                "animation frame duration must be positive"
            );
            sequence.push(*frame).expect("animation sequence fits");
        }
        self.command_signal.signal(Command::Animate(sequence));
        self.completion_signal.wait().await;
        Ok(())
    }
}

#[inline]
/// Converts a column/row pair into the serpentine LED index for this display.
pub fn xy_to_index(column_index: usize, row_index: usize) -> usize {
    MAPPING[row_index * COLS + column_index] as usize
}

/// Build a full display frame for the provided 4-character text and colors.
///
/// This is useful when constructing custom animations manually. See the `led12x4`
/// example for usage.
#[must_use]
pub fn text_frame(chars: [char; 4], colors: [RGB8; 4]) -> [[RGB8; COLS]; ROWS] {
    let black = RGB8::new(0, 0, 0);
    let mut frame_1d = [black; N];

    for (character_index, &character) in chars.iter().enumerate() {
        let color = colors[character_index];
        let base_column_index = character_index * 3;
        let rows = bit_matrix3x4::glyph_rows(character);
        render_glyph(rows, color, base_column_index, &mut frame_1d, black);
    }

    // Convert 1D to 2D
    let mut frame_2d = [[black; COLS]; ROWS];
    for row_index in 0..ROWS {
        for column_index in 0..COLS {
            let led_index = xy_to_index(column_index, row_index);
            frame_2d[row_index][column_index] = frame_1d[led_index];
        }
    }

    frame_2d
}

fn render_glyph(
    rows: [u8; 4],
    glyph_color: RGB8,
    base_column_index: usize,
    frame: &mut [RGB8; N],
    background_color: RGB8,
) {
    for row_index in 0..ROWS {
        let row_bits = rows[row_index];
        for column_offset in 0..3 {
            let bit = (row_bits >> (2 - column_offset)) & 1;
            let pixel_index = xy_to_index(base_column_index + column_offset, row_index);
            let pixel_color = if bit != 0 {
                glyph_color
            } else {
                background_color
            };
            frame[pixel_index] = pixel_color;
        }
    }
}

/// Creates a single-dot perimeter chase animation around the display edges.
///
/// Use the returned frames with [`Led12x4::animate`] to run the loop.
#[must_use]
pub fn perimeter_chase_animation(
    clockwise: bool,
    color: RGB8,
    duration: Duration,
) -> [Frame; PERIMETER_LENGTH] {
    assert!(
        duration.as_micros() > 0,
        "perimeter animation duration must be positive"
    );
    let perimeter_indices = perimeter_indices(clockwise);
    let black = RGB8::new(0, 0, 0);
    core::array::from_fn(|frame_index| {
        let mut frame = [[black; COLS]; ROWS];
        let led_index = perimeter_indices[frame_index];
        // Convert 1D index back to 2D coordinates
        for row_index in 0..ROWS {
            for column_index in 0..COLS {
                if xy_to_index(column_index, row_index) == led_index {
                    frame[row_index][column_index] = color;
                    break;
                }
            }
        }
        led2d::Frame::new(frame, duration)
    })
}

// cmk look at every function and decide if it's necessary
#[must_use]
/// Returns the LED indexes around the perimeter, starting at the top-left corner.
pub fn perimeter_indices(clockwise: bool) -> [usize; PERIMETER_LENGTH] {
    let mut indices = [0usize; PERIMETER_LENGTH];
    let mut write_index = 0;
    let mut push_index = |column_index: usize, row_index: usize| {
        indices[write_index] = xy_to_index(column_index, row_index);
        write_index += 1;
    };

    for column_index in 0..COLS {
        push_index(column_index, 0);
    }
    for row_index in 1..ROWS {
        push_index(COLS - 1, row_index);
    }
    for column_index in (0..(COLS - 1)).rev() {
        push_index(column_index, ROWS - 1);
    }
    for row_index in (1..(ROWS - 1)).rev() {
        push_index(0, row_index);
    }

    debug_assert_eq!(write_index, PERIMETER_LENGTH);

    if clockwise {
        indices
    } else {
        let mut reversed = [0usize; PERIMETER_LENGTH];
        for (reverse_index, &perimeter_index) in indices.iter().enumerate() {
            reversed[PERIMETER_LENGTH - 1 - reverse_index] = perimeter_index;
        }
        reversed
    }
}

async fn inner_device_loop(
    command_signal: &'static Led12x4CommandSignal,
    completion_signal: &'static Led12x4CompletionSignal,
    mut strip: impl LedStrip<{ COLS * ROWS }>,
) -> Result<Infallible> {
    // Wait for first command instead of displaying blank frame
    let mut command = command_signal.wait().await;

    loop {
        command = match command {
            Command::DisplayStatic(frame) => {
                strip.update_pixels(&frame).await?;
                completion_signal.signal(());
                command_signal.wait().await
            }
            Command::DisplayChars { chars, colors } => {
                let frame_2d = text_frame(chars, colors);
                // Convert 2D to 1D
                let mut frame_1d = [RGB8::new(0, 0, 0); N];
                for row_index in 0..ROWS {
                    for column_index in 0..COLS {
                        let led_index = xy_to_index(column_index, row_index);
                        frame_1d[led_index] = frame_2d[row_index][column_index];
                    }
                }
                strip.update_pixels(&frame_1d).await?;
                completion_signal.signal(());
                command_signal.wait().await
            }
            Command::Animate(frames) => {
                run_animation_loop(frames, command_signal, completion_signal, &mut strip).await?
            }
        };
    }
}

async fn run_animation_loop(
    frames: Vec<Frame, ANIMATION_MAX_FRAMES>,
    command_signal: &'static Led12x4CommandSignal,
    completion_signal: &'static Led12x4CompletionSignal,
    strip: &mut impl LedStrip<{ COLS * ROWS }>,
) -> Result<Command> {
    assert!(!frames.is_empty(), "animation requires at least one frame");
    let frame_count = frames.len();
    let mut frame_index = 0;

    loop {
        let frame = frames[frame_index];
        // Convert 2D frame to 1D using the mapping
        let mut frame_1d = [RGB8::new(0, 0, 0); N];
        for row_index in 0..ROWS {
            for column_index in 0..COLS {
                let led_index = xy_to_index(column_index, row_index);
                frame_1d[led_index] = frame.frame[row_index][column_index];
            }
        }
        strip.update_pixels(&frame_1d).await?;
        if frame_index == 0 {
            completion_signal.signal(());
        }
        match select(command_signal.wait(), Timer::after(frame.duration)).await {
            Either::First(command) => return Ok(command),
            Either::Second(()) => {
                frame_index = (frame_index + 1) % frame_count;
            }
        }
    }
}

#[embassy_executor::task]
async fn led12x4_device_loop(
    command_signal: &'static Led12x4CommandSignal,
    completion_signal: &'static Led12x4CompletionSignal,
    strip: Led12x4Strip,
) -> ! {
    let err = inner_device_loop(command_signal, completion_signal, strip)
        .await
        .unwrap_err();
    panic!("{err}");
}

impl Led12x4 {
    /// Create a `Led12x4` display using PIO0 with an internal `LedStripSimple`.
    ///
    /// This is a self-contained constructor that internally creates the LED strip.
    /// For custom LED strip types, use [`Led12x4::from_led_strip`].
    ///
    /// # Parameters
    /// - `led12x4_static`: Static resources for the display
    /// - `pio`: PIO0 peripheral
    /// - `pin`: GPIO pin for LED data
    /// - `max_current`: Maximum current budget in milliamps
    /// - `spawner`: Task spawner for background device loop
    #[doc(hidden)]
    pub async fn new_pio0(
        led12x4_static: &'static Led12x4Static,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
        pin: embassy_rp::Peri<'static, impl embassy_rp::pio::PioPin>,
        max_current: Milliamps,
        spawner: Spawner,
    ) -> Result<Self> {
        let strip = crate::led_strip_simple::LedStripSimple::new_pio0(
            led12x4_static.led_strip_simple(),
            pio,
            pin,
            max_current,
        )
        .await;
        Self::from(led12x4_static, Led12x4Strip::SimplePio0(strip), spawner)
    }
    /// Create a `Led12x4` display using PIO1 with an internal `LedStripSimple`.
    ///
    /// This is a self-contained constructor that internally creates the LED strip.
    /// For custom LED strip types, use [`Led12x4::from_led_strip`].
    ///
    /// # Parameters
    /// - `led12x4_static`: Static resources for the display
    /// - `pio`: PIO1 peripheral
    /// - `pin`: GPIO pin for LED data
    /// - `max_current`: Maximum current budget in milliamps
    /// - `spawner`: Task spawner for background device loop
    #[doc(hidden)]
    pub async fn new_pio1(
        led12x4_static: &'static Led12x4Static,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO1>,
        pin: embassy_rp::Peri<'static, impl embassy_rp::pio::PioPin>,
        max_current: Milliamps,
        spawner: Spawner,
    ) -> Result<Self> {
        let strip = crate::led_strip_simple::LedStripSimple::new_pio1(
            led12x4_static.led_strip_simple(),
            pio,
            pin,
            max_current,
        )
        .await;
        Self::from(led12x4_static, Led12x4Strip::SimplePio1(strip), spawner)
    }

    #[cfg(feature = "pico2")]
    /// Create a `Led12x4` display using PIO2 with an internal `LedStripSimple` (Pico 2 only).
    ///
    /// This is a self-contained constructor that internally creates the LED strip.
    /// For custom LED strip types, use [`Led12x4::from_led_strip`].
    ///
    /// # Parameters
    /// - `led12x4_static`: Static resources for the display
    /// - `pio`: PIO2 peripheral
    /// - `pin`: GPIO pin for LED data
    /// - `max_current`: Maximum current budget in milliamps
    /// - `spawner`: Task spawner for background device loop
    #[doc(hidden)]
    pub async fn new_pio2(
        led12x4_static: &'static Led12x4Static,
        pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO2>,
        pin: embassy_rp::Peri<'static, impl embassy_rp::pio::PioPin>,
        max_current: Milliamps,
        spawner: Spawner,
    ) -> Result<Self> {
        let strip = crate::led_strip_simple::LedStripSimple::new_pio2(
            led12x4_static.led_strip_simple(),
            pio,
            pin,
            max_current,
        )
        .await;
        Self::from(led12x4_static, Led12x4Strip::SimplePio2(strip), spawner)
    }
}

// cmk can we animate pixels directly?

/// Macro wrapper that routes to `new_pio0`/`new_pio1`/`new_pio2` and fails fast if PIO2 is used on Pico 1.
/// See the usage example on [`Led12x4`].
pub macro new_led12x4 {
    (
        $led12x4_static:expr,
        $pin:ident,
        $peripherals:ident . PIO0,
        $max_current:expr,
        $spawner:expr
    ) => {
        $crate::led12x4::Led12x4::new_pio0(
            $led12x4_static,
            $peripherals.PIO0,
            $peripherals.$pin,
            $max_current,
            $spawner,
        )
    },
    (
        $led12x4_static:expr,
        $pin:ident,
        $peripherals:ident . PIO1,
        $max_current:expr,
        $spawner:expr
    ) => {
        $crate::led12x4::Led12x4::new_pio1(
            $led12x4_static,
            $peripherals.PIO1,
            $peripherals.$pin,
            $max_current,
            $spawner,
        )
    },
    (
        $led12x4_static:expr,
        $pin:ident,
        $peripherals:ident . PIO2,
        $max_current:expr,
        $spawner:expr
    ) => {{
        #[cfg(feature = "pico2")]
        {
            $crate::led12x4::Led12x4::new_pio2(
                $led12x4_static,
                $peripherals.PIO2,
                $peripherals.$pin,
                $max_current,
                $spawner,
            )
        }
        #[cfg(not(feature = "pico2"))]
        {
            compile_error!("PIO2 is only available on Pico 2 (rp235x); enable the pico2 feature or choose PIO0/PIO1");
        }
    }}
}
