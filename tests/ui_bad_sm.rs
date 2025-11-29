#![no_std]

use serials::led_strip::define_led_strips;

define_led_strips! {
    pio: PIO1,
    strips: [
        led_strip0 {
            sm: 4, // invalid; macro must reject this (valid range is 0-3)
            dma: DMA_CH0,
            pin: PIN_16,
            len: 8,
            max_current_ma: 100,
        }
    ]
}

fn main() {}
