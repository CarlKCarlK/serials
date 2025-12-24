#![no_std]
#![no_main]
use core::convert::Infallible;

use defmt::info;
use defmt_rtt as _;
use device_kit::Result;
use device_kit::led_strip::{LedStrip, LedStripStatic, Milliamps, colors, new_led_strip};
use embassy_executor::Spawner;
use embassy_time::Timer;
use panic_probe as _;

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(_spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    const MAX_CURRENT: Milliamps = Milliamps(500);

    // cmk000 kill this type of
    // cmk000 stripX to led_strip0, etc
    type StripStatic0 = LedStripStatic<8>;
    static STRIP_STATIC_0: StripStatic0 = StripStatic0::new_static();
    let mut led_strip_0 = new_led_strip!(
        &STRIP_STATIC_0, // static resources
        PIN_2,           // data pin
        p.PIO0,          // PIO block
        DMA_CH0,         // DMA channel
        MAX_CURRENT      // max current budget (mA)
    )
    .await;

    type StripStatic1 = LedStripStatic<48>;
    static STRIP_STATIC_1: StripStatic1 = StripStatic1::new_static();
    let mut led_strip_1 = new_led_strip!(
        &STRIP_STATIC_1, // static resources
        PIN_3,           // data pin
        p.PIO1,          // PIO block
        DMA_CH1,         // DMA channel
        MAX_CURRENT      // max current budget (mA)
    )
    .await;

    info!("LED strip demo starting (GPIO2 & GPIO3, VSYS power)");

    let mut state0 = BounceState::<8>::new();
    let mut state1 = BounceState::<48>::new();

    loop {
        state0.update(&mut led_strip_0).await?;
        state1.update(&mut led_strip_1).await?;

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

    async fn update<PIO: embassy_rp::pio::Instance>(
        &mut self,
        led_strip: &mut LedStrip<'static, PIO, N>,
    ) -> Result<()> {
        assert!(self.position < N);
        let mut pixels = [colors::BLACK; N];
        pixels[self.position] = colors::WHITE;
        led_strip.update_pixels(&pixels).await?;
        self.advance();
        Ok(())
    }
}
