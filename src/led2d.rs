//! A device abstraction for rectangular LED matrix displays with arbitrary dimensions.
//!
//! See [`Led2d`] for usage details.

// Re-export for macro use
// cmk000 this appears in the docs? should it? If not, hide it. If yes, add a documation line.
pub use paste;

use core::convert::Infallible;
use embassy_futures::select::{Either, select};
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, signal::Signal};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    draw_target::DrawTarget,
    mono_font::{
        DecorationDimensions, MonoFont,
        ascii::{
            FONT_4X6, FONT_5X7, FONT_5X8, FONT_6X9, FONT_6X10, FONT_6X12, FONT_6X13,
            FONT_6X13_BOLD, FONT_6X13_ITALIC, FONT_7X13, FONT_7X13_BOLD, FONT_7X13_ITALIC,
            FONT_7X14, FONT_7X14_BOLD, FONT_8X13, FONT_8X13_BOLD, FONT_8X13_ITALIC, FONT_9X15,
            FONT_9X15_BOLD, FONT_9X18, FONT_9X18_BOLD, FONT_10X20,
        },
        mapping::StrGlyphMapping,
    },
    pixelcolor::Rgb888,
    prelude::*,
};
use heapless::Vec;
use smart_leds::RGB8;

use crate::Result;

/// Convert RGB8 (smart-leds) to Rgb888 (embedded-graphics).
#[must_use]
pub const fn rgb8_to_rgb888(color: RGB8) -> Rgb888 {
    Rgb888::new(color.r, color.g, color.b)
}

/// Convert Rgb888 (embedded-graphics) to RGB8 (smart-leds).
#[must_use]
pub fn rgb888_to_rgb8(color: Rgb888) -> RGB8 {
    RGB8::new(color.r(), color.g(), color.b())
}

// cmk does this need to be limited and public
/// Maximum frames supported by [`Led2d::animate`].
pub const ANIMATION_MAX_FRAMES: usize = 32;

// Packed bitmap for the internal 3x4 font (ASCII 0x20-0x7E).
const BIT_MATRIX3X4_FONT_DATA: [u8; 144] = [
    0x0a, 0xd5, 0x10, 0x4a, 0xa0, 0x01, 0x0a, 0xfe, 0x68, 0x85, 0x70, 0x02, 0x08, 0x74, 0x90, 0x86,
    0xa5, 0xc4, 0x08, 0x5e, 0x68, 0x48, 0x08, 0x10, 0xeb, 0x7b, 0xe7, 0xfd, 0x22, 0x27, 0xb8, 0x9b,
    0x39, 0xb4, 0x05, 0xd1, 0xa9, 0x3e, 0xea, 0x5d, 0x28, 0x0a, 0xff, 0xf3, 0xfc, 0xe4, 0x45, 0xd2,
    0xff, 0x7d, 0xff, 0xbc, 0xd9, 0xff, 0xb7, 0xcb, 0xb4, 0xe8, 0xe9, 0xfd, 0xfe, 0xcb, 0x25, 0xaa,
    0xd9, 0x7d, 0x97, 0x7d, 0xe7, 0xbf, 0xdf, 0x6f, 0xdf, 0x7f, 0x6d, 0xb7, 0xe0, 0xd0, 0xf7, 0xe5,
    0x6d, 0x48, 0xc0, 0x68, 0xdf, 0x35, 0x6f, 0x49, 0x40, 0x40, 0x86, 0xf5, 0xd7, 0xab, 0xe0, 0xc7,
    0x5f, 0x7d, 0xff, 0xbc, 0xd9, 0xff, 0x37, 0xcb, 0xb4, 0xe8, 0xe9, 0xfd, 0x1e, 0xcb, 0x25, 0xaa,
    0xd9, 0x7d, 0x17, 0x7d, 0xe7, 0xbf, 0xdf, 0x6f, 0xdf, 0x7f, 0x6d, 0xb7, 0xb1, 0x80, 0xf7, 0xe5,
    0x6d, 0x48, 0xa0, 0xa8, 0xdf, 0x35, 0x6f, 0x49, 0x20, 0x90, 0x86, 0xf5, 0xd7, 0xab, 0xb1, 0x80,
];
const BIT_MATRIX3X4_IMAGE_WIDTH: u32 = 48;
const BIT_MATRIX3X4_GLYPH_MAPPING: StrGlyphMapping<'static> = StrGlyphMapping::new("\0 \u{7e}", 0);

#[doc(hidden)]
/// Monospace 3x4 font matching `bit_matrix3x4`.
#[must_use]
pub fn bit_matrix3x4_font() -> MonoFont<'static> {
    MonoFont {
        image: embedded_graphics::image::ImageRaw::new(
            &BIT_MATRIX3X4_FONT_DATA,
            BIT_MATRIX3X4_IMAGE_WIDTH,
        ),
        glyph_mapping: &BIT_MATRIX3X4_GLYPH_MAPPING,
        character_size: embedded_graphics::prelude::Size::new(3, 4),
        character_spacing: 0,
        baseline: 3,
        underline: DecorationDimensions::new(3, 1),
        strikethrough: DecorationDimensions::new(2, 1),
    }
}

#[doc(hidden)]
/// Render text into a frame using the provided font.
pub fn render_text_to_frame<const ROWS: usize, const COLS: usize>(
    frame: &mut Frame<ROWS, COLS>,
    font: &embedded_graphics::mono_font::MonoFont<'static>,
    text: &str,
    colors: &[RGB8],
) -> Result<()> {
    let glyph_width = font.character_size.width as i32;
    let glyph_height = font.character_size.height as i32;
    let advance_x = glyph_width;
    let advance_y = glyph_height;
    let height_limit = ROWS as i32;
    if height_limit <= 0 {
        return Ok(());
    }
    let baseline = font.baseline as i32;
    let width_limit = COLS as i32;
    let mut x = 0i32;
    let mut y = baseline;
    let mut color_index: usize = 0;

    for ch in text.chars() {
        if ch == '\n' {
            x = 0;
            y += advance_y;
            if y - baseline >= height_limit {
                break;
            }
            continue;
        }

        // Clip characters that exceed width limit (no wrapping until explicit \n)
        if x + glyph_width > width_limit {
            continue;
        }

        let color = if colors.is_empty() {
            smart_leds::colors::WHITE
        } else {
            colors[color_index % colors.len()]
        };
        color_index = color_index.wrapping_add(1);

        let mut buf = [0u8; 4];
        let slice = ch.encode_utf8(&mut buf);
        let style = embedded_graphics::mono_font::MonoTextStyle::new(font, rgb8_to_rgb888(color));
        let position = embedded_graphics::prelude::Point::new(x, y);
        embedded_graphics::Drawable::draw(
            &embedded_graphics::text::Text::new(slice, position, style),
            frame,
        )
        .expect("drawing into frame cannot fail");

        x += advance_x;
    }

    Ok(())
}

// cmk000 this description is bad
/// Built-in 3x4 font and embedded-graphics ASCII fonts.
#[derive(Clone, Copy, Debug)]
pub enum Led2dFont {
    Font3x4,
    Font4x6,
    Font5x7,
    Font5x8,
    Font6x9,
    Font6x10,
    Font6x12,
    Font6x13,
    Font6x13Bold,
    Font6x13Italic,
    Font7x13,
    Font7x13Bold,
    Font7x13Italic,
    Font7x14,
    Font7x14Bold,
    Font8x13,
    Font8x13Bold,
    Font8x13Italic,
    Font9x15,
    Font9x15Bold,
    Font9x18,
    Font9x18Bold,
    Font10x20,
}

impl Led2dFont {
    /// Return the `MonoFont` for this variant.
    #[must_use]
    pub fn to_font(self) -> MonoFont<'static> {
        match self {
            Self::Font3x4 => bit_matrix3x4_font(),
            Self::Font4x6 => FONT_4X6,
            Self::Font5x7 => FONT_5X7,
            Self::Font5x8 => FONT_5X8,
            Self::Font6x9 => FONT_6X9,
            Self::Font6x10 => FONT_6X10,
            Self::Font6x12 => FONT_6X12,
            Self::Font6x13 => FONT_6X13,
            Self::Font6x13Bold => FONT_6X13_BOLD,
            Self::Font6x13Italic => FONT_6X13_ITALIC,
            Self::Font7x13 => FONT_7X13,
            Self::Font7x13Bold => FONT_7X13_BOLD,
            Self::Font7x13Italic => FONT_7X13_ITALIC,
            Self::Font7x14 => FONT_7X14,
            Self::Font7x14Bold => FONT_7X14_BOLD,
            Self::Font8x13 => FONT_8X13,
            Self::Font8x13Bold => FONT_8X13_BOLD,
            Self::Font8x13Italic => FONT_8X13_ITALIC,
            Self::Font9x15 => FONT_9X15,
            Self::Font9x15Bold => FONT_9X15_BOLD,
            Self::Font9x18 => FONT_9X18,
            Self::Font9x18Bold => FONT_9X18_BOLD,
            Self::Font10x20 => FONT_10X20,
        }
    }
}

// cmk0 should also define Default via the trait
/// Pixel frame for LED matrix displays.
///
/// Wraps a 2D array of RGB pixels with support for indexing and embedded-graphics.
#[derive(Clone, Copy, Debug)]
pub struct Frame<const ROWS: usize, const COLS: usize>(pub [[RGB8; COLS]; ROWS]);

impl<const ROWS: usize, const COLS: usize> Frame<ROWS, COLS> {
    /// Create a new blank (all black) frame.
    #[must_use]
    pub const fn new() -> Self {
        Self([[RGB8::new(0, 0, 0); COLS]; ROWS])
    }

    /// Create a frame filled with a single color.
    #[must_use]
    pub const fn filled(color: RGB8) -> Self {
        Self([[color; COLS]; ROWS])
    }

    /// Get the frame dimensions.
    #[must_use]
    pub const fn size() -> Size {
        Size::new(COLS as u32, ROWS as u32)
    }

    /// Get the top-left corner point.
    #[must_use]
    pub const fn top_left() -> Point {
        Point::new(0, 0)
    }

    /// Get the top-right corner point.
    #[must_use]
    pub const fn top_right() -> Point {
        Point::new((COLS - 1) as i32, 0)
    }

    /// Get the bottom-left corner point.
    #[must_use]
    pub const fn bottom_left() -> Point {
        Point::new(0, (ROWS - 1) as i32)
    }

    /// Get the bottom-right corner point.
    #[must_use]
    pub const fn bottom_right() -> Point {
        Point::new((COLS - 1) as i32, (ROWS - 1) as i32)
    }
}

impl<const ROWS: usize, const COLS: usize> core::ops::Deref for Frame<ROWS, COLS> {
    type Target = [[RGB8; COLS]; ROWS];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const ROWS: usize, const COLS: usize> core::ops::DerefMut for Frame<ROWS, COLS> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const ROWS: usize, const COLS: usize> From<[[RGB8; COLS]; ROWS]> for Frame<ROWS, COLS> {
    fn from(array: [[RGB8; COLS]; ROWS]) -> Self {
        Self(array)
    }
}

impl<const ROWS: usize, const COLS: usize> From<Frame<ROWS, COLS>> for [[RGB8; COLS]; ROWS] {
    fn from(frame: Frame<ROWS, COLS>) -> Self {
        frame.0
    }
}

impl<const ROWS: usize, const COLS: usize> Default for Frame<ROWS, COLS> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const ROWS: usize, const COLS: usize> OriginDimensions for Frame<ROWS, COLS> {
    fn size(&self) -> Size {
        Size::new(COLS as u32, ROWS as u32)
    }
}

impl<const ROWS: usize, const COLS: usize> DrawTarget for Frame<ROWS, COLS> {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> core::result::Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(coord, color) in pixels {
            let column_index = coord.x;
            let row_index = coord.y;
            if column_index >= 0
                && column_index < COLS as i32
                && row_index >= 0
                && row_index < ROWS as i32
            {
                self.0[row_index as usize][column_index as usize] =
                    RGB8::new(color.r(), color.g(), color.b());
            }
        }
        Ok(())
    }
}

// cmk000 should not be public and visable to the docs, right?
pub type Led2dCommandSignal<const N: usize> = Signal<CriticalSectionRawMutex, Command<N>>;
// cmk000 should not be public and visable to the docs, right?
pub type Led2dCompletionSignal = Signal<CriticalSectionRawMutex, ()>;

// cmk000 this should not be public nor appear in the docs
/// Command for the LED device loop.
#[derive(Clone)]
pub enum Command<const N: usize> {
    DisplayStatic([RGB8; N]),
    Animate(Vec<([RGB8; N], Duration), ANIMATION_MAX_FRAMES>),
}

// cmk000 bad description. better something like: Static type for the Led2d device abstraction.
/// Signal resources for [`Led2d`].
pub struct Led2dStatic<const N: usize> {
    pub command_signal: Led2dCommandSignal<N>,
    pub completion_signal: Led2dCompletionSignal,
}

impl<const N: usize> Led2dStatic<N> {
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            command_signal: Signal::new(),
            completion_signal: Signal::new(),
        }
    }
}

// cmk think about if this should be public
/// Trait for LED strip drivers that can render a full frame.
pub trait LedStrip<const N: usize> {
    /// Update all pixels at once.
    async fn update_pixels(&mut self, pixels: &[RGB8; N]) -> Result<()>;
}

// cmk000 by the rules of Rust style, should this be Led2D?
// cmk000 this needs a compiled-only doc test.
/// A device abstraction for rectangular LED matrix displays.
///
/// Supports any size display with arbitrary coordinate-to-LED-index mapping.
/// The mapping is stored as a runtime slice, allowing stable Rust without experimental features.
///
/// Rows and columns are metadata used only for indexing - the core type is generic only over N (total LEDs).
pub struct Led2d<'a, const N: usize> {
    command_signal: &'static Led2dCommandSignal<N>,
    completion_signal: &'static Led2dCompletionSignal,
    mapping: &'a [u16],
    cols: usize,
}

impl<'a, const N: usize> Led2d<'a, N> {
    /// Create Led2d device handle.
    ///
    /// The `mapping` slice defines how (column, row) coordinates map to LED strip indices.
    /// Index `row * cols + col` gives the LED index for that position.
    /// Length must equal N (checked with debug_assert).
    #[must_use]
    pub fn new(led2d_static: &'static Led2dStatic<N>, mapping: &'a [u16], cols: usize) -> Self {
        debug_assert_eq!(mapping.len(), N, "mapping length must equal N (total LEDs)");
        Self {
            command_signal: &led2d_static.command_signal,
            completion_signal: &led2d_static.completion_signal,
            mapping,
            cols,
        }
    }

    /// Convert (column, row) coordinates to LED strip index using the stored mapping.
    #[must_use]
    fn xy_to_index(&self, column_index: usize, row_index: usize) -> usize {
        self.mapping[row_index * self.cols + column_index] as usize
    }

    /// Convert 2D frame to 1D array using the mapping.
    fn convert_frame<const ROWS: usize, const COLS: usize>(
        &self,
        frame_2d: Frame<ROWS, COLS>,
    ) -> [RGB8; N] {
        let mut frame_1d = [RGB8::new(0, 0, 0); N];
        for row_index in 0..ROWS {
            for column_index in 0..COLS {
                let led_index = self.xy_to_index(column_index, row_index);
                frame_1d[led_index] = frame_2d[row_index][column_index];
            }
        }
        frame_1d
    }

    /// Render a fully defined frame to the display.
    ///
    /// Frame is a 2D array in row-major order where `frame[row][col]` is the pixel at (col, row).
    pub async fn write_frame<const ROWS: usize, const COLS: usize>(
        &self,
        frame: Frame<ROWS, COLS>,
    ) -> Result<()> {
        defmt::info!("Led2d::write_frame: sending DisplayStatic command");
        let frame_1d = self.convert_frame(frame);
        self.command_signal.signal(Command::DisplayStatic(frame_1d));
        defmt::info!("Led2d::write_frame: waiting for completion");
        self.completion_signal.wait().await;
        defmt::info!("Led2d::write_frame: completed");
        Ok(())
    }

    /// Loop through a sequence of animation frames until interrupted by another command.
    ///
    /// Each frame is a tuple of (Frame, duration).
    pub async fn animate<const ROWS: usize, const COLS: usize>(
        &self,
        frames: &[(Frame<ROWS, COLS>, Duration)],
    ) -> Result<()> {
        assert!(!frames.is_empty(), "animation requires at least one frame");
        defmt::info!("Led2d::animate: preparing {} frames", frames.len());
        let mut sequence: Vec<([RGB8; N], Duration), ANIMATION_MAX_FRAMES> = Vec::new();
        for (frame, duration) in frames {
            assert!(
                duration.as_micros() > 0,
                "animation frame duration must be positive"
            );
            let frame_1d = self.convert_frame(*frame);
            sequence
                .push((frame_1d, *duration))
                .expect("animation sequence fits");
        }
        defmt::info!("Led2d::animate: sending Animate command");
        self.command_signal.signal(Command::Animate(sequence));
        defmt::info!("Led2d::animate: waiting for completion");
        self.completion_signal.wait().await;
        defmt::info!("Led2d::animate: completed (animation started)");
        Ok(())
    }
}

/// Creates a serpentine column-major mapping for rectangular displays.
///
/// Even columns go top-to-bottom (row 0→ROWS-1), odd columns go bottom-to-top (row ROWS-1→0).
/// This matches typical WS2812 LED strip wiring patterns.
///
/// Returns a flat array where index `row * COLS + col` gives the LED index for that position.
#[must_use]
#[doc(hidden)]
pub const fn serpentine_column_major_mapping<
    const N: usize,
    const ROWS: usize,
    const COLS: usize,
>() -> [u16; N] {
    let mut mapping = [0_u16; N];
    let mut row_index = 0;
    while row_index < ROWS {
        let mut column_index = 0;
        while column_index < COLS {
            let led_index = if column_index % 2 == 0 {
                // Even column: top-to-bottom
                column_index * ROWS + row_index
            } else {
                // Odd column: bottom-to-top
                column_index * ROWS + (ROWS - 1 - row_index)
            };
            mapping[row_index * COLS + column_index] = led_index as u16;
            column_index += 1;
        }
        row_index += 1;
    }
    mapping
}

#[doc(hidden)]
/// Device loop for Led2d. This is exported so users can create their own task wrappers.
///
/// Since embassy tasks cannot be generic, users must create a concrete wrapper task.
/// Example usage in `led12x4.rs`.
pub async fn led2d_device_loop<const N: usize, S: LedStrip<N>>(
    command_signal: &'static Led2dCommandSignal<N>,
    completion_signal: &'static Led2dCompletionSignal,
    mut strip: S,
) -> Result<Infallible> {
    defmt::info!("led2d_device_loop: task started");
    loop {
        defmt::debug!("led2d_device_loop: waiting for command");
        let command = command_signal.wait().await;
        command_signal.reset();

        match command {
            Command::DisplayStatic(frame) => {
                defmt::info!("led2d_device_loop: received DisplayStatic command");
                strip.update_pixels(&frame).await?;
                completion_signal.signal(());
                defmt::info!("led2d_device_loop: DisplayStatic completed");
            }
            Command::Animate(frames) => {
                defmt::info!(
                    "led2d_device_loop: received Animate command with {} frames",
                    frames.len()
                );
                let next_command =
                    run_animation_loop(frames, command_signal, completion_signal, &mut strip)
                        .await?;
                defmt::info!("led2d_device_loop: animation interrupted");
                match next_command {
                    Command::DisplayStatic(frame) => {
                        defmt::info!(
                            "led2d_device_loop: processing DisplayStatic from animation interrupt"
                        );
                        strip.update_pixels(&frame).await?;
                        completion_signal.signal(());
                    }
                    Command::Animate(new_frames) => {
                        defmt::info!("led2d_device_loop: restarting with new animation");
                        // Process the new animation immediately without waiting for next command
                        let next_command = run_animation_loop(
                            new_frames,
                            command_signal,
                            completion_signal,
                            &mut strip,
                        )
                        .await?;
                        // Handle any command that interrupted this animation
                        match next_command {
                            Command::DisplayStatic(frame) => {
                                strip.update_pixels(&frame).await?;
                                completion_signal.signal(());
                            }
                            Command::Animate(_) => {
                                // Another animation interrupted; loop back to handle it
                                continue;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn run_animation_loop<const N: usize, S: LedStrip<N>>(
    frames: Vec<([RGB8; N], Duration), ANIMATION_MAX_FRAMES>,
    command_signal: &'static Led2dCommandSignal<N>,
    completion_signal: &'static Led2dCompletionSignal,
    strip: &mut S,
) -> Result<Command<N>> {
    defmt::info!("run_animation_loop: starting with {} frames", frames.len());
    completion_signal.signal(());
    defmt::debug!("run_animation_loop: signaled completion (animation started)");

    loop {
        for (frame_index, (pixels, duration)) in frames.iter().enumerate() {
            defmt::trace!("run_animation_loop: displaying frame {}", frame_index);
            strip.update_pixels(pixels).await?;

            match select(command_signal.wait(), Timer::after(*duration)).await {
                Either::First(new_command) => {
                    defmt::info!("run_animation_loop: received new command, interrupting");
                    command_signal.reset();
                    return Ok(new_command);
                }
                Either::Second(()) => continue,
            }
        }
        defmt::debug!("run_animation_loop: completed one loop, restarting");
    }
}

#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "host"))]
// cmk000 this appears in the docs? should it? If not, hide it
macro_rules! led2d_device_task {
    (
        $task_name:ident,
        $strip_ty:ty,
        $n:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!(
            @inner
            ()
            $task_name,
            $strip_ty,
            $n
        );
    };
    (
        $vis:vis $task_name:ident,
        $strip_ty:ty,
        $n:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!(
            @inner
            ($vis)
            $task_name,
            $strip_ty,
            $n
        );
    };
    (
        @inner
        ($($vis:tt)*)
        $task_name:ident,
        $strip_ty:ty,
        $n:expr $(,)?
    ) => {
        #[embassy_executor::task]
        $($vis)* async fn $task_name(
            command_signal: &'static $crate::led2d::Led2dCommandSignal<$n>,
            completion_signal: &'static $crate::led2d::Led2dCompletionSignal,
            strip: $strip_ty,
        ) {
            let err = $crate::led2d::led2d_device_loop(command_signal, completion_signal, strip)
                .await
                .unwrap_err();
            panic!("{err}");
        }
    };
}

/// Declares an Embassy task that runs [`led2d_device_loop`] for a concrete LED strip type.
///
/// Each `Led2d` device needs a monomorphic task because `#[embassy_executor::task]` does not
/// support generics. This macro generates the boilerplate wrapper and keeps your modules tidy.
///
/// # Example
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use embassy_executor::Spawner;
/// use embassy_rp::{init, peripherals::PIO1};
/// use serials::Result;
/// use serials::led2d::{Led2dStatic, led2d_device_task};
/// use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, Milliamps};
///
/// const COLS: usize = 12;
/// const ROWS: usize = 4;
/// const N: usize = COLS * ROWS;
/// #
/// # #[embassy_executor::main]
/// # async fn main(_spawner: Spawner) { loop {} }
/// ```
#[cfg(not(feature = "host"))]
#[doc(inline)]
pub use led2d_device_task;

#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "host"))]
// cmk000 this appears in the docs? should it? If not, hide it
macro_rules! led2d_device {
    (
        $vis:vis struct $resources_name:ident,
        task: $task_vis:vis $task_name:ident,
        strip: $strip_ty:ty,
        leds: $n:expr,
        mapping: $mapping:expr,
        cols: $cols:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!($task_vis $task_name, $strip_ty, $n);

        $vis struct $resources_name {
            led2d_static: $crate::led2d::Led2dStatic<$n>,
        }

        impl $resources_name {
            /// Create the static resources for this Led2d instance.
            #[must_use]
            pub const fn new_static() -> Self {
                Self {
                    led2d_static: $crate::led2d::Led2dStatic::new_static(),
                }
            }

            /// Construct the `Led2d` handle, spawning the background task automatically.
            pub fn new(
                &'static self,
                strip: $strip_ty,
                spawner: ::embassy_executor::Spawner,
            ) -> $crate::Result<$crate::led2d::Led2d<'static, $n>> {
                let token = $task_name(
                    &self.led2d_static.command_signal,
                    &self.led2d_static.completion_signal,
                    strip,
                )?;
                spawner.spawn(token);
                Ok($crate::led2d::Led2d::new(
                    &self.led2d_static,
                    $mapping,
                    $cols,
                ))
            }
        }
    };
}

/// Declares the full Led2d device/static pair plus the background task wrapper.
///
/// This extends [`led2d_device_task!`] by also generating a static resource holder with
/// `new_static`/`new` so callers do not need to wire up the signals and task spawning manually.
///
/// # Example
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use defmt::info;
/// use embassy_executor::Spawner;
/// use embassy_rp::{init, peripherals::PIO1};
/// use serials::Result;
/// use serials::led2d::{Led2d, led2d_device};
/// use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, Milliamps};
///
/// const COLS: usize = 12;
/// const ROWS: usize = 4;
/// const N: usize = COLS * ROWS;
/// const MAPPING: [u16; N] = serials::led2d::serpentine_column_major_mapping::<N, ROWS, COLS>();
/// #
/// # #[embassy_executor::main]
/// # async fn main(_spawner: Spawner) { loop {} }
#[cfg(not(feature = "host"))]
#[doc(inline)]
pub use led2d_device;

#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "host"))]
macro_rules! led2d_device_simple {
    // Serpentine column-major mapping variant
    (
        $vis:vis $name:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        pio: $pio:ident,
        mapping: serpentine_column_major,
        font: $font_variant:expr $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            const [<$name:upper _ROWS>]: usize = $rows;
            const [<$name:upper _COLS>]: usize = $cols;
            const [<$name:upper _N>]: usize = [<$name:upper _ROWS>] * [<$name:upper _COLS>];
            const [<$name:upper _MAPPING>]: [u16; [<$name:upper _N>]] = $crate::led2d::serpentine_column_major_mapping::<[<$name:upper _N>], [<$name:upper _ROWS>], [<$name:upper _COLS>]>();

            $crate::led2d::led2d_device_simple!(
                @common $vis, $name, $pio, [<$name:upper _ROWS>], [<$name:upper _COLS>], [<$name:upper _N>], [<$name:upper _MAPPING>],
                $font_variant
            );
        }
    };
    // Arbitrary custom mapping variant
    (
        $vis:vis $name:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        pio: $pio:ident,
        mapping: arbitrary([$($index:expr),* $(,)?]),
        font: $font_variant:expr $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            const [<$name:upper _ROWS>]: usize = $rows;
            const [<$name:upper _COLS>]: usize = $cols;
            const [<$name:upper _N>]: usize = [<$name:upper _ROWS>] * [<$name:upper _COLS>];
            const [<$name:upper _MAPPING>]: [u16; [<$name:upper _N>]] = [$($index),*];

            $crate::led2d::led2d_device_simple!(
                @common $vis, $name, $pio, [<$name:upper _ROWS>], [<$name:upper _COLS>], [<$name:upper _N>], [<$name:upper _MAPPING>],
                $font_variant
            );
        }
    };
    // Common implementation (shared by both variants)
    (
        @common $vis:vis,
        $name:ident,
        $pio:ident,
        $rows_const:ident,
        $cols_const:ident,
        $n_const:ident,
        $mapping_const:ident,
        $font_variant:expr
    ) => {
        $crate::led2d::paste::paste! {
            /// Static resources for the device.
            $vis struct [<$name:camel Static>] {
                led_strip_simple: $crate::led_strip_simple::LedStripSimpleStatic<$n_const>,
                led2d_static: $crate::led2d::Led2dStatic<$n_const>,
            }

            // Generate the task wrapper
            $crate::led2d::led2d_device_task!(
                [<$name _device_loop>],
                $crate::led_strip_simple::LedStripSimple<'static, ::embassy_rp::peripherals::$pio, $n_const>,
                $n_const
            );

            /// Device abstraction for the LED matrix.
            $vis struct [<$name:camel>] {
                pub led2d: $crate::led2d::Led2d<'static, $n_const>,
                pub font: embedded_graphics::mono_font::MonoFont<'static>,
            }

            impl [<$name:camel>] {
                /// Number of rows in the display.
                pub const ROWS: usize = $rows_const;
                /// Number of columns in the display.
                pub const COLS: usize = $cols_const;
                /// Total number of LEDs (ROWS * COLS).
                pub const N: usize = $n_const;

                /// Create static resources.
                #[must_use]
                $vis const fn new_static() -> [<$name:camel Static>] {
                    [<$name:camel Static>] {
                        led_strip_simple: $crate::led_strip_simple::LedStripSimpleStatic::new_static(),
                        led2d_static: $crate::led2d::Led2dStatic::new_static(),
                    }
                }

                /// Create a new blank (all black) frame.
                #[must_use]
                $vis const fn new_frame() -> $crate::led2d::Frame<$rows_const, $cols_const> {
                    $crate::led2d::Frame::new()
                }

                /// Create the device, spawning the background task.
                ///
                /// # Parameters
                /// - `static_resources`: Static resources created with `new_static()`
                /// - `pio`: PIO peripheral
                /// - `pin`: GPIO pin for LED data
                /// - `max_current`: Maximum current budget
                /// - `spawner`: Task spawner
                $vis async fn new(
                    static_resources: &'static [<$name:camel Static>],
                    pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
                    pin: ::embassy_rp::Peri<'static, impl ::embassy_rp::pio::PioPin>,
                    max_current: $crate::led_strip_simple::Milliamps,
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<Self> {
                    defmt::info!("Led2d::new: creating LED strip");
                    let strip = $crate::led_strip_simple::LedStripSimple::[<new_ $pio:lower>](
                        &static_resources.led_strip_simple,
                        pio,
                        pin,
                        max_current,
                    )
                    .await;

                    defmt::info!("Led2d::new: strip created, spawning device task");
                    let token = [<$name _device_loop>](
                        &static_resources.led2d_static.command_signal,
                        &static_resources.led2d_static.completion_signal,
                        strip,
                    )?;
                    spawner.spawn(token);
                    defmt::info!("Led2d::new: device task spawned");

                    let led2d = $crate::led2d::Led2d::new(
                        &static_resources.led2d_static,
                        &$mapping_const,
                        $cols_const,
                    );

                    defmt::info!("Led2d::new: device created successfully");
                    Ok(Self {
                        led2d,
                        font: ($font_variant).to_font(),
                    })
                }

                /// Render a fully defined frame to the display.
                ///
                /// Frame is a 2D array in row-major order where `frame[row][col]` is the pixel at (col, row).
                $vis async fn write_frame(&self, frame: $crate::led2d::Frame<$rows_const, $cols_const>) -> $crate::Result<()> {
                    self.led2d.write_frame(frame).await
                }

                /// Loop through a sequence of animation frames.
                ///
                /// Each frame is a tuple of (Frame, duration).
                $vis async fn animate(&self, frames: &[($crate::led2d::Frame<$rows_const, $cols_const>, ::embassy_time::Duration)]) -> $crate::Result<()> {
                    self.led2d.animate(frames).await
                }

                /// Render text into a frame using the configured font and spacing.
                ///
                /// - `text`: text to render; `\n` starts a new line.
                /// - `colors`: cycle of colors for non-newline characters; if empty, white is used.
                /// - `frame`: target frame to draw into.
                pub fn write_text_to_frame(
                    &self,
                    text: &str,
                    colors: &[smart_leds::RGB8],
                    frame: &mut $crate::led2d::Frame<$rows_const, $cols_const>,
                ) -> $crate::Result<()> {
                    $crate::led2d::render_text_to_frame(frame, &self.font, text, colors)
                }

                /// Convenience wrapper to render text into a fresh frame and display it.
                pub async fn write_text(&self, text: &str, colors: &[smart_leds::RGB8]) -> $crate::Result<()> {
                    let mut frame = Self::new_frame();
                    self.write_text_to_frame(text, colors, &mut frame)?;
                    self.write_frame(frame).await
                }
            }
        }
    };
}

/// Declares a complete Led2d device abstraction with LedStripSimple integration.
///
/// This macro generates all the boilerplate for a rectangular LED matrix device:
/// - Constants: ROWS, COLS, N (total LEDs)
/// - Mapping array (serpentine column-major or custom)
/// - Static struct with embedded LedStripSimple resources
/// - Device struct with Led2d handle
/// - Constructor that creates strip + spawns task
/// - Wrapper methods: write_frame, animate, xy_to_index
///
/// # Mapping Variants
///
/// - `serpentine_column_major`: Built-in serpentine column-major wiring pattern
/// - `arbitrary([...])`: Custom mapping array (must have exactly N elements)
///
/// # Examples
///
/// With serpentine mapping:
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use serials::led2d::led2d_device_simple;
///
/// led2d_device_simple! {
///     pub led12x4,
///     rows: 4,
///     cols: 12,
///     pio: PIO1,
///     mapping: serpentine_column_major,
///     font: serials::led2d::Led2dFont::Font3x4,
/// }
/// # use embassy_executor::Spawner;
/// # #[embassy_executor::main]
/// # async fn main(_spawner: Spawner) { loop {} }
/// ```
///
/// With custom mapping:
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use serials::led2d::led2d_device_simple;
///
/// led2d_device_simple! {
///     pub led4x4,
///     rows: 4,
///     cols: 4,
///     pio: PIO0,
///     mapping: arbitrary([
///         0, 1, 2, 3,
///         4, 5, 6, 7,
///         8, 9, 10, 11,
///         12, 13, 14, 15
///     ]),
///     font: serials::led2d::Led2dFont::Font3x4,
/// }
/// # use embassy_executor::Spawner;
/// # #[embassy_executor::main]
/// # async fn main(_spawner: Spawner) { loop {} }
/// ```
#[cfg(not(feature = "host"))]
#[doc(inline)]
pub use led2d_device_simple;
