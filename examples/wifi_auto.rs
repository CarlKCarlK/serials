// //! Example showing how a future `WifiAuto` abstraction could drive
// //! Wi-Fi onboarding plus extra settings (timezone, nickname, etc.).
// //!
// //! This file intentionally references APIs that do not exist yet. The goal is to
// //! illustrate how the final ergonomics could look for applications that want to
// //! reuse a shared onboarding state machine while collecting additional data.

// #![cfg(feature = "wifi")]
// #![no_std]
// #![no_main]

// use core::convert::Infallible;
// use defmt::info;
// use defmt_rtt as _;
// use embassy_executor::Spawner;
// use embassy_rp::gpio::{self, Level};
// use embassy_time::Timer;
// use panic_probe as _;
// use serials::button::Button;
// use serials::flash_array::{FlashArray, FlashArrayNotifier, FlashBlock};
// use serials::led4::OutputArray;
// use serials::wifi_auto::{
//     ExtraFieldValue, SelectOption, WifiAuto, WifiAutoConfig, WifiAutoEvent,
//     WifiAutoEventHandler, WifiAutoField, WifiAutoNotifier, WifiAutoRecoveryTrigger,
//     WifiAutoRunConfig, WifiAutoSubmission,
// };
// use serials::{Error, Result};

// #[embassy_executor::main]
// pub async fn main(spawner: Spawner) -> ! {
//     let err = inner_main(spawner).await.unwrap_err();
//     core::panic!("{err}");
// }

// async fn inner_main(spawner: Spawner) -> Result<Infallible> {
//     info!("Starting clock with hypothetical WifiAuto onboarding");
//     let peripherals = embassy_rp::init(Default::default());

//     static FLASH_NOTIFIER: FlashArrayNotifier = FlashArray::<3>::notifier();
//     let [wifi_credentials_flash, timezone_flash, nickname_flash] =
//         FlashArray::new(&FLASH_NOTIFIER, peripherals.FLASH)?;

//     let cells = OutputArray::new([
//         gpio::Output::new(peripherals.PIN_1, Level::High),
//         gpio::Output::new(peripherals.PIN_2, Level::High),
//         gpio::Output::new(peripherals.PIN_3, Level::High),
//         gpio::Output::new(peripherals.PIN_4, Level::High),
//     ]);
//     let segments = OutputArray::new([
//         gpio::Output::new(peripherals.PIN_5, Level::Low),
//         gpio::Output::new(peripherals.PIN_6, Level::Low),
//         gpio::Output::new(peripherals.PIN_7, Level::Low),
//         gpio::Output::new(peripherals.PIN_8, Level::Low),
//         gpio::Output::new(peripherals.PIN_9, Level::Low),
//         gpio::Output::new(peripherals.PIN_10, Level::Low),
//         gpio::Output::new(peripherals.PIN_11, Level::Low),
//         gpio::Output::new(peripherals.PIN_12, Level::Low),
//     ]);

//     static LED4_NOTIFIER: LED4Notifier = Led4::notifier();
//     let led4 = Led4::new(cells, segments, &LED4_NOTIFIER);

//     let mut button = Button::new(peripherals.PIN_13);

//     static WIFI_AUTO_NOTIFIER: WifiAutoNotifier = WifiAuto::notifier();
//     let wifi_auto = WifiAuto::new(
//         p.PIN_23,  // WiFi chip data out
//         p.PIN_25,  // WiFi chip data in
//         p.PIO0,    // PIO for WiFi chip communication
//         p.PIN_24,  // WiFi chip clock
//         p.PIN_29,  // WiFi chip select
//         p.DMA_CH0, // DMA channel for WiFi
//         &button, // TODO can we borrow the button here, but later use it in the clock?
//         &WIFI_AUTO_NOTIFIER, // TODO should this be last?
//         &[WifiAutoField::timezone_dropdown(&timezone_flash),
//           WifiAutoField::text(
//                 "nickname",
//                 "Device nickname",
//                 16,
//                 Some("Clock"),
//                 nickname_flash,
//             )],
//         spawner,
//     )?;

//     // TODO Loop on events from wifi_auto until it is connected.
//     loop {
//         led4.write(['C', 'O', 'N', 'N']).await;
//         let event = wifi_auto.next_event().await;
//         // todo if connected then break
//         // else info! the event
//     }

//     // TODO pull info from the flash blocks an then ...
//     // TODO display the timezone off set on LED4 for 15 seconds
//     // TODO display the nickname on LED4 for 15 seconds

//     // TODO we need to turn wifi_auto into wifi connected and free the button for later use.

//     let mut counter = 0usize;
//     loop {
//     // TODO Now that we are connected, get the time from the internet and display it to info!
//     // TODO await 30 seconds or the button being pressed, or wifi not connected
//     // TODO  if button pressed, increment the counter and display the number to LED4
//     // TODO if time expires, get the time from the internet and display it to info!
//     // TODO if wifi not connected, reset the device
//     }
//     }
