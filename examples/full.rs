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
use heapless::{String, index_map::FnvIndexMap};
use lib::{
    CharLcd, CharLcdNotifier, IrNec, IrNecEvent, IrNecNotifier, Result, Rfid, RfidChannels, RfidEvent, servo_a
};
use panic_probe as _;

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

    // Initialize LCD (GP4=SDA, GP5=SCL)
    static CHAR_LCD_CHANNEL: CharLcdNotifier = CharLcd::notifier();
    let lcd = CharLcd::new(p.I2C0, p.PIN_5, p.PIN_4, &CHAR_LCD_CHANNEL, spawner)?;
    lcd.display(String::<64>::try_from("Starting RFID...").unwrap(), 0);

    info!("LCD initialized");

    static IR_NEC_NOTIFIER: IrNecNotifier = IrNec::notifier();
    let ir = IrNec::new(
        p.PIN_28,
        Pull::Up, // most 38 kHz IR modules idle HIGH
        &IR_NEC_NOTIFIER,
        spawner,
    )?;

    // Initialize MFRC522 RFID reader device abstraction
    static RFID_CHANNELS: RfidChannels = Rfid::channels();
    let rfid_reader = Rfid::new(
        p.SPI0,         // SPI peripheral
        p.PIN_18,       // SCK (serial clock)
        p.PIN_19,       // MOSI
        p.PIN_16,       // MISO
        p.DMA_CH0,      // DMA channel 0
        p.DMA_CH1,      // DMA channel 1
        p.PIN_15,       // CS (chip select)
        p.PIN_17,       // RST (reset)
        &RFID_CHANNELS, // Event channels (notifier + command)
        spawner,        // Task spawner
    )
    .await?;

    lcd.display(String::<64>::try_from("Scan card...").unwrap(), 0);

    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();

    // Main loop: wait for RFID events OR IR button press
    loop {
        use embassy_futures::select::{Either, select};

        info!("Wait for either card detection OR IR button press");
        match select(rfid_reader.wait(), ir.wait()).await {
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
                    lcd.display(text, 1000); // 1 second

                    // Move servo based on card letter
                    match name {
                        b'A' => servo.set_degrees(180),
                        b'B' => servo.set_degrees(135),
                        b'C' => servo.set_degrees(90),
                        b'D' => servo.set_degrees(45),
                        _ => servo.set_degrees(0), // Unknown card
                    }
                } else {
                    let text = String::<64>::try_from("Unknown Card\nMap Full").unwrap();
                    lcd.display(text, 1000); // 1 second
                    servo.set_degrees(0);
                }
            }
            Either::First(_) => {
                // ignore other RFID events
                continue;
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
                    // Servo control: angle = digit * 20 (0° to 180°)
                    #[expect(
                        clippy::arithmetic_side_effects,
                        reason = "digit is 0-9, so digit*20 is 0-180"
                    )]
                    let angle = digit * 20;
                    servo.set_degrees(angle);
                    
                    let mut text = String::<64>::new();
                    write!(text, "Servo:\n{} degrees", angle).unwrap();
                    lcd.display(text, 1000); // 1 second
                } else {
                    // Any other button: reset the card map
                    info!("IR button pressed, resetting card map");
                    card_map.clear();
                    lcd.display(String::<64>::try_from("Map Reset").unwrap(), 500); // 0.5 seconds
                }
            }
         }

        lcd.display(String::<64>::try_from("Scan card...").unwrap(), 0); // 0 = until next message
    }
}
