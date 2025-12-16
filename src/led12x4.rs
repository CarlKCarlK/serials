//! A device abstraction for 4-character LED matrix displays (12x4 pixels).
//!
//! See [`Led12x4`] for the main usage example.

use core::{convert::Infallible, marker::PhantomData};
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use heapless::Vec;
use smart_leds::RGB8;

use crate::{LedStripDevice, Result, led_strip::LedStrip};

/// Predefined RGB color constants (RED, GREEN, BLUE, etc.).
pub use smart_leds::colors;

/// 3×4 font for digits 0..9. Each entry is 4 rows of 3 bits (LSB = rightmost column).
const FONT: [[u8; 4]; 10] = [
    // 0
    [0b111, 0b101, 0b101, 0b111],
    // 1
    [0b010, 0b110, 0b010, 0b111],
    // 2
    [0b110, 0b001, 0b010, 0b111],
    // 3
    [0b111, 0b001, 0b011, 0b111],
    // 4
    [0b101, 0b101, 0b111, 0b001],
    // 5
    [0b111, 0b100, 0b011, 0b111],
    // 6
    [0b100, 0b111, 0b101, 0b111],
    // 7
    [0b111, 0b001, 0b010, 0b100],
    // 8
    [0b111, 0b101, 0b010, 0b111],
    // 9
    [0b111, 0b101, 0b111, 0b001],
];

// cmk need to be public?
/// Display size in pixels
pub const COLS: usize = 12;
pub const ROWS: usize = 4;
/// Number of LEDs along the outer perimeter of the display.
// cmk need to be public?
pub const PERIMETER_LENGTH: usize = (COLS * 2) + ((ROWS - 2) * 2);
// cmk isn't this font defined elsewhere?

const LETTER_A: [u8; 4] = [0b111, 0b101, 0b111, 0b101];
const LETTER_B: [u8; 4] = [0b110, 0b111, 0b101, 0b110];
const LETTER_C: [u8; 4] = [0b111, 0b100, 0b100, 0b111];
const LETTER_D: [u8; 4] = [0b110, 0b101, 0b101, 0b110];
const LETTER_E: [u8; 4] = [0b111, 0b110, 0b100, 0b111];
const LETTER_F: [u8; 4] = [0b111, 0b110, 0b100, 0b100];
const LETTER_I: [u8; 4] = [0b111, 0b010, 0b010, 0b111];
const LETTER_L: [u8; 4] = [0b100, 0b100, 0b100, 0b111];
const LETTER_N: [u8; 4] = [0b101, 0b111, 0b111, 0b101];
const LETTER_O: [u8; 4] = [0b111, 0b101, 0b101, 0b111];

// cmk does this need to be limited and public
/// Maximum frames supported by [`Led12x4::animate_frames`].
pub const ANIMATION_MAX_FRAMES: usize = 32;

type Led12x4CommandSignal = Signal<CriticalSectionRawMutex, Command>;
type Led12x4CompletionSignal = Signal<CriticalSectionRawMutex, ()>;

// cmk why public?
#[derive(Clone)]
pub enum Command {
    DisplayStatic([RGB8; COLS * ROWS]),
    DisplayChars { chars: [char; 4], colors: [RGB8; 4] },
    Animate(Vec<AnimationFrame, ANIMATION_MAX_FRAMES>),
}

/// Frame of animation for [`Led12x4::animate_frames`]. See [`Led12x4`] for usage.
#[derive(Clone, Copy, Debug)]
pub struct AnimationFrame {
    pub frame: [RGB8; COLS * ROWS],
    pub duration: Duration,
}

impl AnimationFrame {
    #[must_use]
    pub const fn new(frame: [RGB8; COLS * ROWS], duration: Duration) -> Self {
        Self { frame, duration }
    }
}

/// Signal resources for [`Led12x4`].
pub struct Led12x4Static {
    command_signal: Led12x4CommandSignal,
    completion_signal: Led12x4CompletionSignal,
}

impl Led12x4Static {
    pub const fn new() -> Self {
        Self {
            command_signal: Signal::new(),
            completion_signal: Signal::new(),
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
}

/// A device abstraction for a 4-character LED matrix display (12x4 pixels) built on LED strips.
///
/// ```no_run
/// # #![no_std]
/// # use panic_probe as _;
/// # async fn main(_spawner: embassy_executor::Spawner) {}
/// use embassy_time::Duration;
/// use serials::led12x4::{Led12x4, Led12x4Static, colors, perimeter_chase_animation};
/// use serials::led_strip::{LedStrip, LedStripStatic};
///
/// async fn example(spawner: embassy_executor::Spawner) -> serials::Result<()> {
///     static LED_STRIP_STATIC: LedStripStatic<48> = LedStrip::new_static();
///     let led_strip = LedStrip::new(&LED_STRIP_STATIC)?;
///
///     static LED12X4_STATIC: Led12x4Static = Led12x4::new_static();
///     let led12x4 = Led12x4::new(&LED12X4_STATIC, led_strip, spawner)?;
///
///     led12x4.display(['1', '2', '3', '4'], [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW]).await?;
///
///     // cmk too complicated for this example.
///     // Perimeter chase animation
///     let frames = perimeter_chase_animation(true, colors::WHITE, Duration::from_millis(50));
///     led12x4.animate_frames(frames).await?;
///
///     Ok(())
/// }
/// ```
pub struct Led12x4<T: LedStripDevice<{ COLS * ROWS }>> {
    command_signal: &'static Led12x4CommandSignal,
    completion_signal: &'static Led12x4CompletionSignal,
    _marker: PhantomData<T>,
}

// cmk bad name for a trait
pub trait Led12x4Spawn: LedStripDevice<{ COLS * ROWS }> + 'static {
    fn spawn_led12x4(
        self,
        command_signal: &'static Led12x4CommandSignal,
        completion_signal: &'static Led12x4CompletionSignal,
        spawner: Spawner,
    ) -> Result<()>;
}

impl<T: LedStripDevice<{ COLS * ROWS }> + 'static> Led12x4<T> {
    /// Creates static channel resources for the display.
    #[must_use]
    pub const fn new_static() -> Led12x4Static {
        Led12x4Static::new()
    }

    /// Create the device and spawn its background task; see [`Led12x4`] docs.
    #[allow(private_bounds)]
    pub fn new(led12x4_static: &'static Led12x4Static, strip: T, spawner: Spawner) -> Result<Self>
    where
        T: Led12x4Spawn,
    {
        let command_signal = led12x4_static.command_signal();
        let completion_signal = led12x4_static.completion_signal();
        strip.spawn_led12x4(command_signal, completion_signal, spawner)?;
        Ok(Self {
            command_signal,
            completion_signal,
            _marker: PhantomData,
        })
    }

    // cmk what is this?
    /// Render a fully defined frame to the display.
    pub async fn display_frame(&self, frame: [RGB8; COLS * ROWS]) -> Result<()> {
        self.command_signal.signal(Command::DisplayStatic(frame));
        self.completion_signal.wait().await;
        Ok(())
    }

    /// Render four characters with four colors.
    ///
    /// `chars` is an array of 4 characters. Supported:
    /// - `' '` (space) = blank
    /// - `'0'..'9'` = digits from FONT
    /// - `'C'`, `'D'`, `'E'`, `'N'`, `'O'` (and lowercase) = letter glyphs
    /// - any other char = solid 3×4 block
    ///
    /// Builds the entire frame and updates all pixels at once.
    // cmk should this be write_text?
    // cmk why does led4 have state first and text 2nd and this has text first and colors second?
    pub async fn display(&self, chars: [char; 4], colors: [RGB8; 4]) -> Result<()> {
        self.command_signal
            .signal(Command::DisplayChars { chars, colors });
        self.completion_signal.wait().await;
        Ok(())
    }

    // cmk kill this!
    /// Display "1234" in red, green, blue, and yellow respectively.
    pub async fn display_1234(&self) -> Result<()> {
        let red = RGB8::new(32, 0, 0);
        let green = RGB8::new(0, 32, 0);
        let blue = RGB8::new(0, 0, 32);
        let yellow = RGB8::new(32, 32, 0);

        self.display(['1', '2', '3', '4'], [red, green, blue, yellow])
            .await
    }

    // cmk what is this?
    /// Loop through a sequence of animation frames until interrupted by another command.
    pub async fn animate_frames(
        &self,
        frames: Vec<AnimationFrame, ANIMATION_MAX_FRAMES>,
    ) -> Result<()> {
        assert!(!frames.is_empty(), "animation requires at least one frame");
        for frame in frames.iter() {
            assert!(
                frame.duration.as_micros() > 0,
                "animation frame duration must be positive"
            );
        }
        self.command_signal.signal(Command::Animate(frames));
        self.completion_signal.wait().await;
        Ok(())
    }
}

#[inline]
/// Converts a column/row pair into the serpentine LED index for this display.
pub const fn xy_to_index(column_index: usize, row_index: usize) -> usize {
    // Column-major with serpentine: even columns go down (top-to-bottom), odd columns go up (bottom-to-top)
    if column_index % 2 == 0 {
        // Even column: top-to-bottom
        column_index * ROWS + row_index
    } else {
        // Odd column: bottom-to-top (reverse y)
        column_index * ROWS + (ROWS - 1 - row_index)
    }
}

fn build_frame(chars: [char; 4], colors: [RGB8; 4]) -> [RGB8; COLS * ROWS] {
    let black = RGB8::new(0, 0, 0);
    let mut frame = [black; COLS * ROWS];

    for (character_index, &character) in chars.iter().enumerate() {
        let color = colors[character_index];
        let base_column_index = character_index * 3;

        match glyph_rows(character) {
            Some(rows) => render_glyph(rows, color, base_column_index, &mut frame, black),
            None => match character {
                ' ' => {
                    for row_index in 0..ROWS {
                        for column_offset in 0..3 {
                            let pixel_index =
                                xy_to_index(base_column_index + column_offset, row_index);
                            frame[pixel_index] = black;
                        }
                    }
                }
                _ => {
                    for row_index in 0..ROWS {
                        for column_offset in 0..3 {
                            let pixel_index =
                                xy_to_index(base_column_index + column_offset, row_index);
                            frame[pixel_index] = color;
                        }
                    }
                }
            },
        }
    }

    frame
}

fn glyph_rows(character: char) -> Option<[u8; 4]> {
    match character {
        '0'..='9' => Some(FONT[(character as u8 - b'0') as usize]),
        'A' | 'a' => Some(LETTER_A),
        'B' | 'b' => Some(LETTER_B),
        'C' | 'c' => Some(LETTER_C),
        'D' | 'd' => Some(LETTER_D),
        'E' | 'e' => Some(LETTER_E),
        'F' | 'f' => Some(LETTER_F),
        'I' | 'i' => Some(LETTER_I),
        'L' | 'l' => Some(LETTER_L),
        'N' | 'n' => Some(LETTER_N),
        'O' | 'o' => Some(LETTER_O),
        _ => None,
    }
}

fn render_glyph(
    rows: [u8; 4],
    glyph_color: RGB8,
    base_column_index: usize,
    frame: &mut [RGB8; COLS * ROWS],
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
/// Use the returned frames with [`Led12x4::animate_frames`] to run the loop.
#[must_use]
pub fn perimeter_chase_animation(
    clockwise: bool,
    color: RGB8,
    duration: Duration,
) -> Vec<AnimationFrame, ANIMATION_MAX_FRAMES> {
    assert!(
        duration.as_micros() > 0,
        "perimeter animation duration must be positive"
    );
    assert!(
        PERIMETER_LENGTH <= ANIMATION_MAX_FRAMES,
        "perimeter animation exceeds frame capacity"
    );
    let perimeter_indices = perimeter_indices(clockwise);
    let black = RGB8::new(0, 0, 0);
    let mut animation = Vec::new();
    for perimeter_index in perimeter_indices {
        let mut frame = [black; COLS * ROWS];
        frame[perimeter_index] = color;
        animation
            .push(AnimationFrame::new(frame, duration))
            .expect("perimeter animation fits");
    }
    animation
}

// cmk look at every function and decide if it's necessary
/// Creates a blinking text animation that alternates between the given text and blank.
#[must_use]
pub fn blink_text_animation(
    chars: [char; 4],
    colors: [RGB8; 4],
    on_duration: Duration,
    off_duration: Duration,
) -> Vec<AnimationFrame, ANIMATION_MAX_FRAMES> {
    assert!(
        on_duration.as_micros() > 0,
        "blink on_duration must be positive"
    );
    assert!(
        off_duration.as_micros() > 0,
        "blink off_duration must be positive"
    );
    let black = RGB8::new(0, 0, 0);
    let on_frame = build_frame(chars, colors);
    let off_frame = [black; COLS * ROWS];
    let mut animation = Vec::new();
    animation
        .push(AnimationFrame::new(on_frame, on_duration))
        .expect("blink animation fits");
    animation
        .push(AnimationFrame::new(off_frame, off_duration))
        .expect("blink animation fits");
    animation
}

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
    mut strip: impl LedStripDevice<{ COLS * ROWS }>,
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
                let frame = build_frame(chars, colors);
                strip.update_pixels(&frame).await?;
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
    frames: Vec<AnimationFrame, ANIMATION_MAX_FRAMES>,
    command_signal: &'static Led12x4CommandSignal,
    completion_signal: &'static Led12x4CompletionSignal,
    strip: &mut impl LedStripDevice<{ COLS * ROWS }>,
) -> Result<Command> {
    assert!(!frames.is_empty(), "animation requires at least one frame");
    let frame_count = frames.len();
    let mut frame_index = 0;

    loop {
        let frame = frames[frame_index];
        strip.update_pixels(&frame.frame).await?;
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

macro_rules! impl_led12x4_spawn {
    ($task:ident, $ty:ty) => {
        #[embassy_executor::task]
        async fn $task(
            command_signal: &'static Led12x4CommandSignal,
            completion_signal: &'static Led12x4CompletionSignal,
            strip: $ty,
        ) -> ! {
            let err = inner_device_loop(command_signal, completion_signal, strip)
                .await
                .unwrap_err();
            panic!("{err}");
        }

        impl Led12x4Spawn for $ty {
            fn spawn_led12x4(
                self,
                command_signal: &'static Led12x4CommandSignal,
                completion_signal: &'static Led12x4CompletionSignal,
                spawner: Spawner,
            ) -> Result<()> {
                let token = $task(command_signal, completion_signal, self)?;
                spawner.spawn(token);
                Ok(())
            }
        }
    };
}

impl_led12x4_spawn!(
    led12x4_device_loop_led_strip_simple_pio0,
    crate::led_strip_simple::LedStripSimple<
        'static,
        embassy_rp::peripherals::PIO0,
        { COLS * ROWS },
    >
);

impl_led12x4_spawn!(
    led12x4_device_loop_led_strip_simple_pio1,
    crate::led_strip_simple::LedStripSimple<
        'static,
        embassy_rp::peripherals::PIO1,
        { COLS * ROWS },
    >
);

#[cfg(feature = "pico2")]
impl_led12x4_spawn!(
    led12x4_device_loop_led_strip_simple_pio2,
    crate::led_strip_simple::LedStripSimple<
        'static,
        embassy_rp::peripherals::PIO2,
        { COLS * ROWS },
    >
);

impl_led12x4_spawn!(led12x4_device_loop_led_strip, LedStrip<{ COLS * ROWS }>);
