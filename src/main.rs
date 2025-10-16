#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

mod servo;
use servo::Servo;
use lib::ONE_SECOND;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Pull};
use embassy_rp::pwm::{Pwm, Config};
use embassy_time::{Duration, Timer};
use heapless::index_map::FnvIndexMap;
use lib::{CharLcdI2c, Never, Result, RfidEvent, SpiMfrc522Channels, SpiMfrc522Reader};
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner0: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner0).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Never> {
    let p = embassy_rp::init(Default::default());

    // Test servo: sweep angles 0,45,90,135,180 with 1s pause, 2 times
    // GPIO0 is on PWM0 slice, channel A
    // NOTE: PWM divider is NOT set here - it's set at runtime by embassy
    // For servo timing (1000-2000µs), we need ~ 1 MHz PWM ticks (50 Hz clock needs 20ms = 20,000 ticks)
    info!("Starting servo test...");
    let pwm_cfg = Config::default();
    let pwm = Pwm::new_output_a(p.PWM_SLICE0, p.PIN_0, pwm_cfg);
    let mut servo = Servo::new(pwm, 1000, 2000);  // min=1000µs (0°), max=2000µs (180°)
    info!("Servo created, setting to 90 degrees");
    servo.set_degrees(90);
    info!("Servo at 90, waiting 5 seconds...");
    Timer::after(Duration::from_secs(5)).await;
    for i in 0..2 {
        info!("Sweep iteration {}", i);
        servo.set_degrees(0);
        Timer::after(ONE_SECOND).await;
        servo.set_degrees(45);
        Timer::after(ONE_SECOND).await;
        servo.set_degrees(90);
        Timer::after(ONE_SECOND).await;
        servo.set_degrees(135);
        Timer::after(ONE_SECOND).await;
        servo.set_degrees(180);
        Timer::after(ONE_SECOND).await;
    }
    servo.disable();
    info!("Servo test complete!");

    // Initialize LCD (GP4=SDA, GP5=SCL)
    let mut lcd = CharLcdI2c::new(p.I2C0, p.PIN_5, p.PIN_4).await;
    lcd.clear().await;
    lcd.print("Hello").await;
    
    info!("LCD initialized and displaying Hello");

    // Initialize IR receiver (GPIO 6)
    let mut ir_pin = Input::new(p.PIN_6, Pull::Up);
    info!("IR receiver initialized on GPIO 6");

    // Initialize MFRC522 RFID reader device abstraction
    static RFID_CHANNELS: SpiMfrc522Channels = SpiMfrc522Reader::channels();
    let rfid_reader = SpiMfrc522Reader::new(
        p.SPI0,
        p.PIN_18,
        p.PIN_19,
        p.PIN_16,
        p.DMA_CH0,
        p.DMA_CH1,
        p.PIN_15,
        p.PIN_17,
        &RFID_CHANNELS,
        spawner,
    ).await?;
    
    lcd.clear().await;
    lcd.print("Scan card...").await;
    
    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();
    
    // Main loop: wait for RFID events OR IR button press
    loop {
        use embassy_futures::select::{select, Either};
        
        // Wait for either card detection OR IR button press
        match select(rfid_reader.next_event(), ir_pin.wait_for_low()).await {
            Either::First(RfidEvent::CardDetected { uid }) => {
                // Look up or assign card name
                let card_name = card_map.get(&uid).copied().or_else(|| {
                    // Try to assign next letter (A, B, C, D...)
                    #[expect(clippy::arithmetic_side_effects, reason = "Card count limited by map capacity")]
                    let name = b'A' + card_map.len() as u8;
                    card_map.insert(uid, name).ok().map(|_| name)
                });
                
                // Display result on LCD based on card name
                lcd.clear().await;
                
                if let Some(name) = card_name {
                    lcd.print("Card ").await;
                    lcd.write_byte(name).await;
                    lcd.print(" Seen").await;
                } else {
                    lcd.print("Unknown Card").await;
                    lcd.set_cursor(1, 0).await;
                    lcd.print("Map Full").await;
                }
                
                Timer::after_millis(2000).await;
            }
            Either::First(_) => {
                // ignore other RFID events
                continue;
            }
            Either::Second(()) => {
                // IR button pressed - reset the card map
                info!("IR button pressed, resetting card map");
                card_map.clear();
                
                lcd.clear().await;
                lcd.print("Map Reset").await;
                
                // Wait for button release to avoid repeated triggers
                ir_pin.wait_for_high().await;
                Timer::after_millis(500).await;
            }
        }
        
        lcd.clear().await;
        lcd.print("Scan card...").await;
        
        Timer::after_millis(100).await;
    }
}

