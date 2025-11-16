//! Minimal example that provisions Wi-Fi credentials using the `WifiAuto`
//! abstraction and displays connection status on a 4-digit LED display.
//!
//! // cmk0 Future iterations should add extra captive-portal widgets (e.g. nickname)
//! // and show how to persist their flash-backed values before handing control back
//! // to the application logic.

#![cfg(feature = "wifi")]
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "single-threaded")]

use core::convert::Infallible;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{self, Level};
use embassy_time::{Duration, Timer};
use panic_probe as _;
use serials::Result;
use serials::flash_array::{FlashArray, FlashArrayNotifier};
use serials::led4::{AnimationFrame, BlinkState, Led4, Led4Animation, Led4Notifier, OutputArray};
use serials::wifi_auto::{WifiAuto, WifiAutoEvent, WifiAutoNotifier};

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    let err = inner_main(spawner).await.unwrap_err();
    core::panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    info!("Starting wifi_auto example");
    let peripherals = embassy_rp::init(Default::default());

    static FLASH_NOTIFIER: FlashArrayNotifier = FlashArray::<1>::notifier();
    let [wifi_credentials_flash] = FlashArray::new(&FLASH_NOTIFIER, peripherals.FLASH)?;

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
    let wifi_auto = WifiAuto::new(
        &WIFI_AUTO_NOTIFIER,
        peripherals.PIN_23,     // CYW43 power
        peripherals.PIN_25,     // CYW43 chip select
        peripherals.PIO0,       // CYW43 PIO interface
        peripherals.PIN_24,     // CYW43 clock
        peripherals.PIN_29,     // CYW43 data pin
        peripherals.DMA_CH0,    // CYW43 DMA channel
        wifi_credentials_flash, // Flash block storing Wi-Fi creds
        peripherals.PIN_13,     // User button pin
        "Pico",                 // Captive-portal SSID to display
        spawner,
    )?;

    wifi_auto
        .ensure_connected_with_ui(spawner, |event| match event {
            WifiAutoEvent::CaptivePortalReady => {
                led4.write_text(BlinkState::BlinkingAndOn, ['C', 'O', 'N', 'N']);
            }

            WifiAutoEvent::ClientConnecting { try_index, .. } => {
                led4.animate_text(circular_outline_animation((try_index & 1) == 0));
            }

            WifiAutoEvent::Connected => {
                led4.write_text(BlinkState::Solid, ['D', 'O', 'N', 'E']);
            }
        })
        .await?;

    let _button = wifi_auto.take_button();    loop {
        Timer::after_secs(1).await;
    }
}

fn circular_outline_animation(clockwise: bool) -> Led4Animation {
    const FRAME_DURATION: Duration = Duration::from_millis(120);
    const CLOCKWISE: [[char; 4]; 8] = [
        ['\'', '\'', '\'', '\''],
        ['\'', '\'', '\'', '"'],
        [' ', ' ', ' ', '>'],
        [' ', ' ', ' ', ')'],
        ['_', '_', '_', '_'],
        ['*', '_', '_', '_'],
        ['<', ' ', ' ', ' '],
        ['(', '\'', '\'', '\''],
    ];
    const COUNTER: [[char; 4]; 8] = [
        ['(', '\'', '\'', '\''],
        ['<', ' ', ' ', ' '],
        ['*', '_', '_', '_'],
        ['_', '_', '_', '_'],
        [' ', ' ', ' ', ')'],
        [' ', ' ', ' ', '>'],
        ['\'', '\'', '\'', '"'],
        ['\'', '\'', '\'', '\''],
    ];

    let mut animation = Led4Animation::new();
    let frames = if clockwise { &CLOCKWISE } else { &COUNTER };
    for text in frames {
        let _ = animation.push(AnimationFrame::new(*text, FRAME_DURATION));
    }
    animation
}
