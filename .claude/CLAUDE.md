# Claude Code Instructions for EmberHeat

## Project Overview

EmberHeat is a multi-zone thermostat system for hydronic heating, written in Rust for ESP32-C6.

**Primary Platform:** ESP32-C6 (bare-metal, no_std)
**Language:** Rust 2021 edition
**Architecture:** Cargo workspace with `emberheat-core`, `emberheat-matter`, and `emberheat-esp32`

> **Note:** `emberheat-linux` (Raspberry Pi implementation) is preserved but not actively developed.

## Project Structure

```
emberheat/
├── emberheat-core/         # Platform-agnostic core library (no_std)
│   └── src/
│       ├── types.rs        # HeatingState, TemperatureReading, etc.
│       ├── sensor.rs       # TemperatureSensor trait
│       ├── gpio.rs         # GpioManager trait
│       ├── persistence.rs  # StateStorage trait
│       └── control.rs      # Hysteresis control algorithm
├── emberheat-matter/       # Matter protocol types and cluster definitions (no_std)
│   └── src/
│       ├── types.rs        # Matter temperature/mode types
│       ├── state.rs        # ZoneThermostatState with atomic operations
│       ├── cluster.rs      # Thermostat cluster (0x0201) hooks
│       ├── handler.rs      # Matter handler traits
│       └── device.rs       # Device info and endpoint mapping
├── emberheat-esp32/        # ESP32-C6 implementation (active)
│   └── src/
│       ├── main.rs         # Embassy entry point
│       ├── config.rs       # Zone configuration (NUM_ZONES, ZONE_CONFIGS)
│       ├── gpio_impl.rs    # esp-hal GPIO
│       ├── sensor_onewire.rs   # 1-Wire via RMT
│       ├── sensor_i2c.rs   # BME280 I2C sensor
│       ├── persistence_nvs.rs  # NVS flash storage
│       ├── wifi.rs         # WiFi connectivity
│       ├── mqtt.rs         # MQTT logging
│       └── matter/         # Matter integration
│           ├── zones.rs    # Zone state storage
│           └── bridge.rs   # Bridge between Matter and control loop
└── emberheat-linux/        # Linux implementation (preserved, not active)
```

## Build Commands

Default commands only include `emberheat-core` and `emberheat-esp32`:

```bash
# Build emberheat-core
make build

# Build ESP32 firmware
make build-esp32

# Flash to ESP32
make flash

# Flash and monitor
make flash-monitor

# Run tests (emberheat-core and emberheat-matter)
make test

# Format, lint, test
make dev
```

To work on `emberheat-linux` explicitly:
```bash
cargo build -p emberheat-linux
cargo test -p emberheat-linux
```

## Critical Concepts

### Zone Configuration

The number of heating zones is configurable (1-16 supported). To change:

1. Edit `emberheat-esp32/src/config.rs`
2. Set `NUM_ZONES` to desired value
3. Ensure `ZONE_CONFIGS` array has exactly `NUM_ZONES` entries
4. Rebuild firmware

**Constraints for adding zones beyond 7:**
- Add corresponding `zoneN_task` function and static cells in `main.rs` (Embassy requires concrete task functions)
- Ensure sufficient GPIO pins available (~15-20 usable on ESP32-C6)
- Memory overhead is ~1-2KB per zone (not typically a constraint)

Zone states are stored in `emberheat-esp32/src/matter/zones.rs` and sized at compile time based on `NUM_ZONES`.

### Hysteresis Control

Core temperature control algorithm (prevents relay chatter):

```rust
// Constants in emberheat-core/src/control.rs
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.5;   // Turn OFF at target+0.5°C
const HYSTERESIS_LOWER_THRESHOLD_C: f32 = -0.5;  // Turn ON at target-0.5°C
const DEFAULT_TARGET_TEMP_C: f32 = 21.1;         // 70°F

// Logic
if current_temp <= target_temp - 0.5 {
    // Too cold → Turn ON
} else if current_temp >= target_temp + 0.5 {
    // Too hot → Turn OFF
} else {
    // Dead band → Maintain current state
}
```

**Do not change hysteresis values without hardware testing.**

### Sensor Validation

Temperature readings are validated:
- Minimum: 3°C
- Maximum: 37°C
- Invalid readings trigger error handling

### Safety Defaults

- Heating defaults to OFF on errors
- GPIO cleanup on shutdown
- State persisted to NVS flash for recovery

## Code Style

### Error Handling
- Use `Result<T, E>` for fallible operations
- Use `heapless::String` for no_std error messages
- Log errors appropriately

### no_std Compatibility
- `emberheat-core` must remain no_std compatible
- Use `heapless` collections instead of `std` collections
- No allocations in hot paths

### Documentation
- Document public APIs with `///`
- Explain "why" not just "what"

## Testing

```bash
# Run emberheat-core and emberheat-matter tests
make test

# Verbose output
make test-verbose

# Lint
make lint
```

## Common Tasks

### Adding a Sensor Type (ESP32)

1. Create `emberheat-esp32/src/sensor_*.rs`
2. Implement sensor using Embassy async and embedded-hal-async
3. Add to main control loop in `main.rs`

### Modifying Hysteresis Logic

⚠️ **CRITICAL**: Changes affect heating behavior

1. Review constants in `emberheat-core/src/control.rs`
2. Write tests for edge cases
3. Test with hardware before deploying
4. Document rationale in commit message

## Git Workflow

### Commit Messages
```
feat: add humidity sensor support
fix: correct hysteresis boundary condition
docs: update setup instructions
refactor: extract sensor error handling
```

### Before Committing
Pre-commit hook runs automatically:
- `cargo fmt` - formatting
- `cargo clippy -p emberheat-core` - linting
- `cargo test -p emberheat-core` - tests

## Key Dependencies

**emberheat-core (no_std):**
- `serde` - Serialization
- `heapless` - No-std collections

**emberheat-matter (no_std):**
- `portable-atomic` - Cross-platform atomics for state management

**emberheat-esp32:**
- `esp-hal` - ESP32 HAL
- `embassy-executor` - Async runtime
- `esp-hal-rmt-onewire` - 1-Wire via RMT
- `bme280` - I2C sensor
- `rust-mqtt` - MQTT client
- `esp-wifi` - WiFi

## Safety Reminders

This controls physical heating systems:

- **GPIO conflicts**: Never assign same pin to multiple uses
- **Valid ranges**: Always validate sensor readings
- **Default state**: Heating OFF on errors
- **Test on hardware**: Especially for control logic changes

---

**When in doubt, default to safe states (heating OFF, error logging).**
