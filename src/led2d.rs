//! A device abstraction for rectangular WS2812-style LED matrix displays with arbitrary size.
//!
//! Supports text rendering, animation, and full graphics capabilities. For simple
//! single-strip displays, use the `led2d!` macro. For multi-strip scenarios
//! where you need to share a PIO with other devices, use `led2d_from_strip!` with
//! [`define_led_strips_shared!`](crate::led_strip::define_led_strips_shared).
//!
//! For custom graphics, create a [`Frame`] and use the
//! [`embedded-graphics`](https://docs.rs/embedded-graphics) drawing API. See the
//! [`Frame`] documentation for an example.
//!
//! # Quick Start with `led2d!`
//!
//! The simplest way to create an LED matrix display:
//!
//! ```no_run
//! # #![no_std]
//! # #![no_main]
//! # use panic_probe as _;
//! use embassy_executor::Spawner;
//! use embassy_rp::init;
//! use device_kit::led2d;
//! use device_kit::led_strip::Milliamps;
//! use device_kit::led_strip::gamma::Gamma;
//! use device_kit::led_strip::colors;
//!
//! led2d! {
//!     pub led12x4,
//!     pio: PIO0,
//!     pin: PIN_3,
//!     dma: DMA_CH1,
//!     rows: 4,
//!     cols: 12,
//!     mapping: serpentine_column_major,
//!     max_current: Milliamps(500),
//!     gamma: Gamma::Linear,
//!     max_frames: 32,
//!     font: Font3x4Trim,
//! }
//!
//! #[embassy_executor::main]
//! async fn main(spawner: Spawner) {
//!     let p = init(Default::default());
//!     let led = Led12x4::new(p.PIO0, p.DMA_CH1, p.PIN_3, spawner).unwrap();
//!     led.write_text("HI", &[colors::RED]).await.unwrap();
//! }
//! ```
//!
//! # Advanced: Multi-Strip with `led2d_from_strip!`
//!
//! When sharing a PIO with multiple LED strips, use `define_led_strips_shared!` and
//! `led2d_from_strip!` together. The macro generates a type-safe device abstraction
//! with text rendering, animation, and graphics support.
//!
//! ## Macro Parameters
//!
//! - Visibility and base name for generated types (e.g., `pub led12x4`)
//! - `strip_type` - Name of the strip type created by `define_led_strips_shared!` (e.g., `Led12x4Strip`)
//! - `rows` - Number of rows in the display
//! - `cols` - Number of columns in the display
//! - `mapping` - LED strip physical layout:
//!   - `serpentine_column_major` - Common serpentine wiring pattern
//!   - `arbitrary([indices...])` - Custom mapping array
//! - `max_frames` - Maximum animation frames allowed (not buffered)
//! - `font` - Built-in font variant (see [`Led2dFont`])
//!
//! ## Generated API
//!
//! The macro generates:
//! - `YourNameStatic` - Static resources (create with `YourName::new_static()`)
//! - `YourName` - Device handle with methods for text, animation, and graphics
//!
//! # Example
//!
//! ```no_run
//! # #![no_std]
//! # #![no_main]
//! # use panic_probe as _;
//! use embassy_executor::Spawner;
//! use embassy_rp::init;
//! use device_kit::led_strip::define_led_strips_shared;
//! use device_kit::led2d::led2d_from_strip;
//! use device_kit::led_strip::Milliamps;
//! use device_kit::led_strip::gamma::Gamma;
//! use device_kit::led_strip::colors;
//! use device_kit::pio_split;
//!
//! // Define LED strip sharing PIO1
//! define_led_strips_shared! {
//!     pio: PIO1,
//!     strips: [
//!         Led12x4Strip {
//!             sm: 0,
//!             dma: DMA_CH0,
//!             pin: PIN_3,
//!             len: 48,
//!             max_current: Milliamps(500),
//!             gamma: Gamma::Linear
//!         }
//!     ]
//! }
//!
//! // Generate a complete LED matrix device abstraction
//! led2d_from_strip! {
//!     pub led12x4,
//!     strip_type: Led12x4Strip,
//!     rows: 4,
//!     cols: 12,
//!     mapping: serpentine_column_major,
//!     max_frames: 32,
//!     font: Font3x4Trim,
//! }
//!
//! #[embassy_executor::main]
//! async fn main(spawner: Spawner) {
//!     let p = init(Default::default());
//!
//!     // Split PIO and create strip
//!     let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO1);
//!     let strip = Led12x4Strip::new(sm0, p.DMA_CH0, p.PIN_3, spawner).unwrap();
//!
//!     // Create Led2d device from strip
//!     let led_12x4 = Led12x4::from_strip(strip, spawner).unwrap();
//!
//!     // Display colorful text
//!     led_12x4.write_text("HI!", &[colors::CYAN, colors::MAGENTA, colors::YELLOW])
//!         .await
//!         .unwrap();
//!
//!     loop {}
//! }
//! ```

// Re-export for macro use
#[doc(hidden)]
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
    pixelcolor::Rgb888, // cmk should this just be color?
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
    spacing_reduction: (i32, i32),
) -> Result<()> {
    let glyph_width = font.character_size.width as i32;
    let glyph_height = font.character_size.height as i32;
    let advance_x = glyph_width - spacing_reduction.0;
    let advance_y = glyph_height - spacing_reduction.1;
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
        if x + advance_x > width_limit {
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

/// Font options for [`Led2d`] text rendering.
///
/// Fonts with `Trim` suffix remove blank spacing to pack text more tightly on small displays.
#[derive(Clone, Copy, Debug)]
pub enum Led2dFont {
    Font3x4Trim,
    Font4x6,
    Font3x5Trim,
    Font5x7,
    Font4x6Trim,
    Font5x8,
    Font4x7Trim,
    Font6x9,
    Font5x8Trim,
    Font6x10,
    Font5x9Trim,
    Font6x12,
    Font5x11Trim,
    Font6x13,
    Font5x12Trim,
    Font6x13Bold,
    Font5x12TrimBold,
    Font6x13Italic,
    Font5x12TrimItalic,
    Font7x13,
    Font6x12Trim,
    Font7x13Bold,
    Font6x12TrimBold,
    Font7x13Italic,
    Font6x12TrimItalic,
    Font7x14,
    Font6x13Trim,
    Font7x14Bold,
    Font6x13TrimBold,
    Font8x13,
    Font7x12Trim,
    Font8x13Bold,
    Font7x12TrimBold,
    Font8x13Italic,
    Font7x12TrimItalic,
    Font9x15,
    Font8x14Trim,
    Font9x15Bold,
    Font8x14TrimBold,
    Font9x18,
    Font8x17Trim,
    Font9x18Bold,
    Font8x17TrimBold,
    Font10x20,
    Font9x19Trim,
}

impl Led2dFont {
    /// Return the `MonoFont` for this variant.
    #[must_use]
    pub fn to_font(self) -> MonoFont<'static> {
        match self {
            Self::Font3x4Trim => bit_matrix3x4_font(),
            Self::Font4x6 | Self::Font3x5Trim => FONT_4X6,
            Self::Font5x7 | Self::Font4x6Trim => FONT_5X7,
            Self::Font5x8 | Self::Font4x7Trim => FONT_5X8,
            Self::Font6x9 | Self::Font5x8Trim => FONT_6X9,
            Self::Font6x10 | Self::Font5x9Trim => FONT_6X10,
            Self::Font6x12 | Self::Font5x11Trim => FONT_6X12,
            Self::Font6x13 | Self::Font5x12Trim => FONT_6X13,
            Self::Font6x13Bold | Self::Font5x12TrimBold => FONT_6X13_BOLD,
            Self::Font6x13Italic | Self::Font5x12TrimItalic => FONT_6X13_ITALIC,
            Self::Font7x13 | Self::Font6x12Trim => FONT_7X13,
            Self::Font7x13Bold | Self::Font6x12TrimBold => FONT_7X13_BOLD,
            Self::Font7x13Italic | Self::Font6x12TrimItalic => FONT_7X13_ITALIC,
            Self::Font7x14 | Self::Font6x13Trim => FONT_7X14,
            Self::Font7x14Bold | Self::Font6x13TrimBold => FONT_7X14_BOLD,
            Self::Font8x13 | Self::Font7x12Trim => FONT_8X13,
            Self::Font8x13Bold | Self::Font7x12TrimBold => FONT_8X13_BOLD,
            Self::Font8x13Italic | Self::Font7x12TrimItalic => FONT_8X13_ITALIC,
            Self::Font9x15 | Self::Font8x14Trim => FONT_9X15,
            Self::Font9x15Bold | Self::Font8x14TrimBold => FONT_9X15_BOLD,
            Self::Font9x18 | Self::Font8x17Trim => FONT_9X18,
            Self::Font9x18Bold | Self::Font8x17TrimBold => FONT_9X18_BOLD,
            Self::Font10x20 | Self::Font9x19Trim => FONT_10X20,
        }
    }

    /// Return spacing reduction for trimmed variants (cols, rows).
    #[must_use]
    pub const fn spacing_reduction(self) -> (i32, i32) {
        match self {
            Self::Font3x4Trim
            | Self::Font4x6
            | Self::Font5x7
            | Self::Font5x8
            | Self::Font6x9
            | Self::Font6x10
            | Self::Font6x12
            | Self::Font6x13
            | Self::Font6x13Bold
            | Self::Font6x13Italic
            | Self::Font7x13
            | Self::Font7x13Bold
            | Self::Font7x13Italic
            | Self::Font7x14
            | Self::Font7x14Bold
            | Self::Font8x13
            | Self::Font8x13Bold
            | Self::Font8x13Italic
            | Self::Font9x15
            | Self::Font9x15Bold
            | Self::Font9x18
            | Self::Font9x18Bold
            | Self::Font10x20 => (0, 0),
            Self::Font3x5Trim
            | Self::Font4x6Trim
            | Self::Font4x7Trim
            | Self::Font5x8Trim
            | Self::Font5x9Trim
            | Self::Font5x11Trim
            | Self::Font5x12Trim
            | Self::Font5x12TrimBold
            | Self::Font5x12TrimItalic
            | Self::Font6x12Trim
            | Self::Font6x12TrimBold
            | Self::Font6x12TrimItalic
            | Self::Font6x13Trim
            | Self::Font6x13TrimBold
            | Self::Font7x12Trim
            | Self::Font7x12TrimBold
            | Self::Font7x12TrimItalic
            | Self::Font8x14Trim
            | Self::Font8x14TrimBold
            | Self::Font8x17Trim
            | Self::Font8x17TrimBold
            | Self::Font9x19Trim => (1, 1),
        }
    }
}

// cmk0 should also define Default via the trait
/// A 2D array of RGB pixels representing a single display frame.
///
/// Frames are used to prepare images before sending them to the LED matrix. They support:
/// - Direct pixel access via array indexing
/// - Full graphics drawing via [`embedded-graphics`](https://docs.rs/embedded-graphics) (lines, shapes, text, and more)
/// - Automatic conversion to the strip's physical LED order
///
/// Frames are stored in row-major order where `frame[row][col]` represents the pixel
/// at display coordinates (col, row). The physical mapping to the LED strip is handled
/// automatically by the device abstraction.
///
/// # Examples
///
/// Direct pixel access:
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// # use device_kit::led2d::Frame;
/// # use smart_leds::RGB8;
/// # fn example() {
/// let mut frame = Frame::<4, 12>::new();  // 4 rows × 12 columns
/// frame[0][0] = RGB8::new(255, 0, 0);     // Set top-left pixel to red
/// frame[3][11] = RGB8::new(0, 255, 0);    // Set bottom-right pixel to green
/// # }
/// ```
///
/// Drawing with [`embedded-graphics`](https://docs.rs/embedded-graphics):
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// # use device_kit::led2d::{Frame, rgb8_to_rgb888};
/// # use smart_leds::RGB8;
/// use embedded_graphics::{prelude::*, primitives::{Line, PrimitiveStyle}};
/// # fn example() {
/// let mut frame = Frame::<8, 12>::new();
/// let color = rgb8_to_rgb888(RGB8::new(255, 0, 0));
/// Line::new(Point::new(0, 0), Point::new(11, 7))
///     .into_styled(PrimitiveStyle::with_stroke(color, 1))
///     .draw(&mut frame)
///     .unwrap();
/// # }
/// ```
///
/// See the [module-level documentation](mod@crate::led2d) for more usage examples.
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

#[doc(hidden)]
// Public so macro expansions in downstream crates can share the command signal type.
pub type Led2dCommandSignal<const N: usize, const MAX_FRAMES: usize> =
    Signal<CriticalSectionRawMutex, Command<N, MAX_FRAMES>>;
#[doc(hidden)]
// Public so macro expansions in downstream crates can observe completion signals.
pub type Led2dCompletionSignal = Signal<CriticalSectionRawMutex, ()>;

#[doc(hidden)]
// Public so macro-generated tasks can share the command channel type.
/// Command for the LED device loop.
#[derive(Clone)]
pub enum Command<const N: usize, const MAX_FRAMES: usize> {
    DisplayStatic([RGB8; N]),
    Animate(Vec<([RGB8; N], Duration), MAX_FRAMES>),
}

/// Static type for the [`Led2d`] device abstraction.
///
/// Most users should use the `led2d!` or `led2d_from_strip!` macros which generate
/// a higher-level wrapper.
pub struct Led2dStatic<const N: usize, const MAX_FRAMES: usize> {
    pub command_signal: Led2dCommandSignal<N, MAX_FRAMES>,
    pub completion_signal: Led2dCompletionSignal,
}

impl<const N: usize, const MAX_FRAMES: usize> Led2dStatic<N, MAX_FRAMES> {
    #[must_use]
    pub const fn new_static() -> Self {
        Self {
            command_signal: Signal::new(),
            completion_signal: Signal::new(),
        }
    }
}

/// Internal trait for types that can update LED pixels.
#[doc(hidden)] // Required pub for macro expansion in downstream crates
pub trait UpdatePixels<const N: usize> {
    async fn update_pixels(&self, pixels: &[RGB8; N]) -> Result<()>;
}

#[cfg(not(feature = "host"))]
impl<const N: usize> UpdatePixels<N> for crate::led_strip::LedStripShared<N> {
    async fn update_pixels(&self, pixels: &[RGB8; N]) -> Result<()> {
        crate::led_strip::LedStripShared::update_pixels(self, pixels).await
    }
}

#[cfg(not(feature = "host"))]
impl<const N: usize, T> UpdatePixels<N> for &T
where
    T: UpdatePixels<N>,
{
    async fn update_pixels(&self, pixels: &[RGB8; N]) -> Result<()> {
        T::update_pixels(self, pixels).await
    }
}

// cmk000 don't use the phrase 'module-level' in docs.
// cmk00 this needs a compiled-only doc test.
/// A device abstraction for rectangular WS2812-styleLED matrix displays.
///
/// Supports any size display with arbitrary coordinate-to-LED-index mapping.
/// The mapping is stored as a runtime slice, allowing stable Rust without experimental features.
///
/// Rows and columns are metadata used only for indexing - the core type is generic only over
/// N (total LEDs) and MAX_FRAMES (animation capacity).
///
/// Most users should use the `led2d!` or `led2d_from_strip!` macros which generate
/// a higher-level wrapper. See the [module-level documentation](mod@crate::led2d) for examples.
pub struct Led2d<'a, const N: usize, const MAX_FRAMES: usize> {
    command_signal: &'static Led2dCommandSignal<N, MAX_FRAMES>,
    completion_signal: &'static Led2dCompletionSignal,
    mapping: &'a [u16],
    cols: usize,
}

impl<'a, const N: usize, const MAX_FRAMES: usize> Led2d<'a, N, MAX_FRAMES> {
    /// Create Led2d device handle.
    ///
    /// The `mapping` slice defines how (column, row) coordinates map to LED strip indices.
    /// Index `row * cols + col` gives the LED index for that position.
    /// Length must equal N (checked with debug_assert).
    #[must_use]
    pub fn new(
        led2d_static: &'static Led2dStatic<N, MAX_FRAMES>,
        mapping: &'a [u16],
        cols: usize,
    ) -> Self {
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
    /// Each frame is a tuple of `(Frame, Duration)`. Accepts arrays, `Vec`s, or any
    /// iterator that produces `(Frame, Duration)` tuples. For best efficiency with large
    /// frame sequences, pass an iterator to avoid intermediate allocations.
    pub async fn animate<const ROWS: usize, const COLS: usize>(
        &self,
        frames: impl IntoIterator<Item = (Frame<ROWS, COLS>, Duration)>,
    ) -> Result<()> {
        assert!(
            MAX_FRAMES > 0,
            "max_frames must be positive for Led2d animations"
        );
        let mut sequence: Vec<([RGB8; N], Duration), MAX_FRAMES> = Vec::new();
        for (frame, duration) in frames {
            assert!(
                duration.as_micros() > 0,
                "animation frame duration must be positive"
            );
            let frame_1d = self.convert_frame(frame);
            sequence
                .push((frame_1d, duration))
                .expect("animation sequence fits");
        }
        assert!(
            !sequence.is_empty(),
            "animation requires at least one frame"
        );
        defmt::info!("Led2d::animate: sending {} frames", sequence.len());
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

// Must be `pub` (not `pub(crate)`) because called by macro-generated code that expands at the call site in downstream crates.
// This is an implementation detail, not part of the user-facing API.
#[doc(hidden)]
#[allow(private_bounds)]
/// Device loop for Led2d. Called by macro-generated code.
///
/// Since embassy tasks cannot be generic, the macros generate a concrete wrapper task
/// that calls this function. Must be `pub` because macro expansion happens in the calling
/// crate's context, but hidden from docs as it's not part of the public API.
pub async fn led2d_device_loop<const N: usize, const MAX_FRAMES: usize, S>(
    command_signal: &'static Led2dCommandSignal<N, MAX_FRAMES>,
    completion_signal: &'static Led2dCompletionSignal,
    led_strip: S,
) -> Result<Infallible>
where
    S: UpdatePixels<N>,
{
    defmt::info!("led2d_device_loop: task started");
    loop {
        defmt::debug!("led2d_device_loop: waiting for command");
        let command = command_signal.wait().await;
        command_signal.reset();

        match command {
            Command::DisplayStatic(frame) => {
                defmt::info!("led2d_device_loop: received DisplayStatic command");
                led_strip.update_pixels(&frame).await?;
                completion_signal.signal(());
                defmt::info!("led2d_device_loop: DisplayStatic completed");
            }
            Command::Animate(frames) => {
                defmt::info!(
                    "led2d_device_loop: received Animate command with {} frames",
                    frames.len()
                );
                let next_command =
                    run_animation_loop(frames, command_signal, completion_signal, &led_strip)
                        .await?;
                defmt::info!("led2d_device_loop: animation interrupted");
                match next_command {
                    Command::DisplayStatic(frame) => {
                        defmt::info!(
                            "led2d_device_loop: processing DisplayStatic from animation interrupt"
                        );
                        led_strip.update_pixels(&frame).await?;
                        completion_signal.signal(());
                    }
                    Command::Animate(new_frames) => {
                        defmt::info!("led2d_device_loop: restarting with new animation");
                        // Process the new animation immediately without waiting for next command
                        let next_command = run_animation_loop(
                            new_frames,
                            command_signal,
                            completion_signal,
                            &led_strip,
                        )
                        .await?;
                        // Handle any command that interrupted this animation
                        match next_command {
                            Command::DisplayStatic(frame) => {
                                led_strip.update_pixels(&frame).await?;
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

async fn run_animation_loop<const N: usize, const MAX_FRAMES: usize, S>(
    frames: Vec<([RGB8; N], Duration), MAX_FRAMES>,
    command_signal: &'static Led2dCommandSignal<N, MAX_FRAMES>,
    completion_signal: &'static Led2dCompletionSignal,
    led_strip: &S,
) -> Result<Command<N, MAX_FRAMES>>
where
    S: UpdatePixels<N>,
{
    defmt::info!("run_animation_loop: starting with {} frames", frames.len());
    completion_signal.signal(());
    defmt::debug!("run_animation_loop: signaled completion (animation started)");

    loop {
        for (frame_index, (pixels, duration)) in frames.iter().enumerate() {
            defmt::trace!("run_animation_loop: displaying frame {}", frame_index);
            led_strip.update_pixels(pixels).await?;

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
macro_rules! led2d_device_task {
    (
        $task_name:ident,
        $strip_ty:ty,
        $n:expr,
        $max_frames:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!(
            @inner
            ()
            $task_name,
            $strip_ty,
            $n,
            $max_frames
        );
    };
    (
        $vis:vis $task_name:ident,
        $strip_ty:ty,
        $n:expr,
        $max_frames:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!(
            @inner
            ($vis)
            $task_name,
            $strip_ty,
            $n,
            $max_frames
        );
    };
    (
        @inner
        ($($vis:tt)*)
        $task_name:ident,
        $strip_ty:ty,
        $n:expr,
        $max_frames:expr $(,)?
    ) => {
        #[embassy_executor::task]
        #[allow(non_snake_case)]
        $($vis)* async fn $task_name(
            command_signal: &'static $crate::led2d::Led2dCommandSignal<$n, $max_frames>,
            completion_signal: &'static $crate::led2d::Led2dCompletionSignal,
            led_strip: $strip_ty,
        ) {
            let err =
                $crate::led2d::led2d_device_loop(command_signal, completion_signal, led_strip)
                    .await
                    .unwrap_err();
            panic!("{err}");
        }
    };
}

#[doc(hidden)]
#[cfg(not(feature = "host"))]
pub use led2d_device_task;

#[doc(hidden)]
#[macro_export]
#[cfg(not(feature = "host"))]
macro_rules! led2d_device {
    (
        $vis:vis struct $resources_name:ident,
        task: $task_vis:vis $task_name:ident,
        strip: $strip_ty:ty,
        leds: $n:expr,
        mapping: $mapping:expr,
        cols: $cols:expr,
        max_frames: $max_frames:expr $(,)?
    ) => {
        $crate::led2d::led2d_device_task!($task_vis $task_name, $strip_ty, $n, $max_frames);

        $vis struct $resources_name {
            led2d_static: $crate::led2d::Led2dStatic<$n, $max_frames>,
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
                led_strip: $strip_ty,
                spawner: ::embassy_executor::Spawner,
            ) -> $crate::Result<$crate::led2d::Led2d<'static, $n, $max_frames>> {
                let token = $task_name(
                    &self.led2d_static.command_signal,
                    &self.led2d_static.completion_signal,
                    led_strip,
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

#[doc(hidden)]
#[cfg(not(feature = "host"))]
pub use led2d_device;

/// Generate a complete Led2d display with automatic PIO and strip management.
///
/// This macro creates a self-contained LED matrix display with automatic PIO splitting
/// and internal resource management. For simpler single-strip displays, this is the
/// recommended approach. For multi-strip scenarios where you need to share a PIO with
/// other devices, use [`led2d_from_strip!`] instead.
///
/// The macro generates everything needed: the LED strip infrastructure, the Led2d
/// device abstraction, and a simplified constructor that handles all initialization.
///
/// # Parameters
///
/// - Visibility and base name for generated types (e.g., `pub led12x4`)
/// - `pio` - PIO peripheral to use (e.g., `PIO0`, `PIO1`)
/// - `pin` - GPIO pin for LED data signal (e.g., `PIN_3`)
/// - `dma` - DMA channel for LED data transfer (e.g., `DMA_CH0`)
/// - `rows` - Number of rows in the display
/// - `cols` - Number of columns in the display
/// - `mapping` - LED strip physical layout (currently only `serpentine_column_major` supported)
/// - `max_current` - Maximum current budget (e.g., `Milliamps(500)`)
/// - `max_frames` - Maximum animation frames allowed (not buffered)
/// - `font` - Built-in font variant (see [`Led2dFont`])
///
/// # Generated API
///
/// The macro generates a type `YourName` with a simplified constructor:
/// - `YourName::new_simple(pio, dma, pin, spawner)` - Single-call initialization
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use embassy_executor::Spawner;
/// use embassy_rp::init;
/// use device_kit::led2d;
/// use device_kit::led_strip::Milliamps;
/// use device_kit::led_strip::gamma::Gamma;
/// use device_kit::led_strip::colors;
///
/// // Generate a 12×4 LED matrix display
/// led2d! {
///     pub led12x4,
///     pio: PIO0,
///     pin: PIN_3,
///     dma: DMA_CH1,
///     rows: 4,
///     cols: 12,
///     mapping: serpentine_column_major,
///     max_current: Milliamps(500),
///     gamma: Gamma::Linear,
///     max_frames: 32,
///     font: Font3x4Trim,
/// }
///
/// #[embassy_executor::main]
/// async fn main(spawner: Spawner) {
///     let p = init(Default::default());
///     
///     // Single-call initialization
///     let led = Led12x4::new(p.PIO0, p.DMA_CH1, p.PIN_3, spawner).unwrap();
///     
///     // Display text
///     led.write_text("HELLO", &[colors::RED, colors::GREEN, colors::BLUE]).await.unwrap();
/// }
/// ```
#[macro_export]
#[cfg(not(feature = "host"))]
macro_rules! led2d {
    (
        $vis:vis $name:ident,
        pio: $pio:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        mapping: serpentine_column_major,
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        font: $font_variant:ident $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            // Generate the LED strip infrastructure with a CamelCase strip type
            $crate::led_strip::define_led_strips_shared! {
                pio: $pio,
                strips: [
                    [<$name:camel Strip>] {
                        sm: 0,
                        dma: $dma,
                        pin: $pin,
                        len: { $rows * $cols },
                        max_current: $max_current,
                        gamma: $gamma
                    }
                ]
            }

            // Generate the Led2d device from the strip
            $crate::led2d::led2d_from_strip! {
                $vis $name,
                strip_type: [<$name:camel Strip>],
                rows: $rows,
                cols: $cols,
                mapping: serpentine_column_major,
                max_frames: $max_frames,
                font: $font_variant,
            }

            // Add simplified constructor that handles PIO splitting and both statics
            impl [<$name:camel>] {
                /// Create a new LED matrix display with automatic PIO setup.
                ///
                /// This is a convenience constructor that handles PIO splitting and static
                /// resource management automatically. All initialization happens in a single call.
                ///
                /// # Parameters
                ///
                /// - `pio`: PIO peripheral
                /// - `dma`: DMA channel for LED data transfer
                /// - `pin`: GPIO pin for LED data signal
                /// - `spawner`: Task spawner for background operations
                #[allow(non_upper_case_globals)]
                $vis fn new(
                    pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
                    dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                    pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<Self> {
                    // Split PIO into state machines (uses SM0 automatically)
                    let (sm0, _sm1, _sm2, _sm3) = [<$pio:lower _split>](pio);

                    // Create strip (uses interior static)
                    let strip = [<$name:camel Strip>]::new(
                        sm0,
                        dma,
                        pin,
                        spawner
                    )?;

                    // Create Led2d from strip (uses interior static)
                    [<$name:camel>]::from_strip(strip, spawner)
                }
            }
        }
    };
    // Arbitrary custom mapping variant
    (
        $vis:vis $name:ident,
        pio: $pio:ident,
        pin: $pin:ident,
        dma: $dma:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        mapping: arbitrary([$($index:expr),* $(,)?]),
        max_current: $max_current:expr,
        gamma: $gamma:expr,
        max_frames: $max_frames:expr,
        font: $font_variant:ident $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            // Generate the LED strip infrastructure with a CamelCase strip type
            $crate::led_strip::define_led_strips_shared! {
                pio: $pio,
                strips: [
                    [<$name:camel Strip>] {
                        sm: 0,
                        dma: $dma,
                        pin: $pin,
                        len: $rows * $cols,
                        max_current: $max_current,
                        gamma: $gamma
                    }
                ]
            }

            // Generate the Led2d device from the strip with arbitrary mapping
            $crate::led2d::led2d_from_strip! {
                $vis $name,
                strip_type: [<$name:camel Strip>],
                rows: $rows,
                cols: $cols,
                mapping: arbitrary([$($index),*]),
                max_frames: $max_frames,
                font: $font_variant,
            }

            // Add simplified constructor that handles PIO splitting and both statics
            impl [<$name:camel>] {
                /// Create a new LED matrix display with automatic PIO setup.
                ///
                /// This is a convenience constructor that handles PIO splitting and static
                /// resource management automatically. All initialization happens in a single call.
                ///
                /// # Parameters
                ///
                /// - `pio`: PIO peripheral
                /// - `dma`: DMA channel for LED data transfer
                /// - `pin`: GPIO pin for LED data signal
                /// - `spawner`: Task spawner for background operations
                #[allow(non_upper_case_globals)]
                $vis fn new(
                    pio: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pio>,
                    dma: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$dma>,
                    pin: ::embassy_rp::Peri<'static, ::embassy_rp::peripherals::$pin>,
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<Self> {
                    // Split PIO into state machines (uses SM0 automatically)
                    let (sm0, _sm1, _sm2, _sm3) = [<$pio:lower _split>](pio);

                    // Create strip (uses interior static)
                    let led_strip = [<$name:camel Strip>]::new(
                        sm0,
                        dma,
                        pin,
                        spawner
                    )?;

                    // Create Led2d from strip (uses interior static)
                    [<$name:camel>]::from_strip(led_strip, spawner)
                }
            }
        }
    };
}

/// Generate a Led2d device abstraction from an existing LED strip type.
///
/// Use this macro when you want to share a PIO across multiple LED strips and treat one as a 2D display.
/// For simple single-strip displays, use `led2d!` instead.
/// The strip must be created with [`define_led_strips_shared!`](crate::led_strip::define_led_strips_shared).
///
/// # Parameters
///
/// - Visibility and base name for generated types (e.g., `pub led12x4`)
/// - `strip_type` - Name of the strip type created by `define_led_strips_shared!`
/// - `rows` - Number of rows in the display
/// - `cols` - Number of columns in the display
/// - `mapping` - LED strip physical layout:
///   - `serpentine_column_major` - Common serpentine wiring pattern
///   - `arbitrary([indices...])` - Custom mapping array
/// - `max_frames` - Maximum animation frames allowed (not buffered)
/// - `font` - Built-in font variant (see [`Led2dFont`])
///
/// # Example
///
/// ```no_run
/// # #![no_std]
/// # #![no_main]
/// # use panic_probe as _;
/// use device_kit::led_strip::define_led_strips_shared;
/// use device_kit::led2d::led2d_from_strip;
/// use device_kit::led_strip::Milliamps;
/// use device_kit::led_strip::gamma::Gamma;
/// use device_kit::pio_split;
/// use embassy_executor::Spawner;
///
/// // Define multiple strips sharing PIO1
/// define_led_strips_shared! {
///     pio: PIO1,
///     strips: [
///         Led12x4Strip {
///             sm: 0,
///             dma: DMA_CH0,
///             pin: PIN_3,
///             len: 48,
///             max_current: Milliamps(500),
///             gamma: Gamma::Linear
///         }
///     ]
/// }
///
/// // Wrap the strip as a Led2d surface
/// led2d_from_strip! {
///     pub led12x4,
///     strip_type: Led12x4Strip,
///     rows: 4,
///     cols: 12,
///     mapping: serpentine_column_major,
///     max_frames: 32,
///     font: Font3x4Trim,
/// }
///
/// # #[embassy_executor::main]
/// # async fn main(spawner: Spawner) {
/// #     let p = embassy_rp::init(Default::default());
/// #     let (sm0, _sm1, _sm2, _sm3) = pio_split!(p.PIO1);
/// #     let strip = Led12x4Strip::new(sm0, p.DMA_CH0, p.PIN_3, spawner).unwrap();
/// #     let led = Led12x4::from_strip(strip, spawner).unwrap();
/// # }
/// ```
#[macro_export]
#[cfg(not(feature = "host"))]
macro_rules! led2d_from_strip {
    // Serpentine column-major mapping variant
    (
        $vis:vis $name:ident,
        strip_type: $strip_type:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        mapping: serpentine_column_major,
        max_frames: $max_frames:expr,
        font: $font_variant:ident $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            const [<$name:upper _ROWS>]: usize = $rows;
            const [<$name:upper _COLS>]: usize = $cols;
            const [<$name:upper _N>]: usize = [<$name:upper _ROWS>] * [<$name:upper _COLS>];
            const [<$name:upper _MAPPING>]: [u16; [<$name:upper _N>]] = $crate::led2d::serpentine_column_major_mapping::<[<$name:upper _N>], [<$name:upper _ROWS>], [<$name:upper _COLS>]>();
            const [<$name:upper _MAX_FRAMES>]: usize = $max_frames;

            // Compile-time assertion that strip length matches mapping length
            const _: () = assert!([<$name:upper _MAPPING>].len() == $strip_type::LEN);

            $crate::led2d::led2d_from_strip!(
                @common $vis, $name, $strip_type, [<$name:upper _ROWS>], [<$name:upper _COLS>], [<$name:upper _N>], [<$name:upper _MAPPING>],
                $font_variant,
                [<$name:upper _MAX_FRAMES>]
            );
        }
    };
    // Arbitrary custom mapping variant
    (
        $vis:vis $name:ident,
        strip_type: $strip_type:ident,
        rows: $rows:expr,
        cols: $cols:expr,
        mapping: arbitrary([$($index:expr),* $(,)?]),
        max_frames: $max_frames:expr,
        font: $font_variant:ident $(,)?
    ) => {
        $crate::led2d::paste::paste! {
            const [<$name:upper _ROWS>]: usize = $rows;
            const [<$name:upper _COLS>]: usize = $cols;
            const [<$name:upper _N>]: usize = [<$name:upper _ROWS>] * [<$name:upper _COLS>];
            const [<$name:upper _MAPPING>]: [u16; [<$name:upper _N>]] = [$($index),*];
            const [<$name:upper _MAX_FRAMES>]: usize = $max_frames;

            // Compile-time assertion that strip length matches mapping length
            const _: () = assert!([<$name:upper _MAPPING>].len() == $strip_type::LEN);

            $crate::led2d::led2d_from_strip!(
                @common $vis, $name, $strip_type, [<$name:upper _ROWS>], [<$name:upper _COLS>], [<$name:upper _N>], [<$name:upper _MAPPING>],
                $font_variant,
                [<$name:upper _MAX_FRAMES>]
            );
        }
    };
    // Common implementation (shared by both variants)
    (
        @common $vis:vis,
        $name:ident,
        $strip_type:ident,
        $rows_const:ident,
        $cols_const:ident,
        $n_const:ident,
        $mapping_const:ident,
        $font_variant:expr,
        $max_frames_const:ident
    ) => {
        $crate::led2d::paste::paste! {
            /// Static resources for the LED matrix device.
            $vis struct [<$name:camel Static>] {
                led2d_static: $crate::led2d::Led2dStatic<$n_const, $max_frames_const>,
            }

            // Generate the task wrapper
            $crate::led2d::led2d_device_task!(
                [<$name _device_loop>],
                &'static $strip_type,
                $n_const,
                $max_frames_const
            );

            /// LED matrix device handle generated by [`led2d_from_strip!`](crate::led2d::led2d_from_strip).
            $vis struct [<$name:camel>] {
                led2d: $crate::led2d::Led2d<'static, $n_const, $max_frames_const>,
                font: embedded_graphics::mono_font::MonoFont<'static>,
                font_variant: $crate::led2d::Led2dFont,
            }

            /// Frame type for this LED matrix display.
            ///
            /// This is a convenience type alias for `Frame<ROWS, COLS>` specific to this device.
            $vis type [<$name:camel Frame>] = $crate::led2d::Frame<$rows_const, $cols_const>;

            impl [<$name:camel>] {
                /// Number of rows in the display.
                pub const ROWS: usize = $rows_const;
                /// Number of columns in the display.
                pub const COLS: usize = $cols_const;
                /// Total number of LEDs (ROWS * COLS).
                pub const N: usize = $n_const;
                /// Maximum animation frames supported for this device.
                pub const MAX_FRAMES: usize = $max_frames_const;

                /// Create static resources.
                #[must_use]
                $vis const fn new_static() -> [<$name:camel Static>] {
                    [<$name:camel Static>] {
                        led2d_static: $crate::led2d::Led2dStatic::new_static(),
                    }
                }

                /// Create a new blank (all black) frame.
                #[must_use]
                $vis const fn new_frame() -> $crate::led2d::Frame<$rows_const, $cols_const> {
                    $crate::led2d::Frame::new()
                }

                /// Create a new LED matrix display instance from an existing strip.
                ///
                /// The strip must be created from the same type specified in `strip_type`.
                /// For simpler single-strip setups, see the convenience constructor generated
                /// by [`led2d!`].
                ///
                /// # Parameters
                ///
                /// - `led_strip`: LED strip instance from the specified strip type
                /// - `spawner`: Task spawner for background operations
                $vis fn from_strip(
                    led_strip: &'static $strip_type,
                    spawner: ::embassy_executor::Spawner,
                ) -> $crate::Result<Self> {
                    static STATIC: [<$name:camel Static>] = [<$name:camel>]::new_static();

                    defmt::info!("Led2d::new: spawning device task");
                    let token = [<$name _device_loop>](
                        &STATIC.led2d_static.command_signal,
                        &STATIC.led2d_static.completion_signal,
                        led_strip,
                    )?;
                    spawner.spawn(token);
                    defmt::info!("Led2d::new: device task spawned");

                    let led2d = $crate::led2d::Led2d::new(
                        &STATIC.led2d_static,
                        &$mapping_const,
                        $cols_const,
                    );

                    defmt::info!("Led2d::new: device created successfully");
                    Ok(Self {
                        led2d,
                        font: $crate::led2d::Led2dFont::$font_variant.to_font(),
                        font_variant: $crate::led2d::Led2dFont::$font_variant,
                    })
                }

                /// Render a fully defined frame to the display.
                $vis async fn write_frame(&self, frame: $crate::led2d::Frame<$rows_const, $cols_const>) -> $crate::Result<()> {
                    self.led2d.write_frame(frame).await
                }

                /// Loop through a sequence of animation frames. Pass arrays by value or Vecs/iters.
                $vis async fn animate(&self, frames: impl IntoIterator<Item = ($crate::led2d::Frame<$rows_const, $cols_const>, ::embassy_time::Duration)>) -> $crate::Result<()> {
                    self.led2d.animate(frames).await
                }

                /// Render text into a frame using the configured font and spacing.
                pub fn write_text_to_frame(
                    &self,
                    text: &str,
                    colors: &[smart_leds::RGB8],
                    frame: &mut $crate::led2d::Frame<$rows_const, $cols_const>,
                ) -> $crate::Result<()> {
                    $crate::led2d::render_text_to_frame(frame, &self.font, text, colors, self.font_variant.spacing_reduction())
                }

                /// Render text and display it on the LED matrix.
                pub async fn write_text(&self, text: &str, colors: &[smart_leds::RGB8]) -> $crate::Result<()> {
                    let mut frame = Self::new_frame();
                    self.write_text_to_frame(text, colors, &mut frame)?;
                    self.write_frame(frame).await
                }
            }
        }
    };
}

#[cfg(not(feature = "host"))]
#[doc(inline)]
pub use led2d;
#[cfg(not(feature = "host"))]
#[doc(inline)]
pub use led2d_from_strip;
