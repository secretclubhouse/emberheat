# Thermos-Rust

A multi-zone thermostat system for hydronic heating, written in Rust for ESP32-C6.

This is a Rust reimplementation of the [Thermos](https://github.com/truncj/thermos) Python project, offering improved performance, memory safety, and embedded support.

> **Note:** Matter/HomeKit integration is planned but not yet functional. See [MATTER_INTEGRATION.md](MATTER_INTEGRATION.md) for details.

## Features

- **Multi-Zone Control**: Independent temperature control for multiple heating zones
- **Hysteresis Control**: Intelligent heating control with configurable dead band to prevent rapid cycling
- **Multiple Sensor Types**:
  - 1-Wire DS18B20 temperature sensors via RMT peripheral
  - I2C BME280 sensor support
- **State Persistence**: NVS flash storage for recovery after restarts
- **MQTT Logging**: Temperature and state reporting via MQTT
- **WiFi Connectivity**: Built-in WiFi for network integration

## Project Structure

```
thermos-rust/
├── Cargo.toml              # Workspace definition
├── thermos-core/           # Platform-agnostic core library (no_std)
│   └── src/
│       ├── types.rs        # HeatingState, TemperatureReading, etc.
│       ├── sensor.rs       # TemperatureSensor trait
│       ├── gpio.rs         # GpioManager trait
│       ├── persistence.rs  # StateStorage trait
│       └── control.rs      # Hysteresis control algorithm
├── thermos-esp32/          # ESP32-C6 bare-metal implementation
│   └── src/
│       ├── main.rs         # Embassy entry point
│       ├── gpio_impl.rs    # esp-hal GPIO
│       ├── sensor_onewire.rs   # 1-Wire via RMT peripheral
│       ├── sensor_i2c.rs   # BME280 I2C sensor
│       ├── persistence_nvs.rs  # NVS flash storage
│       ├── wifi.rs         # WiFi connectivity
│       └── mqtt.rs         # MQTT logging
└── thermos-linux/          # Linux implementation (preserved, not active)
```

> **Note:** `thermos-linux` contains a Raspberry Pi implementation with Redis and Prometheus support. It is preserved in the repository but not actively developed. Use `cargo build -p thermos-linux` to build it explicitly.

## Hardware Requirements

- ESP32-C6 development board
- DS18B20 1-Wire temperature sensor OR BME280 I2C sensor
- Relay module for heating control
- WiFi network for MQTT connectivity

## Getting Started

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add ESP32 target
rustup target add riscv32imac-unknown-none-elf

# Install espflash
cargo install espflash
```

### Building and Flashing

```bash
# Build ESP32 firmware
make build-esp32

# Flash to device
make flash

# Flash and open serial monitor
make flash-monitor
```

### Configuration

WiFi and MQTT settings are configured in `thermos-esp32/src/main.rs`. Update the constants for your network:

```rust
const WIFI_SSID: &str = "your-ssid";
const WIFI_PASSWORD: &str = "your-password";
const MQTT_BROKER: &str = "192.168.1.100";
```

## Hysteresis Control Logic

The thermostat prevents rapid on/off cycling using a **±0.5°C dead band**:

```
Temperature
    ↑
    │
 22°C ├─────────┐ OFF
    │         │
21.5°C ├─────────┼───────┐ Dead Band
    │         │       │
 21°C ├─────────┼───────┘ Target
    │         │
20.5°C ├─────────┘ ON
    │
    └──────────────────> Time
```

- **Turn ON** when: `temperature ≤ target - 0.5°C`
- **Turn OFF** when: `temperature ≥ target + 0.5°C`
- **Maintain state** when within dead band

## Development

### Running Tests

```bash
# Run thermos-core tests
make test

# Run with verbose output
make test-verbose
```

### Code Quality

```bash
# Format, lint, and test
make dev

# Format code
make fmt

# Run clippy
make lint
```

### Available Make Targets

```bash
make help          # Show all targets
make build         # Build thermos-core
make build-esp32   # Build ESP32 firmware
make flash         # Flash to ESP32
make monitor       # Open serial monitor
make flash-monitor # Flash and monitor
make test          # Run tests
make dev           # Format, lint, test
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development guidelines.

## License

MIT License - see LICENSE file for details

## Acknowledgments

- Original Python implementation: [Thermos](https://github.com/truncj/thermos)
- ESP32 HAL: [esp-hal](https://github.com/esp-rs/esp-hal)
- Async runtime: [Embassy](https://embassy.dev/)

## Roadmap

- [x] ESP32-C6 bare-metal support with Embassy
- [x] WiFi and MQTT integration
- [x] NVS flash persistence
- [x] I2C BME280 sensor support
- [x] 1-Wire via RMT peripheral
- [ ] Matter integration via rs-matter (see [MATTER_INTEGRATION.md](MATTER_INTEGRATION.md))
- [ ] Web UI for configuration
- [ ] Home Assistant integration
