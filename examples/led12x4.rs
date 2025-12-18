#![no_std]
#![no_main]
#![feature(never_type)]
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
use serials::led12x4::{
    AnimationFrame, COLS, Led12x4Static, LedStrip, Milliamps, ROWS, blink_text_animation, colors,
    new_led12x4,
};
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

// cmk why is there a generic T here?
/// Display "RUST" in 4 different colors using write_text.
async fn demo_text_colors<T>(led_12x4: &serials::led12x4::Led12x4<T>) -> Result<()>
where
    T: LedStrip<{ COLS * ROWS }> + 'static,
{
    led_12x4
        .write_text(
            ['r', 'u', 's', 't'],
            [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        )
        .await
}

/// Blink "RUST" using the blink_text_animation builder.
async fn demo_blink_text<T>(led_12x4: &serials::led12x4::Led12x4<T>) -> Result<()>
where
    T: LedStrip<{ COLS * ROWS }> + 'static,
{
    let frames = blink_text_animation(
        ['r', 'u', 's', 't'],
        [colors::RED, colors::GREEN, colors::BLUE, colors::YELLOW],
        Duration::from_millis(500),
        Duration::from_millis(500),
    );

    led_12x4.animate_frames(frames).await
}

/// Frame builder that implements DrawTarget for embedded-graphics.
struct FrameBuilder {
    image: [[RGB8; COLS]; ROWS],
}

impl FrameBuilder {
    fn new() -> Self {
        let black = RGB8::new(0, 0, 0);
        Self {
            image: [[black; COLS]; ROWS],
        }
    }

    fn build(&self) -> [RGB8; COLS * ROWS] {
        let mut frame = [RGB8::new(0, 0, 0); COLS * ROWS];
        for row_index in 0..ROWS {
            for column_index in 0..COLS {
                frame[serials::led12x4::xy_to_index(column_index, row_index)] =
                    self.image[row_index][column_index];
            }
        }
        frame
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

impl OriginDimensions for FrameBuilder {
    fn size(&self) -> Size {
        Size::new(COLS as u32, ROWS as u32)
    }
}

/// Create a red rectangle border with blue diagonals using embedded-graphics.
async fn demo_rectangle_diagonals_embedded_graphics<T>(
    led_12x4: &serials::led12x4::Led12x4<T>,
) -> Result<()>
where
    T: LedStrip<{ COLS * ROWS }> + 'static,
{
    let mut frame_builder = FrameBuilder::new();

    // Draw red rectangle border
    Rectangle::new(Point::new(0, 0), Size::new(COLS as u32, ROWS as u32))
        .into_styled(PrimitiveStyle::with_stroke(Rgb888::RED, 1))
        .draw(&mut frame_builder)
        .map_err(|_| serials::Error::FormatError)?;

    // Draw blue diagonal lines from corner to corner
    Line::new(
        Point::new(0, 0),
        Point::new((COLS - 1) as i32, (ROWS - 1) as i32),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame_builder)
    .map_err(|_| serials::Error::FormatError)?;

    Line::new(
        Point::new(0, (ROWS - 1) as i32),
        Point::new((COLS - 1) as i32, 0),
    )
    .into_styled(PrimitiveStyle::with_stroke(Rgb888::BLUE, 1))
    .draw(&mut frame_builder)
    .map_err(|_| serials::Error::FormatError)?;

    let frame = frame_builder.build();
    led_12x4.write_frame(frame).await
}

/// Bouncing dot manually updating frames with write_frame in a loop.
async fn demo_bouncing_dot_manual<T>(led_12x4: &serials::led12x4::Led12x4<T>) -> Result<()>
where
    T: LedStrip<{ COLS * ROWS }> + 'static,
{
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
        let mut frame = [black; COLS * ROWS];
        frame[serials::led12x4::xy_to_index(column_index as usize, row_index as usize)] =
            COLORS[color_index];
        led_12x4.write_frame(frame).await?;

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
async fn demo_bouncing_dot_animation<T>(led_12x4: &serials::led12x4::Led12x4<T>) -> Result<()>
where
    T: LedStrip<{ COLS * ROWS }> + 'static,
{
    const COLORS: [RGB8; 6] = [
        colors::RED,
        colors::GREEN,
        colors::BLUE,
        colors::YELLOW,
        colors::CYAN,
        colors::MAGENTA,
    ];

    let black = RGB8::new(0, 0, 0);
    let mut frames = Vec::<AnimationFrame, 32>::new();
    let mut column_index: isize = 0;
    let mut row_index: isize = 0;
    let mut delta_column: isize = 1;
    let mut delta_row: isize = 1;
    let mut color_index: usize = 0;

    for _ in 0..32 {
        let mut frame = [black; COLS * ROWS];
        frame[serials::led12x4::xy_to_index(column_index as usize, row_index as usize)] =
            COLORS[color_index];
        frames
            .push(AnimationFrame::new(frame, Duration::from_millis(50)))
            .map_err(|_| serials::Error::FormatError)?;

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

    led_12x4.animate_frames(frames).await
}
