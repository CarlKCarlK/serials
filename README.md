# Serials

An embedded Rust library for Raspberry Pi Pico/Pico W providing async virtual device abstractions for common peripherals.

## Features

- **Async peripheral drivers** - Non-blocking I/O using Embassy async framework
- **Virtual device pattern** - Hardware abstraction with message-passing channels
- **RFID Reader** - MFRC522 SPI reader with card detection events
- **IR Remote** - NEC protocol decoder with GPIO interrupt edge detection
- **LCD Display** - HD44780 I2C async driver with timed messages and two-line support
- **Servo Control** - Hardware PWM-based servo positioning (0-180°)
- **WiFi (Pico W)** - CYW43439 WiFi with TCP/UDP networking and NTP time sync

## Examples

### Full Demo (`examples/full.rs`)

Complete demonstration integrating all peripherals:

- RFID cards assigned letters A-D, control servo position
- IR remote buttons 0-9 set servo angles (0°-180° in 20° steps)
- LCD displays real-time status with two-line messages
- Card mapping with automatic assignment

```bash
cargo full
```

### LCD Clock (`examples/lcd_clock.rs`)

Pico W WiFi clock with automatic time sync:

- Connects to WiFi network (credentials from `.env` file)
- Fetches local time with DST support via WorldTimeAPI
- Displays time in 12-hour format with AM/PM on LCD
- Keeps local time, syncs with internet hourly

```bash
cargo lcd_clock
```

### Wireless/NTP (`examples/wireless.rs`)

Pico W WiFi connectivity example:

- Connects to WiFi network (credentials from `.env` file)
- Fetches current time via WorldTimeAPI (HTTP)
- Displays local time every minute

```bash
cargo wireless
```

### IR NEC Decoder (`examples/ir.rs`)

IR remote receiver using the NEC protocol decoder library

```bash
cargo ir
```

## Hardware Setup

### Pico Pinout (examples/full.rs)

- **GP0**: Servo PWM signal (PWM0 Channel A)
- **GP4**: LCD SDA (I2C0)
- **GP5**: LCD SCL (I2C0)
- **GP15-19**: RFID MFRC522 (SPI0: CS, MISO, SCK, MOSI, RST)
- **GP28**: IR receiver signal (pulled high, edge detection)

### Pico W Additional (examples/wireless.rs, examples/lcd_clock.rs)

- **GP23**: CYW43 power enable
- **GP24**: CYW43 SPI data (via PIO)
- **GP25**: CYW43 SPI chip select
- **GP29**: CYW43 SPI clock (via PIO)

## EMI Mitigation Notes

For reliable IR operation alongside SPI RFID:

- Use **GP28** for IR (away from SPI cluster GP15-19)
- Add **22pF capacitor** between IR signal and GND
- Reduce RFID polling to **500ms intervals**
- MIN_IDLE filter rejects <5ms noise pulses

## Building

Requires Rust nightly with appropriate target for your board:

```bash
rustup target add thumbv6m-none-eabi           # Pico 1 (ARM)
rustup target add thumbv8m.main-none-eabihf    # Pico 2 (ARM)
rustup target add riscv32imac-unknown-none-elf # Pico 2 (RISC-V)

cargo build                    # Library only (defaults: pico1,arm)
cargo build --example full     # Full peripheral demo
cargo build --example full --features pico2,arm  # Full demo for Pico 2
```

Or use the cargo aliases:

```bash
# Pico 1 (default, ARM, no WiFi)
cargo full       # Run full demo (--release)
cargo ir         # Run IR reader (--release)
cargo flash      # Run flash example (--release)

# Pico 1 with WiFi
cargo full_w     # Run full demo with WiFi (--release)
cargo lcd_clock_w # Run LCD clock (--release)

# Pico 2 ARM
cargo full_2     # Run full demo on Pico 2 (--release)
cargo blinky_2   # Run blinky on Pico 2 (--release)

# Pico 2 RISC-V (experimental)
cargo blinky_2r  # Run blinky on Pico 2 RISC-V (--release)
```

See `ARCHITECTURE.md` for detailed information about board and architecture features.

## Configuration

Create `.env` file in project root for WiFi credentials and timezone:

```env
WIFI_SSID=your_network_name
WIFI_PASS=your_password
TIMEZONE=America/Los_Angeles
```

Credentials are embedded at compile-time via `build.rs`.

## Demo

[![Demo Video](https://img.youtube.com/vi/Rx-7iw-0UeA/0.jpg)](https://youtu.be/Rx-7iw-0UeA)

## License

Licensed under either:

- MIT license (see LICENSE-MIT file)
- Apache License, Version 2.0

at your option.
