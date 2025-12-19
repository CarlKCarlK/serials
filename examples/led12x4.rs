#![no_std]
#![no_main]
#![feature(never_type)]
#![feature(inherent_associated_types)]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embedded_graphics::{
    Drawable,
    pixelcolor::Rgb888,
    prelude::*,
    primitives::{Line, PrimitiveStyle, Rectangle},
};
use heapless::Vec;
use panic_probe as _;
use serials::Result;
use serials::button::{Button, PressedTo};
use serials::led12x4::{Frame, Led12x4, Led12x4Static, Milliamps, colors, new_led12x4, text_frame};
use smart_leds::RGB8;

// cmk00 make this demo better, including fixing font

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    info!("LED 12x4 API Exploration");
    let p = embassy_rp::init(Default::default());

    static LED_12X4_STATIC: Led12x4Static = Led12x4Static::new_static();
    let led_12x4 = new_led12x4!(&LED_12X4_STATIC, PIN_3, p.PIO1, Milliamps(500), spawner).await?;

    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    loop {
        info!("Demo 1: Text colors");
        demo_text_colors(&led_12x4).await?;
        button.wait_for_press_duration().await;

        info!("Demo 2: Blink text");
        demo_blink_text(&led_12x4).await?;
        button.wait_for_press_duration().await;

        info!("Demo 3: Rectangle with diagonals (embedded-graphics)");
        demo_rectangle_diagonals_embedded_graphics(&led_12x4).await?;
        button.wait_for_press_duration().await;

        info!("Demo 4: Bouncing dot (manual frames)");
        demo_bouncing_dot_manual(&led_12x4).await?;
        button.wait_for_press_duration().await;

        info!("Demo 5: Bouncing dot (animation)");
        demo_bouncing_dot_animation(&led_12x4).await?;
        button.wait_for_press_duration().await;
    }
}

// cmk why is there a generic T here? (now resolved - using Led12x4Strip enum)
/// Display "RUST" in 4 different colors using write_text.
async fn demo_text_colors(led_12x4: &Led12x4) -> Result<()> {
    led_12x4
        .write_text(
            ['r', 'u', 's', 't'],
            [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        )
        .await
}

/// Blink "RUST" by constructing frames explicitly.
async fn demo_blink_text(led_12x4: &Led12x4) -> Result<()> {
    let on_frame = text_frame(
        ['r', 'u', 's', 't'],
        [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
    );
    led_12x4
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led12x4::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Frame builder that implements DrawTarget for embedded-graphics.
/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led_12x4: &Led12x4) -> Result<()> {
    let mut frame = Led12x4::new_frame();

    // Draw red rectangle border
    Rectangle::new(
        Point::new(0, 0),
        Size::new(Led12x4::COLS as u32, Led12x4::ROWS as u32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
    .draw(&mut frame)
    .map_err(|_| serials::Error::FormatError)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(
        Point::new(0, 0),
        Point::new((Led12x4::COLS - 1) as i32, (Led12x4::ROWS - 1) as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame)
    .map_err(|_| serials::Error::FormatError)?;

    Line::new(
        Point::new(0, (Led12x4::ROWS - 1) as i32),
        Point::new((Led12x4::COLS - 1) as i32, 0),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame)
    .map_err(|_| serials::Error::FormatError)?;

    led_12x4.write_frame(frame).await
}

/// Bouncing dot manually updating frames with write_frame in a loop.
async fn demo_bouncing_dot_manual(led_12x4: &Led12x4) -> Result<()> {
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
        let mut frame = Led12x4::new_frame();
        frame[row_index as usize][column_index as usize] = COLORS[color_index];
        led_12x4.write_frame(frame).await?;

        column_index = column_index + delta_column;
        row_index = row_index + delta_row;

        if column_index >= Led12x4::COLS as isize {
            column_index = (Led12x4::COLS as isize) - 2;
            delta_column = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if column_index < 0 {
            column_index = 1;
            delta_column = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        if row_index >= Led12x4::ROWS as isize {
            row_index = (Led12x4::ROWS as isize) - 2;
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
async fn demo_bouncing_dot_animation(led_12x4: &Led12x4) -> Result<()> {
    const COLORS: [RGB8; 6] = [
        colors::RED,
        colors::GREEN,
        colors::BLUE,
        colors::YELLOW,
        colors::CYAN,
        colors::MAGENTA,
    ];

    let mut frames = Vec::<(Frame, Duration), 32>::new();
    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..32 {
        let mut frame = Led12x4::new_frame();
        frame[row_index as usize][column_index as usize] = COLORS[color_index];
        frames
            .push((frame, Duration::from_millis(50)))
            .map_err(|_| serials::Error::FormatError)?;

        column_index = column_index + delta_column;
        row_index = row_index + delta_row;

        if column_index >= Led12x4::COLS as isize {
            column_index = (Led12x4::COLS as isize) - 2;
            delta_column = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if column_index < 0 {
            column_index = 1;
            delta_column = 1;
            color_index = (color_index + 1) % COLORS.len();
        }

        if row_index >= Led12x4::ROWS as isize {
            row_index = (Led12x4::ROWS as isize) - 2;
            delta_row = -1;
            color_index = (color_index + 1) % COLORS.len();
        } else if row_index < 0 {
            row_index = 1;
            delta_row = 1;
            color_index = (color_index + 1) % COLORS.len();
        }
    }

    led_12x4.animate(&frames).await
}
