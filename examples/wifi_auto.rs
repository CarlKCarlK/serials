//! Minimal example that provisions Wi-Fi credentials using the `WifiAuto`
//! abstraction and displays connection status on a 4-digit LED display.
//!
//! // cmk0 Future iterations should add extra captive-portal widgets (e.g. nickname)
//! // and show how to persist their flash-backed values before
//! // handing control back to the application logic.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_futures::join::join;
use embassy_rp::gpio::{self, Level};
use embassy_time::Timer;
use panic_probe as _;
use serials::button::Button;
use serials::flash_array::{FlashArray, FlashArrayNotifier};
use serials::wifi_config::WifiCredentials;
use serials::led4::{BlinkState, Led4, Led4Notifier, OutputArray};
use serials::wifi_auto::{WifiAuto, WifiAutoEvent, WifiAutoNotifier};
use serials::Result;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting wifi_auto example");
    let peripherals = embassy_rp::init(Default::default());

    static FLASH_NOTIFIER: FlashArrayNotifier = FlashArray::<1>::notifier();
    let [mut wifi_credentials_flash] = FlashArray::new(&FLASH_NOTIFIER, peripherals.FLASH)?;

    let stored_credentials: Option<WifiCredentials> = wifi_credentials_flash.load()?;

    // Boot gesture: if the button is held while powering up, drop into AP mode.
    let button = Button::new(peripherals.PIN_13);
    let force_ap = button.is_pressed();
    if force_ap {
        info!("Force AP: button held at boot");
        wifi_credentials_flash.clear()?;
    }

    let cells = OutputArray::new([
        gpio::Output::new(peripherals.PIN_1, Level::High),
        gpio::Output::new(peripherals.PIN_2, Level::High),
        gpio::Output::new(peripherals.PIN_3, Level::High),
        gpio::Output::new(peripherals.PIN_4, Level::High),
    ]);
    let segments = OutputArray::new([
        gpio::Output::new(peripherals.PIN_5, Level::Low),
        gpio::Output::new(peripherals.PIN_6, Level::Low),
        gpio::Output::new(peripherals.PIN_7, Level::Low),
        gpio::Output::new(peripherals.PIN_8, Level::Low),
        gpio::Output::new(peripherals.PIN_9, Level::Low),
        gpio::Output::new(peripherals.PIN_10, Level::Low),
        gpio::Output::new(peripherals.PIN_11, Level::Low),
        gpio::Output::new(peripherals.PIN_12, Level::Low),
    ]);

    static LED4_NOTIFIER: Led4Notifier = Led4::notifier();
    let led4 = Led4::new(cells, segments, &LED4_NOTIFIER, spawner)?;

    static WIFI_AUTO_NOTIFIER: WifiAutoNotifier = WifiAuto::notifier();
    // cmk0 When type-state support lands, thread the button / extra flash blocks
    // into WifiAuto::new so the provisioning phase can own them temporarily.
    let wifi_auto = WifiAuto::new(
        &WIFI_AUTO_NOTIFIER,
        peripherals.PIN_23,
        peripherals.PIN_25,
        peripherals.PIO0,
        peripherals.PIN_24,
        peripherals.PIN_29,
        peripherals.DMA_CH0,
        wifi_credentials_flash,
        spawner,
    );

    if force_ap {
        if let Some(creds) = stored_credentials {
            wifi_auto.set_default_credentials(creds);
        }
        wifi_auto.force_captive_portal();
    }

    let status_future = async {
        loop {
            match wifi_auto.wait_event().await {
                WifiAutoEvent::CaptivePortalReady => {
                    // cmk0 Consider alternating between CONNECT / URL messaging once multi-state UI lands.
                    led4.write_text(BlinkState::BlinkingAndOn, ['C', 'O', 'N', 'N']);
                }
                WifiAutoEvent::ClientConnecting => {
                    led4.write_text(BlinkState::BlinkingAndOn, ['C', 'N', 'N', ' ']);
                }
                WifiAutoEvent::Connected => {
                    led4.write_text(BlinkState::Solid, ['D', 'O', 'N', 'E']);
                    break;
                }
            }
        }
    };

    let ensure_future = wifi_auto.ensure_connected(spawner);
    let (_status, result) = join(status_future, ensure_future).await;
    result?;

    loop {
        Timer::after_secs(1).await;
    }
}
