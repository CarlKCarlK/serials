#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::select::{Either, select};
use embassy_rp::init;
use embassy_time::{Duration, Timer};
use heapless::Vec;
use panic_probe as _;
use serials::button::{Button, PressedTo};
use serials::led_strip_simple::Milliamps;
use serials::led2d::{Led2dFont, led2d_device_simple};
use serials::{Error, Result};
use smart_leds::colors;

// Create the led2d device using the macro - 12x8 screen (two 12x4 stacked)
led2d_device_simple! {
    pub led8x12,
    rows: 8,
    cols: 12,
    pio: PIO0,
    mapping: arbitrary([
        0, 7, 8, 15, 16, 23, 24, 31, 32, 39, 40, 47,
        1, 6, 9, 14, 17, 22, 25, 30, 33, 38, 41, 46,
        2, 5, 10, 13, 18, 21, 26, 29, 34, 37, 42, 45,
        3, 4, 11, 12, 19, 20, 27, 28, 35, 36, 43, 44,
        48, 55, 56, 63, 64, 71, 72, 79, 80, 87, 88, 95,
        49, 54, 57, 62, 65, 70, 73, 78, 81, 86, 89, 94,
        50, 53, 58, 61, 66, 69, 74, 77, 82, 85, 90, 93,
        51, 52, 59, 60, 67, 68, 75, 76, 83, 84, 91, 92,
    ]),
    font: Led2dFont::Font3x4,
}

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("LED 2D API Exploration (12x8 display)");
    let p = init(Default::default());

    static LED8X12_STATIC: Led8x12Static = Led8x12::new_static();
    let led8x12 = Led8x12::new(&LED8X12_STATIC, p.PIO0, p.PIN_4, Milliamps(1000), spawner).await?;

    let mut button = Button::new(p.PIN_13, PressedTo::Ground);

    loop {
        info!("Demo 1: 3x4 font (\"RUST\" in four colors)");
        demo_rust_text(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 2: Blink text (\"RUST\")");
        demo_blink_text(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 3: Colored corners");
        demo_colored_corners(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 4: Blink pattern");
        demo_blink_pattern(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 5: Rectangle with diagonals (embedded-graphics)");
        demo_rectangle_diagonals_embedded_graphics(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 6: Bouncing dot (manual frames)");
        demo_bouncing_dot_manual(&led8x12, &mut button).await?;

        info!("Demo 7: Bouncing dot (animation)");
        demo_bouncing_dot_animation(&led8x12).await?;
        button.wait_for_press_duration().await;

        info!("Demo 8: Scrolling text");
        demo_scrolling_text(&led8x12).await?;
        button.wait_for_press_duration().await;
    }
}

/// Display "RUST" using the bit_matrix3x4 font via embedded-graphics.
async fn demo_rust_text(led8x12: &Led8x12) -> Result<()> {
    let colors = [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW];
    led8x12.write_text("RUST\ntwo", &colors).await
}

/// Blink "RUST" by constructing frames explicitly.
async fn demo_blink_text(led8x12: &Led8x12) -> Result<()> {
    let mut on_frame = Led8x12::new_frame();
    led8x12.write_text_to_frame(
        "rust",
        &[colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        &mut on_frame,
    )?;
    led8x12
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led8x12::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Display colored corners to demonstrate coordinate mapping.
async fn demo_colored_corners(led8x12: &Led8x12) -> Result<()> {
    // Four corners with different colors
    let mut frame = Led8x12::new_frame();
    frame[0][0] = colors::RED; // Top-left
    frame[0][Led8x12::COLS - 1] = colors::GREEN; // Top-right
    frame[Led8x12::ROWS - 1][0] = colors::BLUE; // Bottom-left
    frame[Led8x12::ROWS - 1][Led8x12::COLS - 1] = colors::YELLOW; // Bottom-right

    led8x12.write_frame(frame).await?;
    Ok(())
}

/// Blink a pattern by constructing frames explicitly.
async fn demo_blink_pattern(led8x12: &Led8x12) -> Result<()> {
    // Create checkerboard pattern
    let mut on_frame = Led8x12::new_frame();
    for row_index in 0..Led8x12::ROWS {
        for column_index in 0..Led8x12::COLS {
            if (row_index + column_index) % 2 == 0 {
                on_frame[row_index][column_index] = colors::CYAN;
            }
        }
    }

    led8x12
        .animate(&[
            (on_frame, Duration::from_millis(500)),
            (Led8x12::new_frame(), Duration::from_millis(500)),
        ])
        .await
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics(led8x12: &Led8x12) -> Result<()> {
    use embedded_graphics::{
        Drawable,
        pixelcolor::Rgb888,
        prelude::*,
        primitives::{Line, PrimitiveStyle, Rectangle},
    };
    use serials::led2d::Frame;

    let mut frame = Led8x12::new_frame();

    // Use the embedded_graphics crate to draw an image.

    // Draw red rectangle border
    Rectangle::new(Frame::<8, 12>::top_left(), Frame::<8, 12>::size())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
        .draw(&mut frame)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(Frame::<8, 12>::top_left(), Frame::<8, 12>::bottom_right())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
        .draw(&mut frame)?;

    Line::new(Frame::<8, 12>::bottom_left(), Frame::<8, 12>::top_right())
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
        .draw(&mut frame)?;

    led8x12.write_frame(frame).await
}

async fn demo_bouncing_dot_manual(led8x12: &Led8x12, button: &mut Button<'_>) -> Result<()> {
    let mut color_cycle = [colors::RED, colors::GREEN, colors::BLUE].iter().cycle();

    // Steps one position coordinate and reports if it hit an edge.
    fn step_and_hit(position: &mut isize, velocity: &mut isize, limit: isize) -> bool {
        *position += *velocity;
        if (0..limit).contains(position) {
            return false;
        }
        *velocity = -*velocity;
        *position += *velocity; // step back inside
        true
    }

    let (mut x, mut y) = (0isize, 0isize);
    let (mut vx, mut vy) = (1isize, 1isize);
    let (x_limit, y_limit) = (Led8x12::COLS as isize, Led8x12::ROWS as isize);
    let mut color = *color_cycle.next().unwrap(); // Safe: cycle() over a non-empty array never returns None

    loop {
        let mut frame = Led8x12::new_frame();
        frame[y as usize][x as usize] = color;
        led8x12.write_frame(frame).await?;

        if step_and_hit(&mut x, &mut vx, x_limit) | step_and_hit(&mut y, &mut vy, y_limit) {
            color = *color_cycle.next().unwrap();
        }

        if let Either::Second(_) = select(Timer::after_millis(50), button.wait_for_press()).await {
            break;
        }
    }

    Ok(())
}

/// Bouncing dot using pre-built animation frames.
async fn demo_bouncing_dot_animation(led8x12: &Led8x12) -> Result<()> {
    use serials::led2d::ANIMATION_MAX_FRAMES;

    let mut color_cycle = [colors::CYAN, colors::YELLOW, colors::LIME].iter().cycle();

    // Steps one position coordinate and reports if it hit an edge.
    fn step_and_hit(position: &mut isize, velocity: &mut isize, limit: isize) -> bool {
        *position += *velocity;
        if (0..limit).contains(position) {
            return false;
        }
        *velocity = -*velocity;
        *position += *velocity; // step back inside
        true
    }

    let mut frames = Vec::<_, ANIMATION_MAX_FRAMES>::new();
    let (mut x, mut y) = (0isize, 0isize);
    let (mut vx, mut vy) = (1isize, 1isize);
    let (x_limit, y_limit) = (Led8x12::COLS as isize, Led8x12::ROWS as isize);
    let mut color = *color_cycle.next().unwrap();

    for _ in 0..ANIMATION_MAX_FRAMES {
        let mut frame = Led8x12::new_frame();
        frame[y as usize][x as usize] = color;
        frames
            .push((frame, Duration::from_millis(50)))
            .map_err(|_| Error::FormatError)?;

        if step_and_hit(&mut x, &mut vx, x_limit) | step_and_hit(&mut y, &mut vy, y_limit) {
            color = *color_cycle.next().unwrap();
        }
    }

    led8x12.animate(&frames).await
}

/// Scrolling text demonstration - takes advantage of the taller 8-row display.
async fn demo_scrolling_text(led8x12: &Led8x12) -> Result<()> {
    let colors = [colors::MAGENTA, colors::ORANGE];

    // Display two lines of text that can fit vertically
    led8x12.write_text("HELLO\nWORLD", &colors).await
}
