// Based on https://github.com/rust-embedded/cortex-m-quickstart/blob/master/examples/allocator.rs
// and https://github.com/rust-lang/rust/issues/51540
#![feature(alloc_error_handler)]
#![no_main]
#![no_std]
extern crate alloc;
use alloc_cortex_m::CortexMHeap;
use core::alloc::Layout;
use cortex_m::asm;
use cortex_m_rt::entry;
use cortex_m_semihosting::{debug, hprintln};
use panic_semihosting as _;

// Import the actual BitMatrixLed4 implementation we're testing
use serials::bit_matrix_led4::{BitMatrixLed4, BitsToIndexes};

#[global_allocator]
static ALLOCATOR: CortexMHeap = CortexMHeap::empty();

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    asm::bkpt();
    loop {}
}

#[entry]
fn main() -> ! {
    hprintln!("Testing BitMatrixLed4 logic...");
    
    // Test 1: from_bits - creates array with same bits in all positions
    let matrix = BitMatrixLed4::from_bits(0b_0011_1111);
    if matrix[0] == 0b_0011_1111 && matrix[1] == 0b_0011_1111 
        && matrix[2] == 0b_0011_1111 && matrix[3] == 0b_0011_1111 {
        hprintln!("PASS: from_bits");
    } else {
        hprintln!("FAIL: from_bits");
        debug::exit(debug::EXIT_FAILURE);
    }
    
    // Test 2: from_number - converts number to digit segments
    let matrix = BitMatrixLed4::from_number(1234, 0);
    if matrix[0] == 0b_0000_0110 && matrix[1] == 0b_0101_1011
        && matrix[2] == 0b_0100_1111 && matrix[3] == 0b_0110_0110 {
        hprintln!("PASS: from_number");
    } else {
        hprintln!("FAIL: from_number - got {:08b} {:08b} {:08b} {:08b}", 
                 matrix[0], matrix[1], matrix[2], matrix[3]);
        debug::exit(debug::EXIT_FAILURE);
    }
    
    // Test 3: from_number overflow - lights decimal points
    let matrix = BitMatrixLed4::from_number(12345, 0);
    let mut all_have_decimal = true;
    for &bits in matrix.iter() {
        if bits & 0b_1000_0000 == 0 {
            all_have_decimal = false;
        }
    }
    if all_have_decimal {
        hprintln!("PASS: from_number overflow");
    } else {
        hprintln!("FAIL: from_number overflow");
        debug::exit(debug::EXIT_FAILURE);
    }
    
    // Test 4: from_text - converts characters to segments
    let matrix = BitMatrixLed4::from_text(&['A', 'b', 'C', 'd']);
    if matrix[0] == 0b_0111_0111 && matrix[1] == 0b_0111_1100
        && matrix[2] == 0b_0011_1001 && matrix[3] == 0b_0101_1110 {
        hprintln!("PASS: from_text");
    } else {
        hprintln!("FAIL: from_text");
        debug::exit(debug::EXIT_FAILURE);
    }
    
    // Test 5: bits_to_indexes - optimizes multiplexing by grouping identical patterns
    let matrix = BitMatrixLed4::from_number(1221, 0);
    let mut bits_to_index = BitsToIndexes::new();
    if matrix.bits_to_indexes(&mut bits_to_index).is_ok() {
        // Should have 2 entries: one for '1' (appears at positions 0 and 3)
        // and one for '2' (appears at positions 1 and 2)
        if bits_to_index.len() == 2 {
            hprintln!("PASS: bits_to_indexes");
        } else {
            hprintln!("FAIL: bits_to_indexes - expected 2 entries, got {}", bits_to_index.len());
            debug::exit(debug::EXIT_FAILURE);
        }
    } else {
        hprintln!("FAIL: bits_to_indexes - returned error");
        debug::exit(debug::EXIT_FAILURE);
    }
    
    hprintln!("All tests passed!");

    debug::exit(debug::EXIT_SUCCESS);
    loop {}
}
