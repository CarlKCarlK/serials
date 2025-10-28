//! Full demonstration of all peripherals: RFID, IR remote, LCD, and Servo
//! - RFID cards control servo position (A=180°, B=135°, C=90°, D=45°)
//! - IR buttons 0-9 set servo to 0°-180° in 20° increments
//! - Other IR buttons reset the card map
//! - LCD displays current status with two-line support
//!
//! Run with: cargo full
#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

use core::convert::Infallible;
use core::fmt::Write;
use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::Pull;
use embassy_rp::pio::Pio;
use heapless::{String, index_map::FnvIndexMap};
use lib::{
    CharLcd, CharLcdNotifier, Clock, ClockEvent, ClockNotifier, ClockState, IrNec, IrNecEvent,
    IrNecNotifier, Result, Rgb, Rfid, RfidEvent, RfidNotifier, Led24x4, define_led_strips,
    servo_a, TimeSync, TimeSyncEvent, TimeSyncNotifier,
};
use panic_probe as _;

define_led_strips! {
    pio: PIO1,
    strips: [
        led_strip0 {
            sm: 0,
            dma: DMA_CH1,
            pin: PIN_2,
            len: 8,
            max_current_ma: 480
        },
        led_strip1 {
            sm: 1,
            dma: DMA_CH4,
            pin: PIN_14,
            len: 48,
            max_current_ma: 2880
        }
    ]
}

const LED_RED_DIM: Rgb = Rgb { r: 32, g: 0, b: 0 };
const LED_GREEN_DIM: Rgb = Rgb { r: 0, g: 32, b: 0 };
const LED_OFF: Rgb = Rgb { r: 0, g: 0, b: 0 };

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Infallible> {
    let p = embassy_rp::init(Default::default());

    // Test servo: sweep angles 0,45,90,135,180 with 1s pause, 2 times
    // GPIO0 is on PWM0 slice, channel A
    info!("Starting servo test...");
    let mut servo = servo_a!(p.PWM_SLICE0, p.PIN_0, 500, 2500); // min=500µs (0°), max=2500µs (180°)
    servo.set_degrees(90);

        // Initialize PIO1 for LED strips (both strips share PIO1)
    let Pio { common, sm0, sm1, .. } = Pio::new(p.PIO1, Pio1Irqs);
    let pio_bus = PIO1_BUS.init_with(|| lib::led_strip::PioBus::new(common));

    static LED_STRIP0_NOTIFIER: led_strip0::Notifier = led_strip0::notifier();
    let mut led_strip0_device = led_strip0::new(
        spawner,
        &LED_STRIP0_NOTIFIER,
        pio_bus,
        sm0,
        p.DMA_CH1.into(),
        p.PIN_2.into(),
    )?;
    let mut led_pixels = [LED_OFF; led_strip0::LEN];
    initialize_led_strip(&mut led_strip0_device, &mut led_pixels).await?;
    let mut led_progress_index: usize = 0;

    static LED_STRIP1_NOTIFIER: led_strip1::Notifier = led_strip1::notifier();
    let led_strip1_device = led_strip1::new(
        spawner,
        &LED_STRIP1_NOTIFIER,
        pio_bus,
        sm1,
        p.DMA_CH4.into(),
        p.PIN_14.into(),
    )?;
    let mut led_24x4 = Led24x4::new(led_strip1_device);
    led_24x4.display_1234().await?;

    // Initialize LCD (GP4=SDA, GP5=SCL)
    static CHAR_LCD_CHANNEL: CharLcdNotifier = CharLcd::notifier();
    let lcd = CharLcd::new(p.I2C0, p.PIN_5, p.PIN_4, &CHAR_LCD_CHANNEL, spawner)?;
    lcd.display(String::<64>::try_from("Starting RFID...").unwrap(), 0).await;

    info!("LCD initialized");

    static CLOCK_NOTIFIER: ClockNotifier = Clock::notifier();
    let clock = Clock::new(&CLOCK_NOTIFIER, spawner);

    static TIME_SYNC_NOTIFIER: TimeSyncNotifier = TimeSync::notifier();
    let time_sync = TimeSync::new(
        &TIME_SYNC_NOTIFIER,
        p.PIN_23, // WiFi power enable
        p.PIN_25, // WiFi chip select
        p.PIO0,   // WiFi PIO block
        p.PIN_24, // WiFi MOSI
        p.PIN_29, // WiFi CLK
        p.DMA_CH0,
        spawner,
    );

    static IR_NEC_NOTIFIER: IrNecNotifier = IrNec::notifier();
    let ir = IrNec::new(
        p.PIN_28,
        Pull::Up, // most 38 kHz IR modules idle HIGH
        &IR_NEC_NOTIFIER,
        spawner,
    )?;

    // Initialize MFRC522 RFID reader device abstraction
    static RFID_NOTIFIER: RfidNotifier = Rfid::notifier();
    let rfid_reader = Rfid::new(
        p.SPI0,         // SPI peripheral
        p.PIN_18,       // SCK (serial clock)
        p.PIN_19,       // MOSI
        p.PIN_16,       // MISO
        p.DMA_CH2,      // DMA channel 2 (leave CH0 for WiFi)
        p.DMA_CH3,      // DMA channel 3
        p.PIN_15,       // CS (chip select)
        p.PIN_17,       // RST (reset)
        &RFID_NOTIFIER, // Event notifier
        spawner,        // Task spawner
    )
    .await?;

    lcd.display(String::<64>::try_from("Scan card...").unwrap(), 0).await;

    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();

    // Track the most recent clock event for display purposes
    let mut latest_time: Option<ClockEvent> = None;

    // Main loop: wait for RFID, IR, clock, or time-sync events
    loop {
        use embassy_futures::select::{Either, select};

        // info!("Waiting for RFID/IR/clock/time-sync events");
        let event = select(
            select(rfid_reader.wait(), ir.wait()),
            select(clock.wait(), time_sync.wait()),
        ).await;
        
        match event {
            Either::First(device_event) => match device_event {
                Either::First(RfidEvent::CardDetected { uid }) => {
                    info!("Card detected");
                    // Look up or assign card name
                    let card_name = card_map.get(&uid).copied().or_else(|| {
                        // Try to assign next letter (A, B, C, D...)
                        #[expect(
                            clippy::arithmetic_side_effects,
                            reason = "Card count limited by map capacity"
                        )]
                        let name = b'A' + card_map.len() as u8;
                        card_map.insert(uid, name).ok().map(|_| name)
                    });

                    // Display result on LCD based on card name
                    if let Some(name) = card_name {
                        let mut text = String::<64>::new();
                        write!(text, "Card {} Seen", name as char).unwrap();
                        append_time_line(&mut text, latest_time);
                        lcd.display(text, 1000).await; // 1 second

                        // Move servo based on card letter
                        match name {
                            b'A' => servo.set_degrees(180),
                            b'B' => servo.set_degrees(135),
                            b'C' => servo.set_degrees(90),
                            b'D' => servo.set_degrees(45),
                            _ => servo.set_degrees(0), // Unknown card
                        }
                    } else {
                        let mut text = String::<64>::new();
                        text.push_str("Unknown Card").unwrap();
                        append_time_line(&mut text, latest_time);
                        lcd.display(text, 1000).await; // 1 second
                        servo.set_degrees(0);
                    }

                    advance_led_progress(&mut led_strip0_device, &mut led_pixels, &mut led_progress_index).await?;
                }
                Either::Second(ir_nec_event) => {
                    // IR button pressed - check if it's 0-9 for servo control, otherwise reset map
                    let IrNecEvent::Press { addr, cmd } = ir_nec_event;
                    info!("IR Press: Addr=0x{:02X} Cmd=0x{:02X}", addr, cmd);

                    // Map button codes to digits 0-9
                    let button_digit = match cmd {
                        0x16 => Some(0),  // Button 0
                        0x0C => Some(1),  // Button 1
                        0x18 => Some(2),  // Button 2
                        0x5E => Some(3),  // Button 3
                        0x08 => Some(4),  // Button 4
                        0x1C => Some(5),  // Button 5
                        0x5A => Some(6),  // Button 6
                        0x42 => Some(7),  // Button 7
                        0x52 => Some(8),  // Button 8
                        0x4A => Some(9),  // Button 9
                        _ => None,        // Any other button
                    };

                    if let Some(digit) = button_digit {
                        // Servo control: angle = digit * 20 (0A° to 180A°)
                        #[expect(
                            clippy::arithmetic_side_effects,
                            reason = "digit is 0-9, so digit*20 is 0-180"
                        )]
                        let angle = digit * 20;
                        servo.set_degrees(angle);
                        
                        let mut text = String::<64>::new();
                        write!(text, "Servo:\n{} degrees", angle).unwrap();
                        lcd.display(text, 1000).await; // 1 second
                    } else {
                        // Any other button: reset the card map
                        info!("IR button pressed, resetting card map");
                        card_map.clear();
                        lcd.display(String::<64>::try_from("Map Reset").unwrap(), 500).await; // 0.5 seconds
                    }
                }
            },
            Either::Second(clock_or_sync_event) => match clock_or_sync_event {
                Either::First(time_info) => {
                    latest_time = Some(time_info);
                    let dt = time_info.datetime;
                    let mm = dt.minute();
                    let ss = dt.second();
                    let chars = [
                        char::from_digit((mm / 10) as u32, 10).unwrap(),
                        char::from_digit((mm % 10) as u32, 10).unwrap(),
                        char::from_digit((ss / 10) as u32, 10).unwrap(),
                        char::from_digit((ss % 10) as u32, 10).unwrap(),
                    ];
                    let red = Rgb { r: 32, g: 0, b: 0 };
                    let green = Rgb { r: 0, g: 32, b: 0 };
                    let blue = Rgb { r: 0, g: 0, b: 32 };
                    let yellow = Rgb { r: 32, g: 32, b: 0 };
                    led_24x4.display(chars, [red, green, blue, yellow]).await?;
                    continue;
                }
                Either::Second(TimeSyncEvent::Success { unix_seconds }) => {
                    info!("Time sync success: unix_seconds={}", unix_seconds.as_i64());
                    clock.set_time(unix_seconds).await;
                    lcd.display(String::<64>::try_from("Synced!").unwrap(), 800).await;
                }
                Either::Second(TimeSyncEvent::Failed(err)) => {
                    info!("Time sync failed: {}", err);
                    lcd.display(String::<64>::try_from("Sync failed").unwrap(), 800).await;
                }
            },
        }

        lcd.display(String::<64>::try_from("Scan card...").unwrap(), 0).await; // 0 = until next message
    }
}

async fn initialize_led_strip(strip: &mut led_strip0::Strip, pixels: &mut [Rgb; led_strip0::LEN]) -> Result<()> {
    for idx in 0..led_strip0::LEN {
        pixels[idx] = if idx == 0 { LED_RED_DIM } else { LED_OFF };
    }
    strip.update_pixels(pixels).await?;
    Ok(())
}



async fn advance_led_progress(
    strip: &mut led_strip0::Strip,
    pixels: &mut [Rgb; led_strip0::LEN],
    current_red: &mut usize,
) -> Result<()> {
    info!("Turning {} to green", *current_red);
    pixels[*current_red] = LED_GREEN_DIM;
    strip.update_pixels(pixels).await?;
    let next = (*current_red + 1) % led_strip0::LEN;
    if next == 0 {
        initialize_led_strip(strip, pixels).await?;
        *current_red = 0;
    } else {
        info!("Turning {} to red", next);
        pixels[next] = LED_RED_DIM;
        strip.update_pixels(pixels).await?;
        *current_red = next;
    }
    Ok(())
}



fn append_time_line(text: &mut String<64>, latest_time: Option<ClockEvent>) {
    match latest_time {
        Some(time_info) => match time_info.state {
            ClockState::Synced => {
                let dt = time_info.datetime;
                write!(text, "\n{:02}:{:02}:{:02}", dt.hour(), dt.minute(), dt.second()).unwrap();
            }
            ClockState::NotSet => {
                text.push_str("\nTime not set").unwrap();
            }
        },
        None => {
            text.push_str("\nTime unknown").unwrap();
        }
    }
}


// BUGBUG cmk: uses unsafe.
// BUGBUG cmk: Led24x4 is not a full virtual device.
// BUGBUG cmk: vs code's problems panel complains about this file.
// BUGBUG cmk: need to build check all examples x all features