#![no_std]
#![no_main]
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::define_led_strips;
use device_kit::led_strip::{Current, Frame, LedStrip, colors};
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

const MAX_CURRENT: Current = Current::Milliamps(500);

define_led_strips! {
    LedStripsPio0 {
        gpio2: { pin: PIN_2, len: 8, max_current: MAX_CURRENT }
    }
}

define_led_strips! {
    pio: PIO1,
    LedStripsPio1 {
        gpio3: { dma: DMA_CH1, pin: PIN_3, len: 48, max_current: MAX_CURRENT }
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    let (gpio2_led_strip,) = LedStripsPio0::new(p.PIO0, p.DMA_CH0, p.PIN_2, spawner)?;
    let (gpio3_led_strip,) = LedStripsPio1::new(p.PIO1, p.DMA_CH1, p.PIN_3, spawner)?;

    info!("LED strip demo starting (GPIO2 & GPIO3, VSYS power)");

    let mut state0 = BounceState::<8>::new();
    let mut state1 = BounceState::<48>::new();

    loop {
        state0.update(gpio2_led_strip).await?;
        state1.update(gpio3_led_strip).await?;

        Timer::after_millis(500).await;
    }
}

struct BounceState<const N: usize> {
    position: usize,
    direction: isize,
}

impl<const N: usize> BounceState<N> {
    const fn new() -> Self {
        Self {
            position: 0,
            direction: 1,
        }
    }

    fn advance(&mut self) {
        if self.direction > 0 {
            if self.position >= N - 1 {
                self.direction = -1;
            } else {
                self.position += 1;
            }
        } else if self.position == 0 {
            self.direction = 1;
        } else {
            self.position -= 1;
        }
    }

    async fn update<const MAX_FRAMES: usize>(
        &mut self,
        led_strip: &impl core::ops::Deref<Target = LedStrip<N, MAX_FRAMES>>,
    ) -> Result<()> {
        assert!(self.position < N);
        let mut frame = Frame::<N>::new();
        frame[self.position] = colors::WHITE;
        led_strip.write_frame(frame).await?;
        self.advance();
        Ok(())
    }
}
