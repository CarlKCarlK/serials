// Simple compile-time test to verify our const functions work correctly
#![cfg(test)]

use serials::led_strip::{pio_id, pin_number, pio_can_use_pin};

#[test]
fn test_pio_id_parsing() {
    const PIO0: u8 = pio_id("PIO0");
    const PIO1: u8 = pio_id("PIO1");
    const PIO2: u8 = pio_id("PIO2");
    
    assert_eq!(PIO0, 0);
    assert_eq!(PIO1, 1);
    assert_eq!(PIO2, 2);
}

#[test]
fn test_pin_number_parsing() {
    const PIN_0: u8 = pin_number("PIN_0");
    const PIN_2: u8 = pin_number("PIN_2");
    const PIN_16: u8 = pin_number("PIN_16");
    const PIN_29: u8 = pin_number("PIN_29");
    const PIN_47: u8 = pin_number("PIN_47");
    
    assert_eq!(PIN_0, 0);
    assert_eq!(PIN_2, 2);
    assert_eq!(PIN_16, 16);
    assert_eq!(PIN_29, 29);
    assert_eq!(PIN_47, 47);
}

#[test]
#[cfg(not(feature = "pico2"))]
fn test_pio_pin_compat_pico1() {
    // On Pico 1 (RP2040), all PIOs can use all pins
    const VALID1: bool = pio_can_use_pin(0, 2);
    const VALID2: bool = pio_can_use_pin(1, 16);
    const VALID3: bool = pio_can_use_pin(0, 29);
    
    assert!(VALID1);
    assert!(VALID2);
    assert!(VALID3);
}

#[test]
#[cfg(feature = "pico2")]
fn test_pio_pin_compat_pico2() {
    // On Pico 2 (RP2350), PIO2 has restrictions
    // PIO0 and PIO1 can use any pin
    const VALID_PIO0: bool = pio_can_use_pin(0, 2);
    const VALID_PIO1: bool = pio_can_use_pin(1, 16);
    
    // PIO2 can only use pins 24-29 and 47
    const VALID_PIO2_PIN24: bool = pio_can_use_pin(2, 24);
    const VALID_PIO2_PIN29: bool = pio_can_use_pin(2, 29);
    const VALID_PIO2_PIN47: bool = pio_can_use_pin(2, 47);
    const INVALID_PIO2_PIN2: bool = pio_can_use_pin(2, 2);
    const INVALID_PIO2_PIN16: bool = pio_can_use_pin(2, 16);
    
    assert!(VALID_PIO0);
    assert!(VALID_PIO1);
    assert!(VALID_PIO2_PIN24);
    assert!(VALID_PIO2_PIN29);
    assert!(VALID_PIO2_PIN47);
    assert!(!INVALID_PIO2_PIN2);
    assert!(!INVALID_PIO2_PIN16);
}
