#[test]
fn ui_tests() {
    let test_cases = trybuild::TestCases::new();

    // SM validation is universal
    test_cases.compile_fail("tests/ui_bad_sm.rs");

    // Run pin-compat only if building for RP2350 (Pico 2)
    #[cfg(feature = "pico2")]
    test_cases.compile_fail("tests/ui_bad_pio_pin.rs");
}
