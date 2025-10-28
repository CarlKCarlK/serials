use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::dma::Channel;
use embassy_rp::gpio::{Level, Output, Pin};
use embassy_rp::peripherals::SPI0;
use embassy_rp::spi::{ClkPin, Config as SpiConfig, MisoPin, MosiPin, Phase, Polarity, Spi};
use embassy_rp::Peri;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel as EmbassyChannel;
use embassy_time::{Instant, Timer};
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal_mfrc522::consts::UidSize;
use esp_hal_mfrc522::drivers::SpiDriver;
use esp_hal_mfrc522::MFRC522;

use crate::{Error, Result};

/// Events from the RFID reader
#[derive(Debug, Clone, Copy)]
pub enum RfidEvent {
    /// A card was detected
    CardDetected { uid: [u8; 10] },
}

/// Notifier type for RFID reader events
pub type Mfrc522Device = MFRC522<SpiDriver<ExclusiveDevice<
    Spi<'static, SPI0, embassy_rp::spi::Async>,
    Output<'static>,
    NoDelay
>>>;

/// Notifier type for RFID reader events (uses Channel to ensure all cards are processed)
pub type RfidNotifier = EmbassyChannel<CriticalSectionRawMutex, RfidEvent, 4>;

/// RFID reader device abstraction
pub struct Rfid<'a> {
    notifier: &'a RfidNotifier,
}

impl Rfid<'_> {
    /// Create a new notifier for the RFID reader
    #[must_use]
    pub const fn notifier() -> RfidNotifier {
        EmbassyChannel::new()
    }

    /// Create a new RFID reader device abstraction
    /// 
    /// Note: Currently hardcoded to SPI0. All peripherals must have 'static lifetime.
    pub async fn new<Sck, Mosi, Miso, Dma0, Dma1, Cs, Rst>(
        spi: Peri<'static, SPI0>,
        sck: Peri<'static, Sck>,
        mosi: Peri<'static, Mosi>,
        miso: Peri<'static, Miso>,
        dma_ch0: Peri<'static, Dma0>,
        dma_ch1: Peri<'static, Dma1>,
        cs: Peri<'static, Cs>,
        rst: Peri<'static, Rst>,
        notifier: &'static RfidNotifier,
        spawner: Spawner,
    ) -> Result<Self>
    where
        Sck: Pin + ClkPin<SPI0>,
        Mosi: Pin + MosiPin<SPI0>,
        Miso: Pin + MisoPin<SPI0>,
        Dma0: Channel,
        Dma1: Channel,
        Cs: Pin,
        Rst: Pin,
    {
        // Initialize the hardware
        let mfrc522 = init_mfrc522_hardware(spi, sck, mosi, miso, dma_ch0, dma_ch1, cs, rst).await?;
        
        // Spawn the polling task
        let token = rfid_polling_task(mfrc522, notifier).map_err(Error::TaskSpawn)?;
        spawner.spawn(token);
        
        Ok(Self { notifier })
    }

    /// Wait for the next RFID event (card detection)
    pub async fn wait(&self) -> RfidEvent {
        self.notifier.receive().await
    }
}

/// Convert UID bytes to a fixed-size array, padding with zeros if needed
fn uid_to_fixed_array(uid_bytes: &[u8]) -> [u8; 10] {
    let mut uid_key = [0u8; 10];
    #[expect(clippy::indexing_slicing, reason = "Length checked")]
    for (i, &byte) in uid_bytes.iter().enumerate() {
        if i < 10 {
            uid_key[i] = byte;
        }
    }
    uid_key
}

/// Embassy task that continuously polls for RFID cards
#[embassy_executor::task]
async fn rfid_polling_task(
    mut mfrc522: Mfrc522Device,
    notifier: &'static RfidNotifier,
) -> ! {
    info!("RFID polling task started");
    
    loop {
        // Try to detect a card
        let Ok(()) = mfrc522.picc_is_new_card_present().await else {
            Timer::after_millis(500).await;
            continue;
        };
        
        info!("Card detected!");
        
        // Try to read UID
        let Ok(uid) = mfrc522.get_card(UidSize::Four).await else {
            info!("UID read error");
            Timer::after_millis(500).await;
            continue;
        };
        
        info!("UID read successfully ({} bytes)", uid.uid_bytes.len());
        
        // Convert to fixed-size array
        let uid_key = uid_to_fixed_array(&uid.uid_bytes);
        
        // Send event to channel
        notifier.send(RfidEvent::CardDetected { uid: uid_key }).await;
        
        // Wait to prevent repeated detections of the same card
        Timer::after_millis(50).await;
    }
}

/// Initialize MFRC522 hardware (internal helper function)
async fn init_mfrc522_hardware<Sck, Mosi, Miso, Dma0, Dma1, Cs, Rst>(
    spi: Peri<'static, SPI0>,
    sck: Peri<'static, Sck>,
    mosi: Peri<'static, Mosi>,
    miso: Peri<'static, Miso>,
    dma_ch0: Peri<'static, Dma0>,
    dma_ch1: Peri<'static, Dma1>,
    cs: Peri<'static, Cs>,
    rst: Peri<'static, Rst>,
) -> Result<Mfrc522Device>
where
    Sck: Pin + ClkPin<SPI0>,
    Mosi: Pin + MosiPin<SPI0>,
    Miso: Pin + MisoPin<SPI0>,
    Dma0: Channel,
    Dma1: Channel,
    Cs: Pin,
    Rst: Pin,
{
    // Initialize async SPI for RFID
    let spi = Spi::new(
        spi,
        sck,
        mosi,
        miso,
        dma_ch0,
        dma_ch1,
        {
            let mut config = SpiConfig::default();
            config.frequency = 1_000_000; // 1 MHz
            config.polarity = Polarity::IdleLow;
            config.phase = Phase::CaptureOnFirstTransition;
            config
        },
    );
    
    // CS pin for MFRC522
    let cs = Output::new(cs, Level::High);
    
    // Reset RFID module
    let mut rst = Output::new(rst, Level::High);
    rst.set_low();
    Timer::after_millis(10).await;
    rst.set_high();
    Timer::after_millis(50).await;
    
    // Wrap SPI+CS in ExclusiveDevice to implement SpiDevice trait
    let spi_device = ExclusiveDevice::new_no_delay(spi, cs).expect("CS pin is infallible");
    let spi_driver = SpiDriver::new(spi_device);
    let mut mfrc522 = MFRC522::new(spi_driver, || Instant::now().as_millis());
    
    // Initialize the MFRC522 chip
    mfrc522.pcd_init().await.map_err(Error::Mfrc522Init)?;
    info!("MFRC522 initialized");
    
    let _version = mfrc522.pcd_get_version().await.map_err(Error::Mfrc522Version)?;
    info!("MFRC522 version read successfully");
    
    Ok(mfrc522)
}
