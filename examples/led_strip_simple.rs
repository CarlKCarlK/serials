#![no_std]
#![no_main]
#![feature(never_type)]

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::bind_interrupts;
use embassy_rp::clocks::clk_sys_freq;
use embassy_rp::pio::program::{Assembler, JmpCondition, OutDestination, SetDestination, SideSet};
use embassy_rp::pio::{
    Common, Config, FifoJoin, Instance, LoadedProgram, PioPin, ShiftConfig, ShiftDirection,
    StateMachine,
};
use embassy_rp::pio_programs::ws2812::{Grb, RgbColorOrder};
use embassy_sync::blocking_mutex::Mutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::once_lock::OnceLock;
use embassy_time::Timer;
use fixed::types::U24F8;
use panic_probe as _;
use serials::Result;
use smart_leds::{RGB8 as Rgb, colors};
use static_cell::StaticCell;

bind_interrupts!(struct Pio0Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
});

struct PioBus<'d, PIO: Instance> {
    common: Mutex<CriticalSectionRawMutex, core::cell::RefCell<Common<'d, PIO>>>,
    program: OnceLock<LoadedProgram<'d, PIO>>,
}

impl<'d, PIO: Instance> PioBus<'d, PIO> {
    fn new(common: Common<'d, PIO>) -> Self {
        Self {
            common: Mutex::new(core::cell::RefCell::new(common)),
            program: OnceLock::new(),
        }
    }

    fn program(&'static self) -> &'static LoadedProgram<'d, PIO> {
        self.program.get_or_init(|| {
            self.common.lock(|cell| {
                let mut common = cell.borrow_mut();
                load_ws2812_program(&mut *common)
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

type PioPeriph = embassy_rp::peripherals::PIO0;
type DataPin = embassy_rp::peripherals::PIN_2;

fn init_pio_bus(
    pio: embassy_rp::Peri<'static, PioPeriph>,
) -> (
    &'static PioBus<'static, PioPeriph>,
    StateMachine<'static, PioPeriph, 0>,
) {
    static PIO0_BUS: StaticCell<PioBus<'static, PioPeriph>> = StaticCell::new();

    let embassy_rp::pio::Pio { common, sm0, .. } = embassy_rp::pio::Pio::new(pio, Pio0Irqs);
    let bus = PIO0_BUS.init_with(|| PioBus::new(common));
    (bus, sm0)
}

const LEN: usize = 8;
const T1: u8 = 2;
const T2: u8 = 5;
const T3: u8 = 3;
const CYCLES_PER_BIT: u32 = (T1 + T2 + T3) as u32;
const WORST_CASE_MA: u32 = (LEN as u32) * 60;
const MAX_CURRENT_MA: u32 = 50;
const MAX_BRIGHTNESS: u8 = {
    let scaled = (MAX_CURRENT_MA * 255) / WORST_CASE_MA;
    if scaled > 255 { 255 } else { scaled as u8 }
};

fn load_ws2812_program<'d, PIO: Instance>(common: &mut Common<'d, PIO>) -> LoadedProgram<'d, PIO> {
    let side_set = SideSet::new(false, 1, false);
    let mut assembler: Assembler<32> = Assembler::new_with_side_set(side_set);

    let mut wrap_target = assembler.label();
    let mut wrap_source = assembler.label();
    let mut do_zero = assembler.label();
    assembler.set_with_side_set(SetDestination::PINDIRS, 1, 0);
    assembler.bind(&mut wrap_target);
    assembler.out_with_delay_and_side_set(OutDestination::X, 1, T3 - 1, 0);
    assembler.jmp_with_delay_and_side_set(JmpCondition::XIsZero, &mut do_zero, T1 - 1, 1);
    assembler.jmp_with_delay_and_side_set(JmpCondition::Always, &mut wrap_target, T2 - 1, 1);
    assembler.bind(&mut do_zero);
    assembler.nop_with_delay_and_side_set(T2 - 1, 0);
    assembler.bind(&mut wrap_source);

    let program = assembler.assemble_with_wrap(wrap_source, wrap_target);
    common.load_program(&program)
}

type LedStripCommands = Channel<CriticalSectionRawMutex, [Rgb; LEN], 2>;

struct PioWs2812Cpu<'d, P: Instance, const S: usize, const N: usize, ORDER = Grb>
where
    ORDER: RgbColorOrder,
{
    sm: StateMachine<'d, P, S>,
    _order: core::marker::PhantomData<ORDER>,
}

impl<'d, P: Instance, const S: usize, const N: usize, ORDER> PioWs2812Cpu<'d, P, S, N, ORDER>
where
    ORDER: RgbColorOrder,
{
    fn new(
        pio: &mut Common<'d, P>,
        sm: StateMachine<'d, P, S>,
        pin: embassy_rp::Peri<'d, impl PioPin>,
        program: &LoadedProgram<'d, P>,
    ) -> Self {
        let mut cfg = Config::default();

        let out_pin = pio.make_pio_pin(pin);
        cfg.set_out_pins(&[&out_pin]);
        cfg.set_set_pins(&[&out_pin]);
        cfg.use_program(program, &[&out_pin]);

        let clock_freq = U24F8::from_num(clk_sys_freq() / 1000);
        let ws2812_freq = U24F8::from_num(800);
        let bit_freq = ws2812_freq * CYCLES_PER_BIT;
        cfg.clock_divider = clock_freq / bit_freq;

        cfg.fifo_join = FifoJoin::TxOnly;
        cfg.shift_out = ShiftConfig {
            auto_fill: true,
            threshold: 24,
            direction: ShiftDirection::Left,
        };

        let mut sm = sm;
        sm.set_config(&cfg);
        sm.set_enable(true);

        Self {
            sm,
            _order: core::marker::PhantomData,
        }
    }

    async fn write(&mut self, colors: &[Rgb; N]) {
        let mut words = [0u32; N];
        for (idx, color) in colors.iter().enumerate() {
            words[idx] = ORDER::pack(*color);
        }

        let tx = self.sm.tx();
        for word in words {
            tx.wait_push(word).await;
        }

        Timer::after_micros(55).await;
    }
}

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
    bus: &'static PioBus<'static, PioPeriph>,
    sm: StateMachine<'static, PioPeriph, 0>,
    pin: embassy_rp::Peri<'static, DataPin>,
    commands: &'static LedStripCommands,
) -> ! {
    let program = bus.program();
    let mut driver =
        bus.with_common(|common| PioWs2812Cpu::<PioPeriph, 0, LEN>::new(common, sm, pin, program));

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

mod led_strip_simple {
    use super::{DataPin, LedStrip, LedStripStatic, PioPeriph};
    use embassy_executor::Spawner;

    pub const LEN: usize = super::LEN;
    pub type Strip = LedStrip;
    pub type Static = LedStripStatic;

    pub fn new(
        strip_static: &'static Static,
        pio: embassy_rp::Peri<'static, PioPeriph>,
        pin: embassy_rp::Peri<'static, DataPin>,
        spawner: Spawner,
    ) -> serials::Result<Strip> {
        let (bus, sm) = super::init_pio_bus(pio);
        let token = super::led_strip0_driver(bus, sm, pin, strip_static.commands())
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
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<!> {
    let peripherals = embassy_rp::init(Default::default());

    // Choose PIO and data pin here
    let pio = peripherals.PIO0;
    let pin = peripherals.PIN_2;

    static LED_STRIP_STATIC: led_strip_simple::Static = led_strip_simple::new_static();
    let mut led_strip_simple_0 = led_strip_simple::new(&LED_STRIP_STATIC, pio, pin, spawner)?;

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut led_strip_simple_0, hue).await?;

        hue = hue.wrapping_add(3);
        Timer::after_millis(80).await;
    }
}

async fn update_rainbow(strip: &mut led_strip_simple::Strip, base: u8) -> Result<()> {
    let mut pixels = [colors::BLACK; led_strip_simple::LEN];
    for idx in 0..led_strip_simple::LEN {
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
