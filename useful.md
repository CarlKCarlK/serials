# Useful Commands

## Building & Running

Run main RFID+Servo demo:
```bash
cargo run
```

Run IR NEC decoder (using alias):
```bash
cargo ir
```

Or explicitly:

```bash
cargo run --example ir_pio_read
```

Troubleshoot IR pin (verify GPIO6 detects edges):
```bash
cargo run --example ir_test_pin
```

Build only (don't flash):
```bash
cargo build
cargo build --release
```

## Checking & Testing

Check code without building binary:
```bash
cargo check
```

Run all tests:
```bash
cargo test
```

Generate & open documentation:
```bash
cargo doc --open
cargo doc --no-deps --open
```

## Code Quality

Check for warnings/errors:
```bash
cargo check 2>&1 | findstr /C:"error" /C:"Finished"
```

Run clippy linter:
```bash
cargo clippy
```

Format code:
```bash
cargo fmt
```

Check formatting without changing:
```bash
cargo fmt -- --check
```

## Debugging

View defmt logs (already active during `cargo run`):
- Logs appear in real-time via RTT (Real-Time Transfer)
- No special command needed - they print to terminal

Flash to device manually (if cargo run fails):
```bash
probe-rs run --chip=RP2040 target/thumbv6m-none-eabi/debug/serials
```

## Clean & Rebuild

Remove build artifacts:
```bash
cargo clean
```

Full rebuild:
```bash
cargo clean && cargo build
```

## Project Structure

- `src/main.rs` - Main RFID card detection + servo demo
- `src/servo.rs` - Hardware PWM servo driver library
- `src/lib.rs` - Shared code (LCD, RFID reader, etc.)
- `examples/ir_pio_read.rs` - **NEW** IR NEC decoder (microsecond-precision edge timing)
- `Cargo.toml` - Dependencies and configuration
- `memory.x` - RP2040 memory layout
- `rust-toolchain.toml` - Rust version pinning

## IR Decoder Design (examples/ir_pio_read.rs)

**Architecture**: State machine with microsecond-precision timing

**States**: `Idle → LeaderLow → LeaderHigh → Data (32 bits) → Done → Idle`

- **Idle**: Waiting for ≥20ms silence, then a falling edge
- **LeaderLow**: Measuring ~9ms LOW pulse (7.5-10.5ms window)
- **LeaderHigh**: Measuring ~4.5ms HIGH pulse (3.7-5.3ms for data, 1.75-2.75ms for repeat)
- **RepeatHigh**: Alternate path for repeat frames; expects ~560µs LOW then complete
- **Data**: Collects 32 bits, each as LOW (~560µs) + HIGH (560µs for '0' or 1690µs for '1')

**Timing Windows** (microseconds):

- Leader LOW: 7500–10500 µs
- Leader HIGH: 3700–5300 µs (standard), 1750–2750 µs (repeat)
- Bit LOW: 360–760 µs
- Bit 0 HIGH: 310–810 µs
- Bit 1 HIGH: 1190–2190 µs
- Silence to arm Idle: ≥20000 µs

**Key Features**:

- ✅ One frame at a time (no buffer scanning or frame boundary confusion)
- ✅ Microsecond precision (no 1ms rounding jitter)
- ✅ Glitch filter (ignores edges < 120 µs)
- ✅ NEC checksum validation (addr ^ ~addr == 0xFF, cmd ^ ~cmd == 0xFF)
- ✅ Repeat frame support (detects and reports NEC repeat codes)
- ✅ Hard state resets on timing errors (recovery by returning to Idle)

**Why This Works**:

- State machine ensures proper frame boundaries (no "leader at index 8" mistakes)
- Microsecond windows avoid rounding artifacts from 1ms quantization
- Single-frame processing prevents mixed/corrupted data
- Pair-wise bit validation (both LOW and HIGH must be valid)
- NEC checksums reject malformed data early

- ✅ Microsecond precision avoids 1ms jitter from coarser polling
- ✅ Non-blocking `Timer::after(100µs)` lets executor multitask
- ✅ Edge-triggered logic only processes state changes (not idle time)
- ✅ Leader pattern search robust to multi-frame captures
- ✅ Handles inter-button gaps gracefully (embedded in first timing)

## Common Issues

Port not found - Make sure Pico is connected via USB and check Device Manager.

"Finished in 0.59s" with no flash - Code compiled but didn't flash, run `cargo run` again.

Code won't compile - Run `cargo check` first to see detailed errors. Common issues: missing imports, type mismatches in PWM config.

## Emulation (Legacy)

```cmd
python3 -m pip install -r C:\deldir\1124\Renode_RP2040\visualization\requirements.txt
cd tests
renode --console run_fib.resc
s
http://localhost:1234/
```
