#![no_std]
#![no_main]
#![feature(never_type)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::init;
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
use serials::led_strip_simple::Milliamps;
use serials::led2d::{Frame, led2d_device_simple};
use serials::{Error, Result};
use smart_leds::{RGB8, colors};

// Create the led2d device using the macro
led2d_device_simple! {
    pub led4x12,
    rows: 4,
    cols: 12,
    pio: PIO1,
    mapping: serpentine_column_major,
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("LED 2D API Exploration (12x4 display)");
    let p = init(Default::default());

    static LED4X12_STATIC: Led4x12Static = Led4x12::new_static();
    let led4x12 = Led4x12::new(&LED4X12_STATIC, p.PIO1, p.PIN_3, Milliamps(500), spawner).await?;

    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    loop {
        info!("Demo 1: Colored corners");
        demo_colored_corners(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 2: Blink pattern");
        demo_blink_pattern(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 3: Rectangle with diagonals (embedded-graphics)");
        demo_rectangle_diagonals_embedded_graphics(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 4: Bouncing dot (manual frames)");
        demo_bouncing_dot_manual(&led4x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 5: Bouncing dot (animation)");
        demo_bouncing_dot_animation(&led4x12).await?;
        button.wait_for_press_duration().await;
    }
}

/// Display colored corners to demonstrate coordinate mapping.
async fn demo_colored_corners(led4x12: &Led4x12) -> Result<()> {
    let black = colors::BLACK;
    let mut frame = [[black; Led4x12::COLS]; Led4x12::ROWS];

    // Four corners with different colors
    frame[0][0] = colors::RED; // Top-left
    frame[0][Led4x12::COLS - 1] = colors::GREEN; // Top-right
    frame[Led4x12::ROWS - 1][0] = colors::BLUE; // Bottom-left
    frame[Led4x12::ROWS - 1][Led4x12::COLS - 1] = colors::YELLOW; // Bottom-right

    led4x12.write_frame(frame).await?;
    Timer::after_millis(1000).await;
    Ok(())
}

/// Blink a pattern by constructing frames explicitly.
async fn demo_blink_pattern(led4x12: &Led4x12) -> Result<()> {
    let black = colors::BLACK;

    // Create checkerboard pattern
    let mut on_frame = [[black; Led4x12::COLS]; Led4x12::ROWS];
    for row_index in 0..Led4x12::ROWS {
        for column_index in 0..Led4x12::COLS {
            if (row_index + column_index) % 2 == 0 {
                on_frame[row_index][column_index] = colors::CYAN;
            }
        }
    }

    let off_frame = [[black; Led4x12::COLS]; Led4x12::ROWS];
    let frames = [
        Frame::new(on_frame, Duration::from_millis(500)),
        Frame::new(off_frame, Duration::from_millis(500)),
    ];
    led4x12.animate(&frames).await
}

/// Frame builder that implements DrawTarget for embedded-graphics.
struct FrameBuilder {
    image: [[RGB8; Led4x12::COLS]; Led4x12::ROWS],
}

impl FrameBuilder {
    fn new() -> Self {
        let black = RGB8::new(0, 0, 0);
        Self {
            image: [[black; Led4x12::COLS]; Led4x12::ROWS],
        }
    }

    fn build(&self) -> [[RGB8; Led4x12::COLS]; Led4x12::ROWS] {
        self.image
    }
}

impl DrawTarget for FrameBuilder {
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
                && column_index < Led4x12::COLS as i32
                && row_index >= 0
                && row_index < Led4x12::ROWS as i32
            {
                self.image[row_index as usize][column_index as usize] =
                    RGB8::new(color.r(), color.g(), color.b());
            }
        }
        Ok(())
    }
}

impl OriginDimensions for FrameBuilder {
    fn size(&self) -> Size {
        Size::new(Led4x12::COLS as u32, Led4x12::ROWS as u32)
    }
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led4x12: &Led4x12) -> Result<()> {
    let mut frame_builder = FrameBuilder::new();

    // Draw red rectangle border
    Rectangle::new(
        Point::new(0, 0),
        Size::new(Led4x12::COLS as u32, Led4x12::ROWS as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
    .draw(&mut frame_builder)
    .map_err(|_| Error::FormatError)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(
        Point::new(0, 0),
        Point::new((Led4x12::COLS - 1) as i32, (Led4x12::ROWS - 1) as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame_builder)
    .map_err(|_| Error::FormatError)?;

    Line::new(
        Point::new(0, (Led4x12::ROWS - 1) as i32),
        Point::new((Led4x12::COLS - 1) as i32, 0),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame_builder)
    .map_err(|_| Error::FormatError)?;

    let frame = frame_builder.build();
    led4x12.write_frame(frame).await
}

/// Bouncing dot manually updating frames with write_frame in a loop.
async fn demo_bouncing_dot_manual(led4x12: &Led4x12) -> Result<()> {
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
        let mut frame = [[black; Led4x12::COLS]; Led4x12::ROWS];
        frame[row_index as usize][column_index as usize] = COLORS[color_index];
        led4x12.write_frame(frame).await?;

        column_index = column_index + delta_column;
        row_index = row_index + delta_row;

        if column_index >= Led4x12::COLS as isize {
            column_index = (Led4x12::COLS as isize) - 2;
            delta_column = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if column_index < 0 {
            column_index = 1;
            delta_column = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        if row_index >= Led4x12::ROWS as isize {
            row_index = (Led4x12::ROWS as isize) - 2;
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
async fn demo_bouncing_dot_animation(led4x12: &Led4x12) -> Result<()> {
    const COLORS: [RGB8; 6] = [
        colors::RED,
        colors::GREEN,
        colors::BLUE,
        colors::YELLOW,
        colors::CYAN,
        colors::MAGENTA,
    ];

    let black = RGB8::new(0, 0, 0);
    let mut frames = Vec::<Frame<{ Led4x12::ROWS }, { Led4x12::COLS }>, 32>::new();
    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..32 {
        let mut frame = [[black; Led4x12::COLS]; Led4x12::ROWS];
        frame[row_index as usize][column_index as usize] = COLORS[color_index];
        frames
            .push(Frame::new(frame, Duration::from_millis(50)))
            .map_err(|_| Error::FormatError)?;

        column_index = column_index + delta_column;
        row_index = row_index + delta_row;

        if column_index >= Led4x12::COLS as isize {
            column_index = (Led4x12::COLS as isize) - 2;
            delta_column = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if column_index < 0 {
            column_index = 1;
            delta_column = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        if row_index >= Led4x12::ROWS as isize {
            row_index = (Led4x12::ROWS as isize) - 2;
            delta_row = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if row_index < 0 {
            row_index = 1;
            delta_row = 1;
            color_index = (color_index + 1) % COLORS.len();
        }
    }

    led4x12.animate(&frames).await
}
