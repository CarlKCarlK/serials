#![no_std]

use serials::led_strip::define_led_strips;

// On RP2350, PIO2 can only use pins 24-29 and 47
// PIN_2 is not in that range, so this should fail
define_led_strips! {
    pio: PIO2,
    strips: [
        led_strip0 {
            sm: 0,
            dma: DMA_CH0,
            pin: PIN_2, // invalid for PIO2 on RP2350
            len: 8,
            max_current_ma: 120,
        }
    ]
}

fn main() {}
