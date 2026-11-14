# Claude Code Instructions for EmberHeat

## Project Overview

EmberHeat is a multi-zone thermostat system for hydronic heating, written in Rust for ESP32-C6.

**Primary Platform:** ESP32-C6 (bare-metal, no_std)
**Language:** Rust 2021 edition
**Architecture:** Cargo workspace with `emberheat-core`, `emberheat-matter`, and `emberheat-esp32`

> **Note:** `emberheat-linux` (Raspberry Pi implementation) is preserved but not actively developed.

## Current Development Status

- **NUM_ZONES = 1** in `emberheat-esp32/src/config.rs` (reduced for Matter commissioning testing; 7-zone config is commented out)
- **USE_MOCK_SENSORS = true** — real DS18B20 support is blocked on `esp-hal-rmt-onewire` compatibility with esp-hal 1.0
- **Matter commissioning works** — tested with Apple Home
- **MQTT works in Matter mode** — enabled via enet-extra-sockets-2 in rs-matter-embassy
- See [GitHub Issues](https://github.com/secretclubhouse/emberheat/issues) for MVP blockers and feature backlog

## Project Structure

```
emberheat/
├── emberheat-core/         # Platform-agnostic core library (no_std, 32 tests)
│   └── src/
│       ├── types.rs        # HeatingState, TemperatureReading, RelayState
│       ├── sensor.rs       # TemperatureSensor trait (async)
│       ├── gpio.rs         # GpioManager, OutputPin traits
│       ├── persistence.rs  # StateStorage trait (async)
│       ├── control.rs      # Hysteresis control algorithm
│       └── config.rs       # HysteresisConfig, ZoneConfig
├── emberheat-matter/       # Matter protocol types and cluster definitions (no_std, 36 tests)
│   └── src/
│       ├── types.rs        # SystemMode, RunningState, temperature conversions, defaults
│       ├── state.rs        # ZoneThermostatState with atomic operations + dirty tracking
│       ├── cluster.rs      # Thermostat cluster (0x0201) definitions and ThermostatHooks trait
│       ├── handler.rs      # ThermostatHandler with out-of-band notifications
│       └── device.rs       # DeviceInfo, endpoint↔zone mapping (MAX_ZONES=16)
├── emberheat-esp32/        # ESP32-C6 implementation (active)
│   └── src/
│       ├── main.rs         # Embassy entry point, zone task spawning (up to 7 static tasks)
│       ├── config.rs       # NUM_ZONES, ZONE_CONFIGS, pins, sensor addresses, Matter credentials
│       ├── controller.rs   # Zone controller logic
│       ├── zone.rs         # Zone task entry points
│       ├── gpio_impl.rs    # esp-hal GPIO (active-low relays)
│       ├── sensors.rs      # MockSensor, SharedOneWireBus (scan/read are TODO stubs)
│       ├── sensor_onewire.rs  # DS18B20 via RMT (written, blocked on dependency)
│       ├── sensor_i2c.rs   # BME280 I2C (simplified calibration)
│       ├── onewire_gpio.rs # 1-Wire via GPIO bit-banging
│       ├── onewire_rmt.rs  # 1-Wire via RMT peripheral (low-level)
│       ├── persistence/    # Flash persistence (sequential-storage, wear-leveled)
│       │   ├── flash_store.rs  # KV store wrapper
│       │   ├── types.rs    # PersistedZoneState (5 bytes: 2x i16 setpoint + u8 mode)
│       │   ├── task.rs     # Background task (checks dirty flag every 5s)
│       │   └── reset.rs    # Factory reset and zone state reset
│       ├── persistence_nvs.rs  # Legacy in-memory NVS (not used with flash persistence)
│       ├── matter/         # Matter integration
│       │   ├── stack.rs    # Matter stack setup, endpoints, cluster handlers (~900 lines)
│       │   ├── zones.rs    # Static zone state array (compile-time sized by NUM_ZONES)
│       │   ├── bridge.rs   # Control loop ↔ Matter state sync functions
│       │   └── mqtt_bridge.rs  # MQTT as UserTask inside Matter stack
│       ├── wifi.rs         # WiFi connectivity (esp-radio)
│       ├── mqtt.rs         # MQTT client and JSON publishing
│       ├── dual_radio.rs   # ESP32-H2 BLE over UART HCI
│       └── mocks.rs        # Test mocks
└── emberheat-linux/        # Linux implementation (preserved, not active)
```

## Build Commands

```bash
# Build emberheat-core
make build

# Build ESP32 firmware (MQTT mode, requires WIFI_SSID/WIFI_PASSWORD in .env.local)
make build-esp32

# Build ESP32 firmware with Matter (WiFi via Matter commissioning, no env vars needed)
make build-matter

# Build Matter with dual-radio (C6 WiFi + H2 BLE over UART)
make build-matter-dual-radio

# Flash Matter firmware and monitor serial output
make flash-monitor-matter

# Run tests (emberheat-core and emberheat-matter)
make test

# Format, lint, test
make dev

# Scan for DS18B20 sensors on 1-Wire bus
make scan-sensors
```

ESP32 feature flags: `matter`, `external-ble`, `onewire-rmt`

To work on `emberheat-linux` explicitly:
```bash
cargo build -p emberheat-linux
cargo test -p emberheat-linux
```

## Critical Concepts

### Zone Configuration

The number of heating zones is configurable (1-16 supported). Currently compile-time only:

1. Edit `emberheat-esp32/src/config.rs`
2. Set `NUM_ZONES` to desired value
3. Ensure `ZONE_CONFIGS` array has exactly `NUM_ZONES` entries
4. Rebuild firmware

**Constraints for adding zones beyond 7:**
- Add corresponding `zoneN_task` function and static cells in `main.rs` (Embassy requires concrete task functions — no dynamic dispatch)
- Ensure sufficient GPIO pins available (~15-20 usable on ESP32-C6)
- Avoid strapping pins (4, 5, 8, 9, 15) and USB pins (12, 13, 16, 17)

Zone states are stored in `emberheat-esp32/src/matter/zones.rs` as a static array of `ZoneThermostatState` sized at compile time by `NUM_ZONES`.

### State Management

Zone state uses lock-free atomics (`ZoneThermostatState` in `emberheat-matter/src/state.rs`):
- `local_temperature` (AtomicI16) — current temp in 0.01°C units
- `occupied_heating_setpoint` (AtomicI16) — heating target
- `occupied_cooling_setpoint` (AtomicI16) — cooling target
- `system_mode` (AtomicU8) — Off/Heat/Cool
- `running_state` (AtomicU16) — heating/cooling active bitmap
- `dirty` (AtomicBool) — set on setpoint/mode change, NOT on temp update

The dirty flag drives persistence — background task checks every 5s and writes dirty zones to flash.

### Flash Persistence Layout

```
Keys 0x0000-0x0FFF: Reserved for Matter (fabrics, network credentials)
Keys 0x1000+:       Zone states (5 bytes each: heating_setpoint, cooling_setpoint, system_mode)
Partition:          0x310000 - 0x320000 (64KB, sequential-storage wear-leveled)
```

### Hysteresis Control

Core temperature control algorithm (prevents relay chatter):

```rust
// Configurable via HysteresisConfig in emberheat-core/src/control.rs
// Default: ±0.5°C dead band around target

if current_temp <= target_temp - 0.5 → Turn ON
if current_temp >= target_temp + 0.5 → Turn OFF
else → Maintain current state (dead band)
```

**Do not change hysteresis values without hardware testing.**

### Sensor Validation

Temperature readings are validated (emberheat-core/src/types.rs):
- Minimum: 3°C (`MIN_VALID_TEMP_C`)
- Maximum: 37°C (`MAX_VALID_TEMP_C`)
- Invalid readings trigger error handling → heating defaults to OFF

### Safety Defaults

- Heating defaults to OFF on errors
- GPIO active-low: relay ON = pin LOW, relay OFF = pin HIGH (fail-safe)
- State persisted to flash for recovery across reboots
- Factory reset via GPIO 9 button (hold 5 seconds)

## Code Style

### Error Handling
- Use `Result<T, E>` for fallible operations
- Use `heapless::String<128>` for no_std error messages
- Domain-specific error types: `SensorError`, `GpioError`, `PersistenceError`, `ControlError`
- Log errors appropriately

### no_std Compatibility
- `emberheat-core` and `emberheat-matter` must remain no_std compatible
- Use `heapless` collections instead of `std` collections
- No allocations in hot paths
- Conditional compilation: `#[cfg(feature = "std")]` / `#[cfg(not(feature = "std"))]`

### Documentation
- Document public APIs with `///`
- Explain "why" not just "what"

## Testing

```bash
# Run emberheat-core (32 tests) and emberheat-matter (36 tests)
make test

# Verbose output
make test-verbose

# Lint (emberheat-core only — ESP32 clippy requires cross-compilation)
make lint
```

## Key Dependencies

**emberheat-core (no_std):**
- `serde` — Serialization (no default features)
- `heapless` 0.9 — No-std collections
- `async-trait` — Async traits

**emberheat-matter (no_std):**
- `rs-matter` 0.4 — Matter protocol (0.4.1, patched via secretclubhouse fork; see Forked dependencies)
- `embassy-sync` 0.8 — Signal for inter-task communication
- `embassy-futures` 0.1 — Async select/Either
- `heapless` 0.9 — No-std collections

**emberheat-esp32:**
- `esp-hal` 1.2 — ESP32-C6 HAL (needs Rust 1.95+)
- `esp-rtos` 0.4 — Embassy runtime + esp-radio + esp-alloc
- `esp-radio` 1.0.0-beta.1 — WiFi + BLE
- `rs-matter-embassy` — Matter Embassy integration (secretclubhouse fork, `emberheat-2026-09` branch)
- `sequential-storage` 3.0 — Wear-leveled flash KV store (pinned to 3 by rs-matter-embassy)
- `rust-mqtt` 0.6 — MQTT v5 client (no_std, supports subscribe)
- `embassy-executor` 0.10, `embassy-time` 0.5, `embassy-net` 0.9

**Forked dependencies (important):**
- `rs-matter`: secretclubhouse/rs-matter `emberheat-2026-11` branch, applied through `[patch.crates-io]` in the root `Cargo.toml` — the 0.4.1 release commit (3f3528e) plus one patch that accepts expired certificates with a warning. Apple Home presents a certificate that expired a year after the home was created; stock 0.4.1 fails commissioning on it with `InvalidTime`.
- `rs-matter-embassy`: secretclubhouse/rs-matter-embassy `emberheat-2026-09` branch — upstream at 2a806b0 (2026-09-28) plus one patch giving MQTT access to the TCP stack. Later upstream commits depend on a patched esp-hal fork, so don't bump past this without checking.

## Common Tasks

### Adding a Sensor Type (ESP32)

1. Create `emberheat-esp32/src/sensor_*.rs`
2. Implement `emberheat_core::TemperatureSensor` trait (async, Send+Sync)
3. Add to `sensors.rs` and wire into zone task in `main.rs`

### Modifying Hysteresis Logic

⚠️ **CRITICAL**: Changes affect physical heating behavior

1. Review `emberheat-core/src/control.rs` — `apply_hysteresis_control()` function and `HysteresisConfig`
2. Write tests for edge cases (boundary conditions, dead band behavior)
3. Test with hardware before deploying
4. Document rationale in commit message

### Modifying Matter State

1. `emberheat-matter/src/state.rs` — `ZoneThermostatState` (atomic state container)
2. `emberheat-matter/src/cluster.rs` — `ThermostatHooks` trait (getter/setter callbacks)
3. `emberheat-esp32/src/matter/stack.rs` — Cluster handler and endpoint setup
4. `emberheat-esp32/src/matter/bridge.rs` — Control loop ↔ Matter sync

### Adding a Zone Beyond 7

1. Add `zone8_task` function in `main.rs` with `#[embassy_executor::task]`
2. Add corresponding `StaticCell` instances for sensor, pin, state
3. Add conditional spawn in `main()` for `NUM_ZONES >= 8`
4. Update `NUM_ZONES` and `ZONE_CONFIGS` in `config.rs`

## Git Workflow

### Commit Messages
```
feat: add humidity sensor support
fix: correct hysteresis boundary condition
docs: update setup instructions
refactor: extract sensor error handling
ci: update security audit workflow
```

### Before Committing
Pre-commit hook runs automatically:
- `cargo fmt` — formatting
- `cargo clippy -p emberheat-core` — linting
- `cargo test -p emberheat-core -p emberheat-matter` — tests

## Safety Reminders

This controls physical heating systems:

- **GPIO conflicts**: Never assign same pin to multiple uses
- **Valid ranges**: Always validate sensor readings
- **Default state**: Heating OFF on errors
- **Test on hardware**: Especially for control logic changes
- **Atomic ordering**: Zone state uses `Relaxed` ordering — sufficient for independent reads/writes but do not add ordering dependencies

---

**When in doubt, default to safe states (heating OFF, error logging).**
