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

### Full Demo (`examples/full_demo.rs`)
Complete demonstration integrating all peripherals:
- RFID cards assigned letters A-D, control servo position
- IR remote buttons 0-9 set servo angles (0°-180° in 20° steps)
- LCD displays real-time status with two-line messages
- Card mapping with automatic assignment

```bash
cargo run --example full_demo
```

### Wireless/NTP (`examples/wireless.rs`)
Pico W WiFi connectivity example:
- Connects to WiFi network (credentials from `.env` file)
- Fetches current time via NTP (UDP)
- Displays UTC time every 10 seconds

```bash
cargo run --example wireless
```

### IR PIO Reader (`examples/ir_pio_read.rs`)
Low-level IR receiver test using PIO state machine

```bash
cargo run --example ir_pio_read
```

## Hardware Setup

### Pico Pinout (examples/full_demo.rs)
- **GP0**: Servo PWM signal (PWM0 Channel A)
- **GP4**: LCD SDA (I2C0)
- **GP5**: LCD SCL (I2C0)
- **GP15-19**: RFID MFRC522 (SPI0: CS, MISO, SCK, MOSI, RST)
- **GP28**: IR receiver signal (pulled high, edge detection)

### Pico W Additional (examples/wireless.rs)
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

Requires Rust nightly with thumbv6m-none-eabi target:

```bash
rustup target add thumbv6m-none-eabi
cargo build                        # Minimal main (library mode)
cargo build --example full_demo    # Full peripheral demo
cargo build --example wireless     # WiFi/NTP example
```

## Configuration

Create `.env` file in project root for WiFi credentials:

```
WIFI_SSID=your_network_name
WIFI_PASS=your_password
```

Credentials are embedded at compile-time via `build.rs`.

## Demo

[![Demo Video](https://img.youtube.com/vi/Rx-7iw-0UeA/0.jpg)](https://youtu.be/Rx-7iw-0UeA)

## License

Licensed under either:

* MIT license (see LICENSE-MIT file)
* Apache License, Version 2.0
  at your option.
