# Thermos-Rust

A Matter-enabled multi-zone thermostat system for hydronic heating, written in Rust for ESP32-C6. Controls heating zones via HomeKit, Google Home, and other Matter-compatible controllers.

## Architecture

**Cargo workspace** with three active crates:

| Crate | Purpose | Platform |
|-------|---------|----------|
| `thermos-core` | Core control logic, traits, types | `no_std` compatible |
| `thermos-matter` | Matter protocol types and cluster definitions | `no_std` compatible |
| `thermos-esp32` | ESP32-C6 firmware (active) | ESP32-C6 bare-metal |
| `thermos-linux` | Raspberry Pi implementation (preserved) | Linux |

### Core Library (`thermos-core`)

Platform-agnostic library containing:
- **Temperature types** with Celsius/Fahrenheit conversion
- **Hysteresis control algorithm** (prevents relay chatter with ±0.5°C dead band)
- **Trait definitions** for sensors, GPIO, and state persistence
- **Sensor validation** (3°C - 37°C range)

### Matter Integration (`thermos-matter`)

Smart home protocol support via [rs-matter](https://github.com/project-chip/rs-matter):
- **Thermostat Cluster** (0x0201) with heating and cooling support
- **Atomic state management** for cross-task zone state access
- **Zone-aware state** with target temperatures and system modes
- **Lock-free design** using portable-atomic for real-time safety
- **Dirty flag tracking** for efficient flash persistence

### ESP32 Implementation (`thermos-esp32`)

Active firmware for ESP32-C6:
- **Embassy async runtime** for cooperative multitasking
- **Matter stack** with BLE + WiFi commissioning (single-radio and dual-radio modes)
- **Flash persistence** with wear-leveling via sequential-storage
- **Factory reset** via GPIO button (hold 5 seconds)
- **1-Wire temperature sensors** via RMT peripheral
- **BME280 I2C sensor** support
- **Relay control** via GPIO
- **WiFi connectivity** provisioned through Matter commissioning
- **MQTT logging** for monitoring
- **Matter bridge** connecting protocol state to control loops

### Linux Implementation (`thermos-linux`)

Preserved but not actively developed. Uses:
- `sysfs_gpio` for GPIO control
- `ds18b20` for 1-Wire sensors
- `serde_json` for state persistence

## Quick Start (ESP32-C6)

### Prerequisites

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add ESP32 target
rustup target add riscv32imac-unknown-none-elf

# Install espflash
cargo install espflash
```

### Build and Flash

```bash
# Build with Matter support (single-radio: C6 WiFi + BLE time-shared)
make build-matter

# Build with dual-radio (C6 WiFi + H2 BLE over UART HCI)
make build-matter-dual-radio

# Flash Matter firmware and monitor serial output
make flash-monitor-matter

# Run tests (thermos-core + thermos-matter)
make test
```

### Matter Commissioning

After flashing, the device enters commissioning mode. Use the Apple Home app, Google Home, or any Matter controller to pair:

1. Power on the ESP32-C6
2. Open your Matter controller app
3. Scan the QR code or enter the setup code manually
4. The device appears as a Thermostat

Default commissioning credentials (configurable in `config.rs`):
- **Discriminator:** 3840
- **Passcode:** 20202021
- **Vendor ID:** 0xFFF1 (test)

The device supports up to 5 Matter fabrics simultaneously (HomeKit (2) + Google Home + Alexa + Home Assistant).

### Zone Configuration

Edit `thermos-esp32/src/config.rs` to configure zones (1-16 supported):

```rust
pub const NUM_ZONES: usize = 7;

pub const ZONE_CONFIGS: [ZoneConfigData; NUM_ZONES] = [
    ZoneConfigData {
        name: "Living Room",
        relay_pin: 5,
        sensor_address: [0x28, ...],
        default_target_c: 21.1,
    },
    // ... more zones
];
```

Each zone appears as a separate thermostat endpoint in Matter controllers.

## How It Works

### Control Loop (per zone)

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│ Read Sensor  │────▶│ Apply        │────▶│ Set Relay   │
│ Temperature  │     │ Hysteresis   │     │ State       │
└─────────────┘     └──────────────┘     └─────────────┘
       │                    │                     │
       ▼                    ▼                     ▼
  Validate 3-37°C    ±0.5°C dead band     GPIO HIGH/LOW
```

### Matter Integration

```
┌──────────────┐     ┌──────────────┐     ┌─────────────┐
│ Matter Stack │────▶│ Zone State   │────▶│ Control     │
│ (HomeKit etc)│     │ (Atomic)     │     │ Loop        │
└──────────────┘     └──────────────┘     └─────────────┘
  Set target temp    Lock-free R/W     Reads new target
  Set mode           Portable-atomic   Applies hysteresis
       │                    │
       ▼                    ▼
  BLE commission     Flash persist
  WiFi provision     (sequential-storage)
```

### Persistence

Zone setpoints and modes are persisted to flash with wear-leveling. On power loss, the device restores its last known configuration. Matter commissioning data (fabrics, network credentials) is also persisted.

### Safety

- Heating defaults to **OFF** on any error
- Temperature readings validated before use
- State persisted to flash for power-loss recovery
- Hysteresis prevents rapid relay switching
- Factory reset available via GPIO 9 button (hold 5 seconds)

## Matter Clusters

The thermostat endpoint implements:

| Cluster | ID | Purpose |
|---------|-----|---------|
| Thermostat | 0x0201 | Temperature control, setpoints, system mode |
| Identify | 0x0003 | Device identification (HomeKit requirement) |
| Binding | 0x001E | External sensor support |
| Thermostat UI Config | 0x0204 | Temperature display format |
| Descriptor | 0x001D | Device type and cluster lists |

### Thermostat Attributes

| Attribute | Access | Description |
|-----------|--------|-------------|
| LocalTemperature | Read | Current temperature (0.01°C units) |
| OccupiedHeatingSetpoint | Read/Write | Heating target (7-30°C) |
| OccupiedCoolingSetpoint | Read/Write | Cooling target (10-35°C) |
| SystemMode | Read/Write | Off / Heat / Cool |
| ThermostatRunningState | Read | Currently heating/cooling/idle |
| ControlSequenceOfOperation | Read | CoolingAndHeating |

## Development

```bash
make dev        # Format + lint + test
make build      # Build core library
make test       # Run all tests
make lint       # Clippy lints
```

### Build Targets

```bash
# Core
make build              # Build thermos-core
make build-esp32        # Build ESP32 firmware (MQTT mode, no Matter)

# Matter
make build-matter                    # Single radio (C6 WiFi + BLE)
make build-matter-dual-radio         # Dual radio (C6 WiFi + H2 BLE)
make flash-matter                    # Flash single radio
make flash-matter-dual-radio         # Flash dual radio
make flash-monitor-matter            # Flash + monitor
make flash-monitor-matter-dual-radio # Flash + monitor (dual radio)

# Quality
make test               # Run tests (core + matter)
make lint               # Clippy
make fmt                # Format
make dev                # Format + lint + test
```

## Project Status

- [x] Core hysteresis algorithm with tests
- [x] ESP32-C6 firmware with Embassy async runtime
- [x] 1-Wire and I2C sensor support
- [x] Multi-zone relay control (1-16 zones)
- [x] Matter commissioning (BLE + WiFi)
- [x] Thermostat cluster with heating and cooling
- [x] HomeKit / Google Home / Alexa integration via Matter
- [x] Atomic zone state management (lock-free)
- [x] Flash persistence with wear-leveling (fabrics + zone state)
- [x] Factory reset and zone state reset
- [x] Dual-radio support (ESP32-C6 WiFi + ESP32-H2 BLE)
- [ ] MQTT telemetry in Matter mode (blocked by socket pool size)
- [ ] OTA firmware updates

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT License ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
