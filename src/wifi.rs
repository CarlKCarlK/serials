//! WiFi virtual device - manages WiFi connection and network stack

#![allow(clippy::future_not_send, reason = "single-threaded")]

use cyw43::JoinOptions;
use cyw43_pio::{DEFAULT_CLOCK_DIVIDER, PioSpi};
use defmt::*;
use embassy_executor::Spawner;
use embassy_net::{Config, Stack, StackResources};
use embassy_rp::{Peri, bind_interrupts, peripherals};
use embassy_rp::gpio::{Level, Output};
use embassy_rp::peripherals::{DMA_CH0, PIO0};
use embassy_rp::pio::{InterruptHandler, Pio};
use embassy_time::Timer;
use static_cell::StaticCell;

use crate::Result;

// ============================================================================
// WiFi Virtual Device
// ============================================================================

/// WiFi virtual device - manages WiFi connection and provides network stack
pub struct Wifi;

impl Wifi {
    /// Create a new WiFi device, connect to network, and return the network stack
    /// 
    /// This initializes the WiFi hardware, connects to the configured network,
    /// and spawns the necessary background tasks. Returns a static reference to
    /// the network stack that can be used by other devices (NTP, HTTP, etc.).
    pub async fn new(
        pin_23: Peri<'static, peripherals::PIN_23>,
        pin_25: Peri<'static, peripherals::PIN_25>,
        pio0: Peri<'static, PIO0>,
        pin_24: Peri<'static, peripherals::PIN_24>,
        pin_29: Peri<'static, peripherals::PIN_29>,
        dma_ch0: Peri<'static, DMA_CH0>,
        spawner: Spawner,
    ) -> Result<&'static Stack<'static>> {
        // Read WiFi credentials from compile-time environment
        const WIFI_SSID: &str = env!("WIFI_SSID");
        const WIFI_PASS: &str = env!("WIFI_PASS");

        info!("WiFi device initializing");

        // Initialize WiFi hardware
        let fw = cyw43_firmware::CYW43_43439A0;
        let clm = cyw43_firmware::CYW43_43439A0_CLM;

        let pwr = Output::new(pin_23, Level::Low);
        let cs = Output::new(pin_25, Level::High);
        let mut pio = Pio::new(pio0, Irqs);
        let spi = PioSpi::new(
            &mut pio.common,
            pio.sm0,
            DEFAULT_CLOCK_DIVIDER,
            pio.irq0,
            cs,
            pin_24,
            pin_29,
            dma_ch0,
        );

        static STATE: StaticCell<cyw43::State> = StaticCell::new();
        let state = STATE.init(cyw43::State::new());
        let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, fw).await;
        unwrap!(spawner.spawn(wifi_task(runner)));

        control.init(clm).await;
        control
            .set_power_management(cyw43::PowerManagementMode::PowerSave)
            .await;

        // Initialize network stack
        let config = Config::dhcpv4(Default::default());
        let seed = 0x7c8f_3a2e_9d14_6b5a;

        static RESOURCES: StaticCell<StackResources<3>> = StaticCell::new();
        static STACK: StaticCell<Stack<'static>> = StaticCell::new();
        let (stack, runner) = embassy_net::new(
            net_device,
            config,
            RESOURCES.init(StackResources::<3>::new()),
            seed,
        );
        let stack = STACK.init(stack);

        unwrap!(spawner.spawn(net_task(runner)));

        // Connect to WiFi
        info!("Connecting to WiFi: {}", WIFI_SSID);
        loop {
            match control
                .join(WIFI_SSID, JoinOptions::new(WIFI_PASS.as_bytes()))
                .await
            {
                Ok(_) => break,
                Err(err) => {
                    info!("Join failed: {}", err.status);
                    Timer::after_secs(1).await;
                }
            }
        }

        info!("WiFi connected! Waiting for DHCP...");
        stack.wait_config_up().await;

        if let Some(config) = stack.config_v4() {
            info!("IP Address: {}", config.address);
        }

        info!("WiFi device ready");
        Ok(stack)
    }
}

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => InterruptHandler<PIO0>;
});

// ============================================================================
// WiFi Tasks
// ============================================================================

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<'static, Output<'static>, PioSpi<'static, PIO0, 0, DMA_CH0>>,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}
