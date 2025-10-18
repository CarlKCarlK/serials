#![no_std]
#![no_main]
#![allow(clippy::future_not_send, reason = "Single-threaded")]

mod servo;

use defmt::info;
use defmt_rtt as _;
use embassy_executor::Spawner;
use embassy_rp::gpio::Pull;
use embassy_time::Timer;
use heapless::index_map::FnvIndexMap;
use lib::{
    CharLcdI2c, IrNec, IrNecEvent, IrNecNotifier, Never, Result, RfidEvent, SpiMfrc522Channels,
    SpiMfrc522Reader,
};
// This crate's own internal library
use panic_probe as _;

#[embassy_executor::main]
pub async fn main(spawner: Spawner) -> ! {
    // If it returns, something went wrong.
    let err = inner_main(spawner).await.unwrap_err();
    panic!("{err}");
}

async fn inner_main(spawner: Spawner) -> Result<Never> {
    let p = embassy_rp::init(Default::default());

    // Test servo: sweep angles 0,45,90,135,180 with 1s pause, 2 times
    // GPIO0 is on PWM0 slice, channel A
    info!("Starting servo test...");
    let mut servo = servo_a!(p.PWM_SLICE0, p.PIN_0, 500, 2500); // min=500µs (0°), max=2500µs (180°)
    servo.set_degrees(90);

    // Initialize LCD (GP4=SDA, GP5=SCL)
    let mut lcd = CharLcdI2c::new(p.I2C0, p.PIN_5, p.PIN_4).await;
    lcd.clear().await;
    lcd.print("Starting RFID...").await;

    info!("LCD initialized");

    static IR_NEC_NOTIFIER: IrNecNotifier = IrNec::notifier();
    let ir = IrNec::new(
        p.PIN_6,
        Pull::Up, // most 38 kHz IR modules idle HIGH
        &IR_NEC_NOTIFIER,
        spawner,
    )?;
    loop {
        let ir_nec_event = ir.next_event().await;
        let (IrNecEvent::Press { addr, cmd } | IrNecEvent::Repeat { addr, cmd }) = ir_nec_event;
        info!("IR Press: Addr=0x{:02X} Cmd=0x{:02X}", addr, cmd);
    }

    // Initialize MFRC522 RFID reader device abstraction
    static RFID_CHANNELS: SpiMfrc522Channels = SpiMfrc522Reader::channels();
    let rfid_reader = SpiMfrc522Reader::new(
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

    lcd.clear().await;
    lcd.print("Scan card...").await;

    // Card tracking - map UID to assigned name (A-D for first 4 cards)
    // heapless requires power-of-2 capacity, so using 4
    let mut card_map: FnvIndexMap<[u8; 10], u8, 4> = FnvIndexMap::new();

    // Main loop: wait for RFID events OR IR button press
    loop {
        use embassy_futures::select::{Either, select};

        // Wait for either card detection OR IR button press
        match select(rfid_reader.next_event(), ir.next_event()).await {
            Either::First(RfidEvent::CardDetected { uid }) => {
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
                lcd.clear().await;

                if let Some(name) = card_name {
                    lcd.print("Card ").await;
                    lcd.write_byte(name).await;
                    lcd.print(" Seen").await;

                    // Move servo based on card letter
                    match name {
                        b'A' => servo.set_degrees(180),
                        b'B' => servo.set_degrees(135),
                        b'C' => servo.set_degrees(90),
                        b'D' => servo.set_degrees(45),
                        _ => servo.set_degrees(0), // Unknown card
                    }
                } else {
                    lcd.print("Unknown Card").await;
                    lcd.set_cursor(1, 0).await;
                    lcd.print("Map Full").await;
                    servo.set_degrees(0);
                }

                Timer::after_millis(2000).await;
            }
            Either::First(_) => {
                // ignore other RFID events
                continue;
            }
            Either::Second(ir_nec_event) => {
                // IR button pressed - reset the card map
                info!("IR button pressed, resetting card map");
                let (IrNecEvent::Press { addr, cmd } | IrNecEvent::Repeat { addr, cmd }) =
                    ir_nec_event;
                info!("IR Press: Addr=0x{:02X} Cmd=0x{:02X}", addr, cmd);
                card_map.clear();

                lcd.clear().await;
                lcd.print("Map Reset").await;

                // Wait for button release to avoid repeated triggers
            }
        }

        lcd.clear().await;
        lcd.print("Scan card...").await;

        Timer::after_millis(100).await;
    }
}
