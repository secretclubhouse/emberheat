# Thermos-Rust

A multi-zone thermostat system for hydronic heating, written in Rust.

**Supported Platforms:**
- **Linux (Raspberry Pi)** - Full-featured implementation with Redis persistence and Prometheus metrics
- **ESP32-C6 (bare-metal)** - Embedded no_std implementation with WiFi, MQTT, and NVS flash storage

This is a Rust reimplementation of the [Thermos](https://github.com/truncj/thermos) Python project, offering improved performance, memory safety, and multi-platform support.

> **Note:** Matter/HomeKit integration is currently a stub (not functional). See [MATTER_INTEGRATION.md](MATTER_INTEGRATION.md) for planned rs-matter integration on ESP32.

## Features

- **Multi-Zone Control**: Independent temperature control for multiple heating zones
- **Hysteresis Control**: Intelligent heating control with configurable dead band to prevent rapid cycling
- **Multiple Sensor Types**:
  - 1-Wire DS18B20 temperature sensors (sysfs on Linux, RMT peripheral on ESP32)
  - HTTP endpoints for remote sensors (fallback, Linux only)
  - I2C BME280 sensor support (ESP32 only)
- **State Persistence**: Redis (Linux) or NVS flash (ESP32) for recovery after restarts
- **Metrics Export**: Prometheus metrics (Linux) and MQTT logging (ESP32)
- **Robust Error Handling**: Automatic sensor power cycling on failures
- **Graceful Shutdown**: Proper cleanup of GPIO and network resources

## Architecture

```
         ┌───────────────────────┐
         │  Thermostat Bridge    │
         └───────┬───────────────┘
                 │
    ┌────────────┼────────────┬──────────┐
    │            │            │          │
  ┌─▼──────┐ ┌──▼──────┐ ┌──▼──────┐ ┌─▼──────┐
  │Zone1   │ │Zone2    │ │Zone3    │ │ZoneN   │
  │Thermo. │ │Thermo.  │ │Thermo.  │ │Thermo. │
  └─┬──────┘ └──┬──────┘ └──┬──────┘ └─┬──────┘
    │           │           │          │
    ├────┬──────┼─────┬─────┼──────┬───┤
    │    │      │     │     │      │   │
    ▼    ▼      ▼     ▼     ▼      ▼   ▼
 ┌─────────────────────────────────────┐
 │ GPIO Controller (rppal)             │
 │ ├─ Relay Pins (heating control)     │
 │ ├─ Temp Pins (sensor power)         │
 │ └─ VCC Pin (power cycling)          │
 └─────────────────────────────────────┘
    │           │                      │
    ▼           ▼                      ▼
1-Wire      HTTP Sensors           GPIO State
Sensors     (fallback)             (relay)
(DS18B20)


Side Services:
┌─────────────────────────┐
│ Redis:6379              │ (State persistence)
├─────────────────────────┤
│ Prometheus:8080         │ (Metrics HTTP)
└─────────────────────────┘
```

## Hardware Requirements

### Linux (Raspberry Pi)

- Raspberry Pi (any model with GPIO)
- DS18B20 1-Wire temperature sensors
- Solid-state relays for heating control
- 4.7kΩ pull-up resistor for 1-Wire bus
- Redis server (can run on same Pi or remote)

### ESP32-C6

- ESP32-C6 development board (recommended for Thread radio support)
- DS18B20 1-Wire temperature sensor OR BME280 I2C sensor
- Relay module for heating control
- WiFi network for MQTT connectivity

### GPIO Pin Configuration

The application uses BCM pin numbering:

- **VCC Pin (GPIO 0)**: Controls power to temperature sensors
- **Relay Pins**: Configurable per zone (e.g., GPIO 23, 24, 25)
- **Temp Pins**: Configurable per zone (e.g., GPIO 4, 5, 6)

## Installation

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install system dependencies (Debian/Ubuntu)
sudo apt-get update
sudo apt-get install -y \
    build-essential \
    pkg-config \
    libssl-dev \
    redis-server

# Enable 1-Wire interface
sudo raspi-config
# Navigate to: Interface Options > 1-Wire > Enable
```

### Building from Source

```bash
# Clone the repository
cd thermos-rust

# Build release version
cargo build --release

# The binary will be at: target/release/thermos-rust
```

### Cross-Compilation for Raspberry Pi

If building on another machine:

```bash
# Install cross-compilation toolchain
rustup target add armv7-unknown-linux-gnueabihf  # For Pi 2/3/4
# or
rustup target add aarch64-unknown-linux-gnu      # For Pi 3/4 (64-bit)

# Install cross-compiler
sudo apt-get install gcc-arm-linux-gnueabihf

# Build
cargo build --release --target armv7-unknown-linux-gnueabihf
```

## Configuration

### Configuration File Location

The application searches for `config.json` in the following locations (in order):

1. **Command line argument**: `thermos-rust /path/to/config.json`
2. **System-wide** (recommended for binary installations): `/etc/thermos-rust/config.json`
3. **User-specific**: `~/.config/thermos-rust/config.json`
4. **Local directory** (development): `./config/config.json`

**For binary installations**, create the system-wide config:

```bash
# Create config directory
sudo mkdir -p /etc/thermos-rust

# Copy and edit the example config
sudo cp config/config.example.json /etc/thermos-rust/config.json
sudo nano /etc/thermos-rust/config.json
```

### Configuration Format

```json
{
    "redis_url": "redis://localhost:6379",
    "Zone1": {
        "relay_pin": 23,
        "temp_pin": 4,
        "temp_id": "28-00000ab12345"
    },
    "Zone2": {
        "relay_pin": 24,
        "temp_pin": 4,
        "temp_id": "28-00000ab12346",
        "extra_sensor": "http://192.168.1.100/temp"
    },
    "Zone3": {
        "relay_pin": 25,
        "temp_pin": 4,
        "temp_id": "28-00000ab12347"
    }
}
```

### Configuration Fields

| Field | Required | Description |
|-------|----------|-------------|
| `redis_url` | No | Redis server URL (defaults to `redis://localhost:6379`) |
| `relay_pin` | Yes | GPIO pin (BCM) for relay control (0-27), must be unique per zone |
| `temp_pin` | Yes | GPIO pin (BCM) for 1-Wire bus (0-27), can be shared across zones |
| `temp_id` | Yes | 1-Wire sensor ID (with or without "28-" prefix) |
| `extra_sensor` | No | HTTP endpoint returning `{"temp_c": 20.5}` for fallback |

**Note**: Multiple zones can share the same `temp_pin` since 1-Wire is a bus protocol that supports multiple sensors on one pin.

### Finding Your Sensor IDs

```bash
# List all 1-Wire sensors
ls /sys/bus/w1/devices/

# Example output:
# 28-00000ab12345  28-00000ab12346  w1_bus_master1

# The IDs starting with "28-" are DS18B20 sensors
```

## Usage

### Running the Application

```bash
# Run with default config (./config/config.json)
./target/release/thermos-rust

# Run with custom config
./target/release/thermos-rust /path/to/config.json

# Enable debug logging
RUST_LOG=debug ./target/release/thermos-rust

# Enable trace logging for specific module
RUST_LOG=thermos_rust::devices=trace ./target/release/thermos-rust
```

### Running as a System Service

Create a systemd service file at `/etc/systemd/system/thermos-rust.service`:

```ini
[Unit]
Description=Thermos-Rust Multi-Zone Thermostat
After=network.target redis.service
Requires=redis.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/thermos-rust
ExecStart=/opt/thermos-rust/thermos-rust
Restart=always
RestartSec=10
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
```

Enable and start the service:

```bash
sudo systemctl daemon-reload
sudo systemctl enable thermos-rust
sudo systemctl start thermos-rust
sudo systemctl status thermos-rust
```

## Monitoring

### Prometheus Metrics

Metrics are exposed at `http://<raspberry-pi-ip>:8080/metrics`

Available metrics:

```
# Temperature readings
current_temperature{room="Zone1",heat_status="1"} 68.5
target_temperature{room="Zone1",heat_status="1"} 70.0

# Heating status (0=off, 1=on)
heat_status{room="Zone1"} 1

# Sensor response time (seconds)
response_time{room="Zone1"} 0.125

# Error counts
reset_error_count{room="Zone1"} 3
```

### Grafana Dashboards

Pre-built Grafana dashboards are included for visualization:

```bash
# Start Grafana using Docker Compose
docker-compose up -d grafana

# Access Grafana at http://<raspberry-pi-ip>:3000
# Default credentials: admin/admin
```

The included dashboard provides:
- Multi-zone temperature graphs
- Heat status indicators
- Response time tracking
- Error counter displays
- Time-series visualization

Dashboard configuration files:
- `grafana/dashboards/thermostat.json` - Pre-built dashboard
- `grafana/datasources/prometheus.yml` - Prometheus datasource config

### Logs

View real-time logs:

```bash
# If running as systemd service
sudo journalctl -u thermos-rust -f

# If running directly
# Logs go to stdout/stderr
```

### Redis State Inspection

```bash
# Connect to Redis
redis-cli

# List all thermostat zones
KEYS thermostat:*

# View state for a specific zone
GET thermostat:Zone1

# Example output:
# {"relay_pin":23,"temp_pin":4,"temp_id":"28-00000ab12345",
#  "target_temp":21.1,"target_state":1,"current_state":1,
#  "last_updated":"2024-01-15T10:30:00Z"}
```

## Hysteresis Control Logic

The thermostat prevents rapid on/off cycling using a **±0.5°C dead band**:

```
Temperature
    ↑
    │
 22°C ├─────────┐ OFF
    │         │
21.5°C ├─────────┼───────┐ Dead Band (maintain current state)
    │         │       │
 21°C ├─────────┼───────┘ Target Temperature
    │         │
20.5°C ├─────────┘ ON
    │
    └──────────────────> Time
```

**Control Rules:**

- **Turn ON** when: `temperature ≤ target - 0.5°C`
- **Turn OFF** when: `temperature ≥ target + 0.5°C`
- **Maintain state** when: `target - 0.5°C < temperature < target + 0.5°C`

### Constants

| Constant | Value | Description |
|----------|-------|-------------|
| `HYSTERESIS_UPPER_THRESHOLD_C` | 0.5°C | Turn OFF threshold |
| `HYSTERESIS_LOWER_THRESHOLD_C` | -0.5°C | Turn ON threshold |
| `DEFAULT_TARGET_TEMP_C` | 21.1°C (70°F) | Initial setpoint |
| `CONTROL_LOOP_INTERVAL_SECS` | 6 seconds | Control loop frequency |
| `SENSOR_ERROR_MIN_TEMP_C` | 3°C | Minimum valid reading |
| `SENSOR_ERROR_MAX_TEMP_C` | 37°C | Maximum valid reading |

## Error Recovery

### Sensor Failures

When a temperature sensor returns invalid readings:

1. Error is logged with details
2. Prometheus error counter is incremented
3. VCC power is cycled (3 seconds off, 5 seconds recovery)
4. Default temperature (21.1°C) is used temporarily
5. Next reading attempt after recovery period

### Network Failures

- **Redis**: Application logs error but continues operation (state not persisted until reconnection)
- **Matter**: Connection attempts continue in background
- **HTTP Sensors**: Automatically falls back to 1-Wire sensor

### GPIO Errors

GPIO errors are critical and logged as errors. The application attempts to continue but functionality may be degraded.

## Development

### Project Structure

The project uses a Cargo workspace with three crates:

```
thermos-rust/
├── Cargo.toml                  # Workspace definition
├── thermos-core/               # Platform-agnostic core library (no_std)
│   └── src/
│       ├── lib.rs              # Core exports
│       ├── types.rs            # HeatingState, TemperatureReading, etc.
│       ├── sensor.rs           # TemperatureSensor trait
│       ├── gpio.rs             # GpioManager trait
│       ├── persistence.rs      # StateStorage trait
│       ├── control.rs          # Hysteresis control algorithm
│       └── config.rs           # Configuration types
├── thermos-linux/              # Linux/Raspberry Pi implementation
│   └── src/
│       ├── main.rs             # Entry point
│       ├── config/             # JSON config loading
│       ├── devices/            # Thermostat bridge
│       ├── gpio/               # rppal-based GPIO
│       ├── sensors/            # 1-Wire sysfs, HTTP sensors
│       ├── persistence/        # Redis integration
│       ├── metrics/            # Prometheus export
│       └── mocks/              # Test infrastructure
├── thermos-esp32/              # ESP32-C6 bare-metal implementation
│   └── src/
│       ├── main.rs             # Embassy entry point
│       ├── lib.rs              # Library exports
│       ├── gpio_impl.rs        # esp-hal GPIO
│       ├── sensor_onewire.rs   # 1-Wire via RMT peripheral
│       ├── sensor_i2c.rs       # BME280 I2C sensor
│       ├── persistence_nvs.rs  # NVS flash storage
│       ├── wifi.rs             # WiFi connectivity
│       └── mqtt.rs             # MQTT temperature logging
├── src/                        # Legacy single-crate structure
│   └── ...                     # (being migrated to workspace)
└── tests/                      # Integration tests
    ├── integration_test.rs
    └── mock_integration_tests.rs
```

### Running Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run specific test
cargo test test_temperature_conversion

# Run with coverage (requires cargo-tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

### Code Documentation

```bash
# Generate and open documentation
cargo doc --open

# Generate docs for dependencies too
cargo doc --open --no-deps
```

### Linting and Formatting

```bash
# Format code
cargo fmt

# Run clippy (linter)
cargo clippy

# Run clippy with all features and pedantic checks
cargo clippy --all-features -- -W clippy::pedantic
```

## Comparison with Python Version

| Feature | Python (HAP) | Rust |
|---------|--------------|------|
| **Language** | Python 3 | Rust |
| **Memory Usage** | ~50-100 MB | ~5-10 MB (estimated) |
| **Startup Time** | ~5 seconds | ~1 second (estimated) |
| **Thread Model** | Thread-based | Async (Tokio) |
| **Type Safety** | Runtime checks | Compile-time checks |
| **GPIO Library** | RPi.GPIO | rppal |
| **Sensor Library** | w1thermsensor | Custom 1-Wire reader |
| **Redis Client** | redis-py | redis-rs |
| **Metrics** | prometheus_client | prometheus |

## Troubleshooting

### Common Issues

#### 1. Permission Denied (GPIO)

```bash
# Run as root or add user to gpio group
sudo usermod -a -G gpio $USER
# Log out and back in for group to take effect
```

#### 2. 1-Wire Sensors Not Found

```bash
# Verify 1-Wire is enabled
lsmod | grep w1

# Should show:
# w1_gpio
# w1_therm

# If not, enable in /boot/config.txt:
echo "dtoverlay=w1-gpio" | sudo tee -a /boot/config.txt
sudo reboot
```

#### 3. Redis Connection Failed

```bash
# Check Redis is running
sudo systemctl status redis

# Start if not running
sudo systemctl start redis

# Check connectivity
redis-cli ping
# Should respond: PONG
```

### Debug Mode

Enable detailed logging:

```bash
RUST_LOG=trace ./thermos-rust
```

This will show:

- Detailed sensor readings
- GPIO pin state changes
- Redis operations

## Performance Tuning

### Adjusting Control Loop Interval

The default 6-second interval balances responsiveness and system load. To change:

Edit `src/devices/mod.rs`:

```rust
const CONTROL_LOOP_INTERVAL_SECS: u64 = 6;  // Change this value
```

Recommended range: 5-10 seconds

### Hysteresis Tuning

To adjust the dead band:

Edit `src/devices/mod.rs`:

```rust
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.5;  // Increase for wider band
const HYSTERESIS_LOWER_THRESHOLD_C: f32 = -0.5; // Decrease for wider band
```

Wider band = fewer relay cycles, larger temperature swings
Narrower band = more relay cycles, tighter temperature control

## Testing

Thermos-Rust includes comprehensive test coverage with both hardware and cross-platform mock tests.

### Running Tests

```bash
# Run all tests (requires Linux/Raspberry Pi)
cargo test

# Run cross-platform mock tests (works on macOS/Windows)
cargo test --test mock_integration_tests
# Or use the Makefile:
make test-mock

# Run specific test suites
cargo test hysteresis     # Hysteresis logic tests
cargo test sensor         # Sensor tests
cargo test gpio           # GPIO tests
```

### Test Coverage

| Component | Unit Tests | Mock Tests | Integration Tests |
|-----------|------------|------------|-------------------|
| GPIO      | 2 tests    | ✓          | ✓                 |
| Sensors   | 5 tests    | ✓          | ✓                 |
| State     | 5 tests    | ✓          | ✓                 |
| Devices   | 2 tests    | ✓          | 50+ scenarios     |

See [TESTING.md](TESTING.md) for detailed information about mock implementations and cross-platform testing.

## Contributing

Contributions are welcome! Please:

1. Fork the repository
2. Create a feature branch
3. Write tests for new functionality
4. Ensure all tests pass: `cargo test` (requires Linux)
5. Format code: `cargo fmt`
6. Run clippy: `cargo clippy`
7. Submit a pull request

**Note:** Development and testing require Linux with GPIO hardware access.

## License

MIT License - see LICENSE file for details

## Acknowledgments

- Original Python implementation: [Thermos](https://github.com/truncj/thermos)
- GPIO library: [rppal](https://github.com/golemparts/rppal)

## Support

For issues and questions:

- GitHub Issues: [Report a bug](https://github.com/josht/thermos-rust/issues)
- Documentation: [Full API docs](https://docs.rs/thermos-rust)

## Roadmap

- [x] ESP32-C6 bare-metal support with Embassy
- [x] WiFi and MQTT integration (ESP32)
- [x] NVS flash persistence (ESP32)
- [x] I2C BME280 sensor support (ESP32)
- [x] 1-Wire via RMT peripheral (ESP32)
- [x] Grafana dashboard templates
- [ ] Matter integration via rs-matter (ESP32, see [MATTER_INTEGRATION.md](MATTER_INTEGRATION.md))
- [ ] Web UI for configuration and monitoring
- [ ] Docker multi-arch images
- [ ] Home Assistant integration
- [ ] Energy usage tracking
