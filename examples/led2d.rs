#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;

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
use serials::led2d::led2d_device_simple;
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

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
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
    // Four corners with different colors
    let mut frame = Led4x12::new_frame();
    frame[0][0] = colors::RED; // Top-left
    frame[0][Led4x12::COLS - 1] = colors::GREEN; // Top-right
    frame[Led4x12::ROWS - 1][0] = colors::BLUE; // Bottom-left
    frame[Led4x12::ROWS - 1][Led4x12::COLS - 1] = colors::YELLOW; // Bottom-right

    led4x12.write_frame(frame).await?;
    Ok(())
}

/// Blink a pattern by constructing frames explicitly.
async fn demo_blink_pattern(led4x12: &Led4x12) -> Result<()> {
    // Create checkerboard pattern
    let mut on_frame = Led4x12::new_frame();
    for row_index in 0..Led4x12::ROWS {
        for column_index in 0..Led4x12::COLS {
            if (row_index + column_index) % 2 == 0 {
                on_frame[row_index][column_index] = colors::CYAN;
            }
        }
    }

    led4x12
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led4x12::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led4x12: &Led4x12) -> Result<()> {
    let mut frame = Led4x12::new_frame();

    // Draw red rectangle border
    Rectangle::new(
        Point::new(0, 0),
        Size::new(Led4x12::COLS as u32, Led4x12::ROWS as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
    .draw(&mut frame)
    .map_err(|_| Error::FormatError)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(
        Point::new(0, 0),
        Point::new((Led4x12::COLS - 1) as i32, (Led4x12::ROWS - 1) as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame)
    .map_err(|_| Error::FormatError)?;

    Line::new(
        Point::new(0, (Led4x12::ROWS - 1) as i32),
        Point::new((Led4x12::COLS - 1) as i32, 0),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame)
    .map_err(|_| Error::FormatError)?;

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

    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..100 {
        let mut frame = Led4x12::new_frame();
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

    let mut frames = Vec::<_, 32>::new();
    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..32 {
        let mut frame = Led4x12::new_frame();
        frame[row_index as usize][column_index as usize] = COLORS[color_index];
        frames
            .push((frame, Duration::from_millis(50)))
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
