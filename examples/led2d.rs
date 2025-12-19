#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::{init, peripherals::PIO1};
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    Drawable,
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
};
use heapless::Vec;
use panic_probe as _;
use serials::button::{Button, PressedTo};
use serials::led_strip_simple::{LedStripSimple, LedStripSimpleStatic, Milliamps, colors};
use serials::led2d::{Frame, Led2d, led2d_device, serpentine_column_major_mapping};
use serials::{Error, Result};
use smart_leds::RGB8;

const COLS: usize = 12;
const ROWS: usize = 4;
const N: usize = COLS * ROWS;

// Serpentine column-major mapping for 12x4 display
const MAPPING: [u16; N] = serpentine_column_major_mapping::<N, ROWS, COLS>();

led2d_device!(
    struct Led2dDeviceResources,
    task: run_led2d_device_loop,
    strip: LedStripSimple<'static, PIO1, N>,
    leds: N,
    mapping: &MAPPING,
    cols: COLS,
);

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("LED 2D API Exploration (12x4 display)");
    let p = init(Default::default());

    // Create LED strip
    static LED_STRIP_STATIC: LedStripSimpleStatic<N> = LedStripSimpleStatic::new_static();
    let led_strip =
        LedStripSimple::new_pio1(&LED_STRIP_STATIC, p.PIO1, p.PIN_3, Milliamps(500)).await;

    static LED2D_DEVICE_RESOURCES_STATIC: Led2dDeviceResources = Led2dDeviceResources::new_static();
    let led2d = LED2D_DEVICE_RESOURCES_STATIC.new(led_strip, spawner)?;

    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    loop {
        info!("Demo 1: Colored corners");
        demo_colored_corners(&led2d).await?;
        button.wait_for_press_duration().await;

        info!("Demo 2: Blink pattern");
        demo_blink_pattern(&led2d).await?;
        button.wait_for_press_duration().await;

        info!("Demo 3: Rectangle with diagonals (embedded-graphics)");
        demo_rectangle_diagonals_embedded_graphics(&led2d).await?;
        button.wait_for_press_duration().await;

        info!("Demo 4: Bouncing dot (manual frames)");
        demo_bouncing_dot_manual(&led2d).await?;
        button.wait_for_press_duration().await;

        info!("Demo 5: Bouncing dot (animation)");
        demo_bouncing_dot_animation(&led2d).await?;
        button.wait_for_press_duration().await;
    }
}

/// Display colored corners to demonstrate coordinate mapping.
async fn demo_colored_corners(led2d: &Led2d<'_, N>) -> Result<()> {
    let black = colors::BLACK;
    let mut frame = [black; N];

    // Four corners with different colors
    frame[led2d.xy_to_index(0, 0)] = colors::RED; // Top-left
    frame[led2d.xy_to_index(COLS - 1, 0)] = colors::GREEN; // Top-right
    frame[led2d.xy_to_index(0, ROWS - 1)] = colors::BLUE; // Bottom-left
    frame[led2d.xy_to_index(COLS - 1, ROWS - 1)] = colors::YELLOW; // Bottom-right

    led2d.write_frame(frame).await?;
    Timer::after_millis(1000).await;
    Ok(())
}

/// Blink a pattern by constructing frames explicitly.
async fn demo_blink_pattern(led2d: &Led2d<'_, N>) -> Result<()> {
    let black = colors::BLACK;

    // Create checkerboard pattern
    let mut on_frame = [black; N];
    for row_index in 0..ROWS {
        for column_index in 0..COLS {
            if (row_index + column_index) % 2 == 0 {
                on_frame[led2d.xy_to_index(column_index, row_index)] = colors::CYAN;
            }
        }
    }

    let off_frame = [black; N];
    let frames = [
        Frame::new(on_frame, Duration::from_millis(500)),
        Frame::new(off_frame, Duration::from_millis(500)),
    ];
    led2d.animate(&frames).await
}

/// Frame builder that implements DrawTarget for embedded-graphics.
struct FrameBuilder<'a> {
    image: [[RGB8; COLS]; ROWS],
    led2d: &'a Led2d<'a, N>,
}

impl<'a> FrameBuilder<'a> {
    fn new(led2d: &'a Led2d<'a, N>) -> Self {
        let black = RGB8::new(0, 0, 0);
        Self {
            image: [[black; COLS]; ROWS],
            led2d,
        }
    }

    fn build(&self) -> [RGB8; N] {
        let mut frame = [RGB8::new(0, 0, 0); N];
        for row_index in 0..ROWS {
            for column_index in 0..COLS {
                frame[self.led2d.xy_to_index(column_index, row_index)] =
                    self.image[row_index][column_index];
            }
        }
        frame
    }
}

impl<'a> DrawTarget for FrameBuilder<'a> {
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
                self.image[row_index as usize][column_index as usize] =
                    RGB8::new(color.r(), color.g(), color.b());
            }
        }
        Ok(())
    }
}

impl<'a> OriginDimensions for FrameBuilder<'a> {
    fn size(&self) -> Size {
        Size::new(COLS as u32, ROWS as u32)
    }
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led2d: &Led2d<'_, N>) -> Result<()> {
    let mut frame_builder = FrameBuilder::new(led2d);

    // Draw red rectangle border
    Rectangle::new(Point::new(0, 0), Size::new(COLS as u32, ROWS as u32))
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
        .draw(&mut frame_builder)
        .map_err(|_| Error::FormatError)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(
        Point::new(0, 0),
        Point::new((COLS - 1) as i32, (ROWS - 1) as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame_builder)
    .map_err(|_| Error::FormatError)?;

    Line::new(
        Point::new(0, (ROWS - 1) as i32),
        Point::new((COLS - 1) as i32, 0),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame_builder)
    .map_err(|_| Error::FormatError)?;

    let frame = frame_builder.build();
    led2d.write_frame(frame).await
}

/// Bouncing dot manually updating frames with write_frame in a loop.
async fn demo_bouncing_dot_manual(led2d: &Led2d<'_, N>) -> Result<()> {
    const COLORS: [RGB8; 6] = [
        colors::RED,
        colors::GREEN,
        colors::BLUE,
        colors::YELLOW,
        colors::CYAN,
        colors::MAGENTA,
    ];

    let black = RGB8::new(0, 0, 0);
    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..100 {
        let mut frame = [black; N];
        frame[led2d.xy_to_index(column_index as usize, row_index as usize)] = COLORS[color_index];
        led2d.write_frame(frame).await?;

        column_index = column_index + delta_column;
        row_index = row_index + delta_row;

        if column_index >= COLS as isize {
            column_index = (COLS as isize) - 2;
            delta_column = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if column_index < 0 {
            column_index = 1;
            delta_column = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        if row_index >= ROWS as isize {
            row_index = (ROWS as isize) - 2;
            delta_row = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if row_index < 0 {
            row_index = 1;
            delta_row = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        Timer::after_millis(50).await;
    }

    Ok(())
}

/// Bouncing dot using pre-built animation frames.
async fn demo_bouncing_dot_animation(led2d: &Led2d<'_, N>) -> Result<()> {
    const COLORS: [RGB8; 6] = [
        colors::RED,
        colors::GREEN,
        colors::BLUE,
        colors::YELLOW,
        colors::CYAN,
        colors::MAGENTA,
    ];

    let black = RGB8::new(0, 0, 0);
    let mut frames = Vec::<Frame<N>, 32>::new();
    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..32 {
        let mut frame = [black; N];
        frame[led2d.xy_to_index(column_index as usize, row_index as usize)] = COLORS[color_index];
        frames
            .push(Frame::new(frame, Duration::from_millis(50)))
            .map_err(|_| Error::FormatError)?;

        column_index = column_index + delta_column;
        row_index = row_index + delta_row;

        if column_index >= COLS as isize {
            column_index = (COLS as isize) - 2;
            delta_column = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if column_index < 0 {
            column_index = 1;
            delta_column = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        if row_index >= ROWS as isize {
            row_index = (ROWS as isize) - 2;
            delta_row = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if row_index < 0 {
            row_index = 1;
            delta_row = 1;
            color_index = (color_index + 1) % COLORS.len();
        }
    }

    led2d.animate(&frames).await
}
