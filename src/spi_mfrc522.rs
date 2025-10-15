use defmt::info;
use embassy_rp::dma::Channel;
use embassy_rp::gpio::{Level, Output, Pin};
use embassy_rp::spi::{ClkPin, Config as SpiConfig, Instance, MisoPin, MosiPin, Phase, Polarity, Spi};
use embassy_rp::Peri;
use embassy_time::{Instant, Timer};
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal_mfrc522::consts::PCDErrorCode;
use esp_hal_mfrc522::drivers::SpiDriver;
use esp_hal_mfrc522::MFRC522;

use crate::Result;

/// Create and initialize a new MFRC522 RFID reader with Embassy-RP SPI peripherals
pub async fn new_spi_mfrc522<'a, T, Sck, Mosi, Miso, Dma0, Dma1, Cs, Rst>(
    spi: Peri<'a, T>,
    sck: Peri<'a, Sck>,
    mosi: Peri<'a, Mosi>,
    miso: Peri<'a, Miso>,
    dma_ch0: Peri<'a, Dma0>,
    dma_ch1: Peri<'a, Dma1>,
    cs: Peri<'a, Cs>,
    rst: Peri<'a, Rst>,
) -> MFRC522<SpiDriver<ExclusiveDevice<Spi<'a, T, embassy_rp::spi::Async>, Output<'a>, NoDelay>>>
where
    T: Instance,
    Sck: Pin + ClkPin<T>,
    Mosi: Pin + MosiPin<T>,
    Miso: Pin + MisoPin<T>,
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
    let _: Result<(), PCDErrorCode> = mfrc522.pcd_init().await;
    info!("MFRC522 initialized");
    
    match mfrc522.pcd_get_version().await {
        Ok(_v) => info!("MFRC522 Version read successfully"),
        Err(_e) => info!("Version read error"),
    }
    
    mfrc522
}
