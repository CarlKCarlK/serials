//! Internal PIO interrupt bindings reused by LED strip drivers.

#![cfg(not(feature = "host"))]

::embassy_rp::bind_interrupts! {
    pub struct Pio0Irqs {
        PIO0_IRQ_0 => ::embassy_rp::pio::InterruptHandler<::embassy_rp::peripherals::PIO0>;
    }
}

::embassy_rp::bind_interrupts! {
    pub struct Pio1Irqs {
        PIO1_IRQ_0 => ::embassy_rp::pio::InterruptHandler<::embassy_rp::peripherals::PIO1>;
    }
}

#[cfg(feature = "pico2")]
::embassy_rp::bind_interrupts! {
    pub struct Pio2Irqs {
        PIO2_IRQ_0 => ::embassy_rp::pio::InterruptHandler<::embassy_rp::peripherals::PIO2>;
    }
}
