use defmt::info;
use embassy_executor::Spawner;
use embassy_rp::dma::Channel;
use embassy_rp::gpio::{Level, Output, Pin};
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
    /// A page was read (NTAG213, 4 bytes)
    PageRead { page: u8, data: [u8; 4] },
    /// A page was written
    PageWritten { page: u8 },
    /// A page was locked (read-only)
    PageLocked { page: u8 },
    /// An error occurred during authentication or I/O
    ErrorEvent,
}

/// Commands sent to the RFID task
#[derive(Debug, Clone, Copy)]
pub enum RfidCommand {
    ReadPage(u8),
    WritePage(u8, [u8; 4]),
    LockPage(u8),
}

/// Concrete type for the MFRC522 device - needed because Embassy tasks can't be generic
pub type Mfrc522Device = MFRC522<SpiDriver<ExclusiveDevice<
    Spi<'static, embassy_rp::peripherals::SPI0, embassy_rp::spi::Async>,
    Output<'static>,
    NoDelay
>>>;

/// Notifier type for RFID reader events (uses Channel to ensure all cards are processed)
pub type SpiMfrc522Notifier = EmbassyChannel<CriticalSectionRawMutex, RfidEvent, 4>;
/// Command channel type for RFID commands
pub type SpiMfrc522CommandChannel = EmbassyChannel<CriticalSectionRawMutex, RfidCommand, 4>;
/// Combined channels for notifier and commands
pub type SpiMfrc522Channels = (SpiMfrc522Notifier, SpiMfrc522CommandChannel);

/// RFID reader device abstraction
pub struct SpiMfrc522Reader<'a> {
    notifier: &'a SpiMfrc522Notifier,
    commands: &'a SpiMfrc522CommandChannel,
}

impl SpiMfrc522Reader<'_> {
    /// Create a new notifier for the RFID reader
    #[must_use]
    pub const fn notifier() -> SpiMfrc522Notifier {
        EmbassyChannel::new()
    }
    /// Create a new command channel for the RFID reader
    #[must_use]
    pub const fn command_channel() -> SpiMfrc522CommandChannel {
        EmbassyChannel::new()
    }
    /// Create paired notifier+command channels
    #[must_use]
    pub const fn channels() -> SpiMfrc522Channels {
        (Self::notifier(), Self::command_channel())
    }

    /// Create a new RFID reader device abstraction
    /// 
    /// Note: Currently hardcoded to SPI0. All peripherals must have 'static lifetime.
    pub async fn new<Sck, Mosi, Miso, Dma0, Dma1, Cs, Rst>(
        spi: Peri<'static, embassy_rp::peripherals::SPI0>,
        sck: Peri<'static, Sck>,
        mosi: Peri<'static, Mosi>,
        miso: Peri<'static, Miso>,
        dma_ch0: Peri<'static, Dma0>,
        dma_ch1: Peri<'static, Dma1>,
        cs: Peri<'static, Cs>,
        rst: Peri<'static, Rst>,
    channels: &'static SpiMfrc522Channels,
        spawner: Spawner,
    ) -> Result<Self>
    where
        Sck: Pin + ClkPin<embassy_rp::peripherals::SPI0>,
        Mosi: Pin + MosiPin<embassy_rp::peripherals::SPI0>,
        Miso: Pin + MisoPin<embassy_rp::peripherals::SPI0>,
        Dma0: Channel,
        Dma1: Channel,
        Cs: Pin,
        Rst: Pin,
    {
        // Initialize the hardware
        let mfrc522 = init_mfrc522_hardware(spi, sck, mosi, miso, dma_ch0, dma_ch1, cs, rst).await?;
        
    // Spawn the polling task with both channels
    let notifier = &channels.0;
    let commands = &channels.1;
    spawner.spawn(rfid_polling_task(mfrc522, notifier, commands))
            .map_err(Error::TaskSpawn)?;
        
        Ok(Self { notifier, commands })
    }

    /// Wait for the next RFID event
    pub async fn next_event(&self) -> RfidEvent {
        self.notifier.receive().await
    }

    /// Read a 4-byte page from an NTAG213 tag
    pub async fn read_page(&self, page: u8) -> Result<[u8; 4]> {
        // send read command and await response event
        self.commands.send(RfidCommand::ReadPage(page)).await;
        loop {
            match self.next_event().await {
                RfidEvent::PageRead { page: p, data } if p == page => return Ok(data),
                RfidEvent::ErrorEvent => return Err(Error::IndexOutOfBounds),
                _ => continue,
            }
        }
    }

    /// Write a 4-byte page to an NTAG213 tag
    pub async fn write_page(&self, page: u8, data: [u8; 4]) -> Result<()> {
        self.commands.send(RfidCommand::WritePage(page, data)).await;
        loop {
            match self.next_event().await {
                RfidEvent::PageWritten { page: p } if p == page => return Ok(()),
                RfidEvent::ErrorEvent => return Err(Error::IndexOutOfBounds),
                _ => continue,
            }
        }
    }

    /// Lock (set read-only) a page on an NTAG213 tag
    pub async fn lock_page(&self, page: u8) -> Result<()> {
        self.commands.send(RfidCommand::LockPage(page)).await;
        loop {
            match self.next_event().await {
                RfidEvent::PageLocked { page: p } if p == page => return Ok(()),
                RfidEvent::ErrorEvent => return Err(Error::IndexOutOfBounds),
                _ => continue,
            }
        }
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
    notifier: &'static SpiMfrc522Notifier,
    commands: &'static SpiMfrc522CommandChannel,
) -> ! {
    info!("RFID polling task started");
    
    loop {
        // Process any pending commands
    if let Ok(cmd) = commands.try_receive() {
            match cmd {
                RfidCommand::ReadPage(page) => {
                    // read 4-byte page
                    let mut buf = [0u8; 4];
                    let mut size = 4u8;
                    let res = mfrc522.mifare_read(page, &mut buf, &mut size).await;
                    if res.is_ok() {
                        notifier.send(RfidEvent::PageRead { page, data: buf }).await
                    } else {
                        notifier.send(RfidEvent::ErrorEvent).await
                    }
                }
                RfidCommand::WritePage(page, data) => {
                    // write 4-byte page
                    let mut buf = data;
                    let res = mfrc522.mifare_ultralight_write(page, &mut buf, 4).await;
                    if res.is_ok() {
                        notifier.send(RfidEvent::PageWritten { page }).await
                    } else {
                        notifier.send(RfidEvent::ErrorEvent).await
                    }
                }
                RfidCommand::LockPage(_page) => {
                    // NTAG213 lock bits not implemented yet
                    notifier.send(RfidEvent::ErrorEvent).await
                }
            }
        }
        // Try to detect a card
        let Ok(()) = mfrc522.picc_is_new_card_present().await else {
            Timer::after_millis(100).await;
            continue;
        };
        
        info!("Card detected!");
        
        // Try to read UID
        let Ok(uid) = mfrc522.get_card(UidSize::Four).await else {
            info!("UID read error");
            Timer::after_millis(100).await;
            continue;
        };
        
        info!("UID read successfully ({} bytes)", uid.uid_bytes.len());
        
        // Convert to fixed-size array
        let uid_key = uid_to_fixed_array(&uid.uid_bytes);
        
        // Send event to channel
        notifier.send(RfidEvent::CardDetected { uid: uid_key }).await;
        
        // Wait to prevent repeated detections of the same card
        Timer::after_millis(1000).await;
    }
}

/// Initialize MFRC522 hardware (internal helper function)
async fn init_mfrc522_hardware<Sck, Mosi, Miso, Dma0, Dma1, Cs, Rst>(
    spi: Peri<'static, embassy_rp::peripherals::SPI0>,
    sck: Peri<'static, Sck>,
    mosi: Peri<'static, Mosi>,
    miso: Peri<'static, Miso>,
    dma_ch0: Peri<'static, Dma0>,
    dma_ch1: Peri<'static, Dma1>,
    cs: Peri<'static, Cs>,
    rst: Peri<'static, Rst>,
) -> Result<Mfrc522Device>
where
    Sck: Pin + ClkPin<embassy_rp::peripherals::SPI0>,
    Mosi: Pin + MosiPin<embassy_rp::peripherals::SPI0>,
    Miso: Pin + MisoPin<embassy_rp::peripherals::SPI0>,
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
