#![no_std]
#![no_main]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::pio::{Common, Instance, StateMachine};
use embassy_rp::pio_programs::ws2812::{PioWs2812, PioWs2812Program};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::once_lock::OnceLock;
use embassy_time::Timer;
use panic_probe as _;
use serials::Result;
use smart_leds::{RGB8 as Rgb, colors};
use static_cell::StaticCell;

bind_interrupts!(struct Pio0Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

struct PioBus<'d, PIO: Instance> {
    common: Mutex<CriticalSectionRawMutex, core::cell::RefCell<Common<'d, PIO>>>,
    program: OnceLock<PioWs2812Program<'d, PIO>>,
}

impl<'d, PIO: Instance> PioBus<'d, PIO> {
    fn new(common: Common<'d, PIO>) -> Self {
        Self {
            common: Mutex::new(core::cell::RefCell::new(common)),
            program: OnceLock::new(),
        }
    }

    fn program(&'static self) -> &'static PioWs2812Program<'d, PIO> {
        self.program.get_or_init(|| {
            self.common.lock(|cell| {
                let mut common = cell.borrow_mut();
                PioWs2812Program::new(&mut *common)
            })
        })
    }

    fn with_common<R>(&self, f: impl FnOnce(&mut Common<'d, PIO>) -> R) -> R {
        self.common.lock(|cell| {
            let mut common = cell.borrow_mut();
            f(&mut *common)
        })
    }
}

fn pio0_split(
    pio: embassy_rp::Peri<'static, embassy_rp::peripherals::PIO0>,
) -> (
    &'static PioBus<'static, embassy_rp::peripherals::PIO0>,
    StateMachine<'static, embassy_rp::peripherals::PIO0, 0>,
    StateMachine<'static, embassy_rp::peripherals::PIO0, 1>,
    StateMachine<'static, embassy_rp::peripherals::PIO0, 2>,
    StateMachine<'static, embassy_rp::peripherals::PIO0, 3>,
) {
    static PIO0_BUS: StaticCell<PioBus<'static, embassy_rp::peripherals::PIO0>> = StaticCell::new();

    let embassy_rp::pio::Pio {
        common,
        sm0,
        sm1,
        sm2,
        sm3,
        ..
    } = embassy_rp::pio::Pio::new(pio, Pio0Irqs);
    let bus = PIO0_BUS.init_with(|| PioBus::new(common));
    (bus, sm0, sm1, sm2, sm3)
}

const LEN: usize = 8;
const WORST_CASE_MA: u32 = (LEN as u32) * 60;
const MAX_CURRENT_MA: u32 = 50;
const MAX_BRIGHTNESS: u8 = {
    let scaled = (MAX_CURRENT_MA * 255) / WORST_CASE_MA;
    if scaled > 255 { 255 } else { scaled as u8 }
};

type LedStripCommands = Channel<CriticalSectionRawMutex, [Rgb; LEN], 2>;

struct LedStripStatic {
    commands: LedStripCommands,
}

impl LedStripStatic {
    const fn new_static() -> Self {
        Self {
            commands: Channel::new(),
        }
    }

    fn commands(&'static self) -> &'static LedStripCommands {
        &self.commands
    }
}

struct LedStrip {
    commands: &'static LedStripCommands,
}

impl LedStrip {
    const fn new_static() -> LedStripStatic {
        LedStripStatic::new_static()
    }

    fn new(led_strip_static: &'static LedStripStatic) -> Result<Self> {
        Ok(Self {
            commands: led_strip_static.commands(),
        })
    }

    async fn update_pixels(&mut self, pixels: &[Rgb; LEN]) -> Result<()> {
        self.commands.send(*pixels).await;
        embassy_time::Timer::after_micros((LEN as u64 * 30) + 100).await;
        Ok(())
    }
}

#[embassy_executor::task]
async fn led_strip0_driver(
    bus: &'static PioBus<'static, embassy_rp::peripherals::PIO0>,
    sm: StateMachine<'static, embassy_rp::peripherals::PIO0, 0>,
    dma: embassy_rp::Peri<'static, embassy_rp::peripherals::DMA_CH0>,
    pin: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_2>,
    commands: &'static LedStripCommands,
) -> ! {
    let program = bus.program();
    let mut driver = bus.with_common(|common| {
        PioWs2812::<embassy_rp::peripherals::PIO0, 0, LEN, _>::new(common, sm, dma, pin, program)
    });

    loop {
        let mut frame = commands.receive().await;
        for color in frame.iter_mut() {
            *color = Rgb::new(
                scale_brightness(color.r, MAX_BRIGHTNESS),
                scale_brightness(color.g, MAX_BRIGHTNESS),
                scale_brightness(color.b, MAX_BRIGHTNESS),
            );
        }
        driver.write(&frame).await;
    }
}

fn scale_brightness(value: u8, brightness: u8) -> u8 {
    ((u16::from(value) * u16::from(brightness)) / 255) as u8
}

mod led_strip0 {
    use super::{LedStrip, LedStripStatic, PioBus, StateMachine};
    use embassy_executor::Spawner;

    pub const LEN: usize = super::LEN;
    pub type Strip = LedStrip;
    pub type Static = LedStripStatic;

    pub fn new(
        spawner: Spawner,
        strip_static: &'static Static,
        bus: &'static PioBus<'static, embassy_rp::peripherals::PIO0>,
        sm: StateMachine<'static, embassy_rp::peripherals::PIO0, 0>,
        dma: embassy_rp::Peri<'static, embassy_rp::peripherals::DMA_CH0>,
        pin: embassy_rp::Peri<'static, embassy_rp::peripherals::PIN_2>,
    ) -> serials::Result<Strip> {
        let token = super::led_strip0_driver(bus, sm, dma, pin, strip_static.commands())
            .map_err(serials::Error::TaskSpawn)?;
        spawner.spawn(token);
        Strip::new(strip_static)
    }

    pub const fn new_static() -> Static {
        Strip::new_static()
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) -> ! {
    let peripherals = embassy_rp::init(Default::default());

    // Initialize PIO0 bus
    let (pio_bus, sm0, _sm1, _sm2, _sm3) = pio0_split(peripherals.PIO0);

    static LED_STRIP_STATIC: led_strip0::Static = led_strip0::new_static();
    let mut led_strip_0 = led_strip0::new(
        spawner,
        &LED_STRIP_STATIC,
        pio_bus,
        sm0,
        peripherals.DMA_CH0.into(),
        peripherals.PIN_2.into(),
    )
    .expect("Failed to start LED strip");

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip_0, hue)
            .await
            .expect("pattern update failed");

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(strip: &mut led_strip0::Strip, base: u8) -> Result<()> {
    let mut pixels = [colors::BLACK; led_strip0::LEN];
    for idx in 0..led_strip0::LEN {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        pixels[idx] = wheel(offset);
    }
    strip.update_pixels(&pixels).await?;
    Ok(())
}

fn wheel(pos: u8) -> Rgb {
    let pos = 255 - pos;
    if pos < 85 {
        rgb(255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        rgb(0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        rgb(pos * 3, 255 - pos * 3, 0)
    }
}

const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
    Rgb { r, g, b }
}
