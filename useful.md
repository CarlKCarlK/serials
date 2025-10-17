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
- `examples/ir_pio_read.rs` - IR signal reading experiment
- `Cargo.toml` - Dependencies and configuration
- `memory.x` - RP2040 memory layout
- `rust-toolchain.toml` - Rust version pinning

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
