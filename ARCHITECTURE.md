# EmberHeat Architecture

This document describes the internal architecture and design decisions of EmberHeat.

## Table of Contents

- [Overview](#overview)
- [Workspace Structure](#workspace-structure)
- [Module Structure](#module-structure)
- [Data Flow](#data-flow)
- [Concurrency Model](#concurrency-model)
- [State Management](#state-management)
- [Error Handling](#error-handling)
- [Matter Integration](#matter-integration)
- [Design Decisions](#design-decisions)

## Overview

EmberHeat is a multi-zone thermostat control system built with Rust, designed for:

- **Safety**: Memory safety and thread safety via Rust's type system
- **Performance**: Low resource usage suitable for Raspberry Pi and ESP32
- **Reliability**: Robust error handling and automatic recovery
- **Modularity**: Clean separation of concerns
- **Portability**: Multi-platform support via trait-based abstraction

## Workspace Structure

The project is organized as a Cargo workspace with four crates:

```
emberheat/
├── emberheat-core/       # Platform-agnostic core library (no_std compatible)
├── emberheat-matter/     # Matter protocol types and cluster definitions (no_std)
├── emberheat-esp32/      # ESP32-C6 bare-metal implementation (active)
└── emberheat-linux/      # Linux/Raspberry Pi implementation (preserved, not active)
```

### emberheat-core

Platform-agnostic library containing shared types and control logic. Designed to be `no_std` compatible for embedded use.

**Key exports:**
- `HeatingState`, `RelayState`, `TemperatureReading` - Core types
- `TemperatureSensor` trait - Sensor abstraction
- `GpioManager` trait - GPIO abstraction
- `StateStorage` trait - Persistence abstraction
- `apply_hysteresis_control()` - Control algorithm
- `HysteresisConfig` - Configuration types

### emberheat-linux

Full-featured implementation for Raspberry Pi with:
- Tokio async runtime
- rppal GPIO library
- 1-Wire via sysfs (`/sys/bus/w1/devices/`)
- HTTP fallback sensors via reqwest
- Redis state persistence
- Prometheus metrics export

### emberheat-esp32

Bare-metal `no_std` implementation for ESP32-C6 with:
- Embassy async executor
- esp-hal GPIO
- 1-Wire via RMT peripheral (hardware-timed, more reliable)
- I2C BME280 sensor support
- NVS flash persistence
- WiFi connectivity
- MQTT temperature logging

## Module Structure

### emberheat-core (platform-agnostic)

```
emberheat-core/src/
├── lib.rs              # Library exports
├── types.rs            # HeatingState, RelayState, TemperatureReading
├── sensor.rs           # TemperatureSensor trait
├── gpio.rs             # GpioManager trait  
├── persistence.rs      # StateStorage trait
├── control.rs          # Hysteresis control algorithm
└── config.rs           # HysteresisConfig, ZoneConfig
```

### emberheat-linux (Raspberry Pi)

```
emberheat-linux/src/
├── main.rs             # Tokio entry point, orchestration
├── config/mod.rs       # JSON config loading, validation
├── devices/mod.rs      # Thermostat bridge, control loops
├── gpio/mod.rs         # rppal GPIO, relay management
├── sensors/
│   ├── mod.rs          # Composite sensor, HTTP sensor
│   └── onewire.rs      # 1-Wire sysfs implementation
├── persistence/mod.rs  # Redis integration
├── metrics/mod.rs      # Prometheus metrics export
└── mocks/              # Test infrastructure
    ├── gpio_mock.rs
    ├── sensor_mock.rs
    └── state_mock.rs
```

### emberheat-esp32 (bare-metal)

```
emberheat-esp32/src/
├── main.rs               # Embassy entry point, zone task spawning
├── lib.rs                # Library exports
├── config.rs             # Zone configuration (NUM_ZONES, pins, sensor addresses)
├── controller.rs         # Zone controller logic
├── zone.rs               # Zone task entry points
├── gpio_impl.rs          # esp-hal GPIO implementation
├── sensors.rs            # MockSensor, SharedOneWireBus, SharedDs18b20Sensor
├── sensor_onewire.rs     # DS18B20 1-Wire via RMT peripheral
├── sensor_i2c.rs         # BME280 I2C sensor
├── onewire_gpio.rs       # 1-Wire via GPIO bit-banging
├── onewire_rmt.rs        # 1-Wire via RMT peripheral (low-level)
├── persistence_nvs.rs    # Legacy NVS storage (in-memory)
├── persistence/          # Flash persistence (active)
│   ├── mod.rs            # Module exports
│   ├── flash_store.rs    # sequential-storage KV wrapper
│   ├── types.rs          # PersistedZoneState (5 bytes per zone)
│   ├── task.rs           # Background persistence task (every 5s)
│   └── reset.rs          # Factory reset and zone state reset
├── matter/               # Matter protocol integration
│   ├── mod.rs            # Module exports
│   ├── stack.rs          # Matter stack setup, endpoints, cluster handlers
│   ├── zones.rs          # Static zone state array (compile-time sized)
│   ├── bridge.rs         # Control loop ↔ Matter state sync
│   └── mqtt_bridge.rs    # MQTT as Matter UserTask
├── wifi.rs               # WiFi connectivity (esp-radio)
├── mqtt.rs               # MQTT client and publishing
├── dual_radio.rs         # ESP32-H2 BLE over UART HCI
└── mocks.rs              # Test mocks
```

### emberheat-matter (Matter protocol)

```
emberheat-matter/src/
├── lib.rs              # Library exports
├── types.rs            # Matter temperature/mode types, defaults
├── state.rs            # ZoneThermostatState with atomic operations
├── cluster.rs          # Thermostat cluster (0x0201) definitions and hooks
├── handler.rs          # Matter handler traits
└── device.rs           # Device info and endpoint mapping
```

### Module Responsibilities

#### emberheat-core

| Module | Responsibility | Dependencies |
|--------|----------------|--------------|
| `types` | Core data types | `serde` (no_std) |
| `sensor` | Sensor trait definition | `async-trait` |
| `gpio` | GPIO trait definition | None |
| `persistence` | Storage trait definition | `async-trait` |
| `control` | Hysteresis algorithm | None |
| `config` | Configuration types | `serde` |

#### emberheat-linux

| Module | Responsibility | Dependencies |
|--------|----------------|--------------|
| `config` | JSON config loading/validation | `serde`, `serde_json` |
| `devices` | Thermostat control loops | `embassy_time`, `embassy_sync`, `tracing` |
| `gpio` | Raspberry Pi GPIO | `rppal` |
| `sensors` | 1-Wire sysfs, HTTP fallback | `reqwest`, `embassy_time` |
| `persistence` | Redis state storage | `redis`, `tokio` |
| `metrics` | Prometheus export | `prometheus` |
| `mocks` | Test infrastructure | `embassy_time` |

#### emberheat-esp32

| Module | Responsibility | Dependencies |
|--------|----------------|--------------|
| `config` | Compile-time zone configuration | None |
| `controller` | Zone control logic | `emberheat-core` |
| `zone` | Zone task entry points | `embassy-executor` |
| `gpio_impl` | ESP32 GPIO control | `esp-hal` |
| `sensors` | Mock sensor, shared 1-Wire bus | `embassy-sync` |
| `sensor_onewire` | DS18B20 1-Wire via RMT | `esp-hal-rmt-onewire` |
| `sensor_i2c` | BME280 I2C sensor | `bme280` |
| `persistence/` | Flash state persistence | `sequential-storage` |
| `matter/stack` | Matter stack and endpoints | `rs-matter-embassy` |
| `matter/bridge` | Control loop ↔ Matter sync | `emberheat-matter` |
| `matter/mqtt_bridge` | MQTT inside Matter stack | `rust-mqtt` |
| `wifi` | WiFi connectivity | `esp-radio` |
| `mqtt` | MQTT client and publishing | `rust-mqtt` |
| `dual_radio` | H2 BLE over UART HCI | `bt-hci` |

## Data Flow

### Startup Sequence (ESP32)

```
main()
  ├─> Load compile-time configuration (config.rs)
  ├─> Initialize GPIO relay pins
  ├─> Initialize flash persistence
  │   └─> Load persisted zone states (setpoints, modes)
  ├─> Initialize sensors (1-Wire or I2C)
  ├─> Initialize Matter stack (if matter feature enabled)
  │   ├─> Set up WiFi via BLE commissioning
  │   ├─> Register thermostat endpoints (one per zone)
  │   └─> Start MQTT bridge for telemetry
  ├─> Spawn control loop tasks (one per zone)
  └─> Run Embassy executor
```

### Control Loop (Per Zone)

```
Every 6 seconds:
  ├─> Read temperature from sensor
  │   ├─> 1-Wire via RMT (or I2C BME280)
  │   └─> Validate range (3°C - 37°C)
  ├─> Get setpoint from Matter state (if enabled)
  ├─> Apply hysteresis control logic
  │   ├─> Calculate bounds (target ± 0.5°C)
  │   ├─> Determine desired relay state
  │   └─> Update relay if state changed
  ├─> Update Matter state with current temperature
  ├─> Publish telemetry via MQTT
  └─> Persist state to flash (on changes)
```

### Matter Integration

```
HomeKit Controller
        ↓
  Matter Protocol
        ↓
  MatterServer
        ↓
  ThermostatAccessory
        ↓
  Thermostat Device
```

## Concurrency Model

### Threading Architecture

**ESP32 (active):** Uses Embassy async executor with cooperative multitasking:

```
Embassy Executor
  ├─> Zone Control Task (Zone 1)
  ├─> Zone Control Task (Zone 2)
  ├─> Zone Control Task (Zone N)
  ├─> Matter Stack Task
  ├─> MQTT Bridge Task
  └─> Persistence Task
```

**Linux (preserved):** Uses Tokio async runtime:

```
Main Thread (Tokio)
  ├─> Control Loop Task (Zone 1..N)
  ├─> Matter Server Task
  └─> Metrics Server Task
```

### Shared State

**ESP32 (active):** Zone state uses lock-free atomics via `ZoneThermostatState`:

```rust
pub struct ZoneThermostatState {
    local_temperature: AtomicI16,         // Current temp (0.01°C units)
    occupied_heating_setpoint: AtomicI16, // Heating target
    occupied_cooling_setpoint: AtomicI16, // Cooling target
    system_mode: AtomicU8,               // Off/Heat/Cool
    running_state: AtomicU16,            // Heating/cooling active bitmap
    dirty: AtomicBool,                   // Needs persistence
}
```

**Design rationale:**
- `Relaxed` atomic ordering — sufficient for independent readers/writers
- No mutex contention — control loop never blocks waiting for Matter stack
- Dirty flag tracking — persistence task only writes to flash when state changes
- Static allocation — zone state array sized at compile time by `NUM_ZONES`

The 1-Wire bus is shared across zone tasks using `embassy_sync::Mutex<CriticalSectionRawMutex>` to serialize bus access.

**Linux (preserved):** Uses `Arc<Mutex<T>>` from Embassy for shared state between Tokio tasks.

### GPIO Access

**ESP32:** Each zone task owns its relay pin directly (no shared GPIO controller). Pins are initialized in `main()` and moved into zone tasks via `StaticCell`.

**Linux (preserved):** GPIO controller is shared via `Arc<GpioController>` with interior mutability for relay state updates.

## State Management

### In-Memory State

Each thermostat maintains in-memory state:

```rust
struct Thermostat {
    current_temp: f32,      // Latest sensor reading
    target_temp: f32,       // User-set target
    target_state: HeatingState,  // User requested state
    current_state: HeatingState, // Actual relay state
}
```

### Persistent State

**ESP32 (active):** State is persisted to flash via `sequential-storage` (wear-leveled key-value store):

```rust
// 5 bytes per zone, stored at key 0x1000 + zone_index
struct PersistedZoneState {
    heating_setpoint: i16,   // Matter centi-degrees (0.01°C)
    cooling_setpoint: i16,   // Matter centi-degrees (0.01°C)
    system_mode: u8,         // 0=Off, 4=Heat, 3=Cool
}

// Flash layout:
// Keys 0x0000-0x0FFF: Reserved for Matter (fabrics, network credentials)
// Keys 0x1000+:       Zone states (0x1000 = Zone 0, 0x1001 = Zone 1, ...)
// Partition: 0x310000 - 0x320000 (64KB)
```

**Persistence strategy:**
- Dirty-flag tracking: Only writes when state has changed
- Background task: Checks every 5 seconds for dirty zones
- Load on startup: Restores state before Matter stack starts
- Current temperature and relay state are ephemeral (not persisted)

**Linux (preserved):** State is persisted to Redis as JSON with `ThermostatState` structs.

### State Recovery

On startup, each thermostat:
1. Attempts to load state from flash (ESP32) or Redis (Linux)
2. Falls back to defaults if not found
3. Validates loaded state
4. Applies state to hardware (GPIO)

## Error Handling

### Error Types

```rust
// Domain-specific errors
pub enum SensorError {
    Timeout,
    OutOfRange(f32),
    NotFound(String),
    // ...
}

pub enum GpioError {
    PinInUse(u8),
    PinNotFound(u8),
    HardwareError(String),
}
```

### Error Handling Strategy

| Error Type | Strategy | Rationale |
|------------|----------|-----------|
| Sensor read failure | Power cycle + default temp | Sensors can hang, power cycle usually fixes |
| GPIO error | Log + continue | Critical but recovery possible |
| Persistence error | Log + continue | Can operate without persistence temporarily |
| Config error | Fail fast | Invalid config = unsafe operation |

### Recovery Mechanisms

**Sensor Failures:**
```rust
1. Detect invalid reading
2. Log error + increment metric
3. Initiate power cycle (VCC off 3s, on + wait 5s)
4. Track reset state to prevent concurrent resets
5. Use default temperature until recovery
```

**Network Failures:**
- MQTT: Retry with backoff, control loop continues without telemetry
- Matter: Buffer state changes, resync on reconnect

## Matter Integration

Matter is fully implemented on ESP32-C6 using `rs-matter` (pure Rust). The implementation lives in `emberheat-matter/` (protocol types and cluster definitions) and `emberheat-esp32/src/matter/` (hardware integration). See [MATTER_INTEGRATION.md](MATTER_INTEGRATION.md) for details.

### Cluster Implementation (Thermostat 0x0201)

Feature Map: `0x03` (HEAT + COOL)

| Attribute | Type | Description |
|-----------|------|-------------|
| LocalTemperature | Nullable i16 | Current temp (0.01°C units, NULL = 0x8000) |
| OccupiedHeatingSetpoint | i16 | Heating target (7-30°C) |
| OccupiedCoolingSetpoint | i16 | Cooling target (10-35°C) |
| SystemMode | enum8 | Off (0x00) / Cool (0x03) / Heat (0x04) |
| ThermostatRunningState | bitmap16 | Heating (bit 0), cooling (bit 1) |
| ControlSequenceOfOperation | enum8 | CoolingAndHeating (0x04) |
| MinHeatSetpointLimit | i16 | 7°C (700) |
| MaxHeatSetpointLimit | i16 | 30°C (3000) |

### Commissioning Flow

```
1. ESP32 starts Matter stack with BLE commissioning
2. QR code + pairing code displayed via serial
3. User scans QR code in HomeKit / Google Home / etc.
4. Matter commissioning handshake over BLE
5. Device joins Matter fabric, WiFi credentials transferred
6. Thermostat zones appear as individual accessories
```

### Radio Modes

- **Single radio** (default): ESP32-C6 WiFi + BLE time-shared on the same radio
- **Dual radio**: ESP32-C6 handles WiFi, external ESP32-H2 handles BLE over UART HCI

## Design Decisions

### Why Rust?

- **Memory safety**: Eliminates entire classes of bugs
- **Performance**: ~10x lower memory usage than Python
- **Concurrency**: Fearless concurrency with ownership system
- **Reliability**: Type system catches errors at compile time

### Why Embassy?

The project uses Embassy as its primary async runtime:

**ESP32 (active):**
- **Bare-metal async**: Cooperative multitasking without an OS
- **rs-matter compatible**: rs-matter library uses Embassy primitives
- **Low overhead**: Minimal memory footprint for embedded use

**Linux (preserved):** Uses a hybrid Tokio + Embassy approach:
- Tokio for network I/O (Redis, HTTP) and OS integration
- Embassy primitives for portability with ESP32 code

```rust
// Time primitives from Embassy
use embassy_time::{Timer, Duration, Ticker};
Timer::after(Duration::from_secs(3)).await;

// Sync primitives from Embassy
use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
```

### Why Redis? (Linux only)

- **Simple**: Key-value store, easy to use
- **Fast**: In-memory, microsecond latency
- **Reliable**: Persistent storage options
- **Ubiquitous**: Well-supported, easy to deploy

On ESP32, NVS flash is used instead (no external dependencies).

### Why Matter (vs HAP)?

| Factor | Matter | HAP |
|--------|--------|-----|
| Ecosystems | Apple, Google, Amazon | Apple only |
| Standard | Open, royalty-free | Proprietary |
| Future-proof | Industry backing | Apple-specific |
| Complexity | Higher | Lower |

### Hysteresis Constants

**Choice:** ±0.5°C dead band

**Rationale:**
- Prevents relay chatter (relays have limited lifetime)
- Balances comfort vs energy efficiency
- Standard HVAC practice
- Tested in Python version

### Control Loop Interval

**Choice:** 6 seconds

**Rationale:**
- Sensor read time: ~0.1-0.5s
- Temperature changes slowly
- Reduces CPU/power usage
- Quick enough for responsive control
- Tested in Python version

## Performance Characteristics

### Memory Usage

| Component | Python | Rust | Notes |
|-----------|--------|------|-------|
| Base runtime | ~20 MB | ~2 MB | Interpreter vs binary |
| Per zone | ~10 MB | ~500 KB | Object overhead |
| 3 zones total | ~50 MB | ~5 MB | Estimated |

### CPU Usage

- Idle: <1% (waiting on timers)
- Control loop: ~2-5% per zone during sensor read
- Matter: <1% (event-driven)

### Startup Time (ESP32)

- Configuration load: ~10ms
- GPIO init: ~50ms per pin
- NVS flash init: ~20ms
- WiFi connect: ~2-5s
- Matter init: ~500ms
- Total: ~3-6s

## Future Improvements

See [GitHub Issues](https://github.com/secretclubhouse/emberheat/issues) for the full backlog.

### Completed

- [x] Hardware abstraction layer (HAL) for ESP32 portability
- [x] Embassy primitives for bare-metal compatibility
- [x] ESP32-C6 bare-metal implementation
- [x] 1-Wire and I2C sensor driver code
- [x] WiFi and MQTT integration on ESP32
- [x] Flash persistence with wear-leveling on ESP32
- [x] Complete rs-matter integration (ESP32)
- [x] MQTT telemetry in Matter mode
- [x] Dual-radio support (C6 WiFi + H2 BLE)
- [x] Migrate legacy `src/` to workspace crates

### MVP Remaining

- [ ] Resolve esp-hal-rmt-onewire compatibility ([#1](https://github.com/secretclubhouse/emberheat/issues/1))
- [ ] Implement SharedOneWireBus scan/read ([#2](https://github.com/secretclubhouse/emberheat/issues/2))
- [ ] Re-enable multi-zone with real sensors ([#3](https://github.com/secretclubhouse/emberheat/issues/3))
- [ ] End-to-end hardware integration test ([#4](https://github.com/secretclubhouse/emberheat/issues/4))

### Short Term

- [ ] Runtime zone configuration via NVS ([#5](https://github.com/secretclubhouse/emberheat/issues/5))
- [ ] Sensor auto-discovery and assignment ([#6](https://github.com/secretclubhouse/emberheat/issues/6))
- [ ] OTA firmware updates ([#7](https://github.com/secretclubhouse/emberheat/issues/7))

### Long Term

- [ ] Time-based scheduling ([#9](https://github.com/secretclubhouse/emberheat/issues/9))
- [ ] Web configuration UI ([#11](https://github.com/secretclubhouse/emberheat/issues/11))
- [ ] PID control mode ([#12](https://github.com/secretclubhouse/emberheat/issues/12))
- [ ] Energy usage tracking ([#14](https://github.com/secretclubhouse/emberheat/issues/14))

## References

- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rs-matter](https://github.com/project-chip/rs-matter) — Pure Rust Matter implementation
- [Embassy](https://embassy.dev/) — Async embedded framework for Rust
- [HVAC Control Theory](https://en.wikipedia.org/wiki/Bang%E2%80%93bang_control)
- [ESP32-C6 Datasheet](https://www.espressif.com/en/products/socs/esp32-c6)
