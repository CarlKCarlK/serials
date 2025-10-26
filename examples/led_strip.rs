#![no_std]
#![no_main]

use cortex_m::{interrupt as cortex_interrupt, register::primask};
use cortex_m_rt::entry;
use defmt::info;
use defmt_rtt as _;
use embedded_hal::delay::DelayNs;
use panic_probe as _;
use rp2040_hal::{
    clocks::{init_clocks_and_plls, Clock},
    gpio::{FunctionPio0, Pins},
    pac,
    pio::PIOExt,
    sio::Sio,
    timer::Timer,
    watchdog::Watchdog,
};
use pac::interrupt;
use smart_leds::{SmartLedsWrite, RGB8};
use ws2812_pio::Ws2812;

struct CriticalSectionImpl;

critical_section::set_impl!(CriticalSectionImpl);

#[expect(unsafe_code, reason = "Boot2 blob must reside in flash at fixed address")]
mod boot2_blob {
    #[unsafe(link_section = ".boot2")]
    #[used]
    pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_W25Q080;
}

#[expect(unsafe_code, reason = "Critical section implementation requires manipulating PRIMASK")]
unsafe impl critical_section::Impl for CriticalSectionImpl {
    unsafe fn acquire() -> critical_section::RawRestoreState {
        let was_disabled = u8::from(primask::read().is_active());
        cortex_interrupt::disable();
        was_disabled
    }

    unsafe fn release(state: critical_section::RawRestoreState) {
        if state == 0 {
            unsafe {
                cortex_interrupt::enable();
            }
        }
    }
}

const XOSC_CRYSTAL_FREQ: u32 = 12_000_000;

#[entry]
fn main() -> ! {
    let mut peripherals = pac::Peripherals::take().unwrap();
    let _core = pac::CorePeripherals::take().unwrap();
    let mut watchdog = Watchdog::new(peripherals.WATCHDOG);

    let clocks = init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ,
        peripherals.XOSC,
        peripherals.CLOCKS,
        peripherals.PLL_SYS,
        peripherals.PLL_USB,
        &mut peripherals.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let sio = Sio::new(peripherals.SIO);
    let pins = Pins::new(
        peripherals.IO_BANK0,
        peripherals.PADS_BANK0,
        sio.gpio_bank0,
        &mut peripherals.RESETS,
    );

    let mut timer = Timer::new(peripherals.TIMER, &mut peripherals.RESETS, &clocks);

    let (mut pio, sm0, _, _, _) = peripherals.PIO0.split(&mut peripherals.RESETS);
    let mut strip = Ws2812::new(
        pins.gpio2.into_function::<FunctionPio0>(),
        &mut pio,
        sm0,
        clocks.peripheral_clock.freq(),
        timer.count_down(),
    );

    info!("LED strip demo starting (GPIO2 data, VSYS power)");

    let mut frame = [RGB8::default(); 8];
    let mut hue: u8 = 0;

    loop {
        update_rainbow(&mut frame, hue);
        strip.write(frame.iter().copied()).unwrap();

        hue = hue.wrapping_add(3);
        timer.delay_ns(120_000_000);
    }
}

fn update_rainbow(buf: &mut [RGB8], base: u8) {
    for (idx, led) in buf.iter_mut().enumerate() {
        let offset = base.wrapping_add((idx as u8).wrapping_mul(16));
        *led = wheel(offset);
    }
}

fn wheel(pos: u8) -> RGB8 {
    let pos = 255 - pos;
    if pos < 85 {
        RGB8::new(255 - pos * 3, 0, pos * 3)
    } else if pos < 170 {
        let pos = pos - 85;
        RGB8::new(0, pos * 3, 255 - pos * 3)
    } else {
        let pos = pos - 170;
        RGB8::new(pos * 3, 255 - pos * 3, 0)
    }
}

// Dummy handlers for software interrupts not used in this demo but required by rp2040-pac vector table.
#[allow(non_snake_case)]
#[cortex_m_rt::interrupt]
fn SW0_IRQ() {}

#[allow(non_snake_case)]
#[cortex_m_rt::interrupt]
fn SW1_IRQ() {}

#[allow(non_snake_case)]
#[cortex_m_rt::interrupt]
fn SW2_IRQ() {}

#[allow(non_snake_case)]
#[cortex_m_rt::interrupt]
fn SW3_IRQ() {}

#[allow(non_snake_case)]
#[cortex_m_rt::interrupt]
fn SW4_IRQ() {}

#[allow(non_snake_case)]
#[cortex_m_rt::interrupt]
fn SW5_IRQ() {}
