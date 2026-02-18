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

The project is organized as a Cargo workspace with three crates:

```
emberheat/
├── emberheat-core/       # Platform-agnostic core library (no_std compatible)
├── emberheat-linux/      # Linux/Raspberry Pi implementation  
├── emberheat-esp32/      # ESP32-C6 bare-metal implementation
└── src/                # Legacy single-crate (being migrated)
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
├── main.rs             # Embassy entry point
├── lib.rs              # Library exports
├── gpio_impl.rs        # esp-hal GPIO implementation
├── sensor_onewire.rs   # 1-Wire via RMT peripheral
├── sensor_i2c.rs       # BME280 I2C sensor
├── persistence_nvs.rs  # NVS flash storage
├── wifi.rs             # WiFi connectivity
├── mqtt.rs             # MQTT temperature logging
└── mocks.rs            # Test mocks
```

### Legacy src/ (being migrated)

```
src/
├── main.rs             # Application entry point
├── config/mod.rs       # Configuration management
├── devices/mod.rs      # Hysteresis control, state management
├── gpio/mod.rs         # GPIO control, relay management
├── hal/                # Hardware Abstraction Layer
│   ├── mod.rs          # HAL traits (OutputPin, Platform)
│   ├── linux.rs        # Linux/rppal implementation
│   └── stub.rs         # Stub for testing
├── sensors/
│   ├── mod.rs          # Sensor trait, composite sensor
│   └── onewire.rs      # 1-Wire DS18B20 (sysfs)
├── persistence/mod.rs  # Redis integration
├── metrics/mod.rs      # Prometheus metrics export
├── matter/mod.rs       # Matter protocol stub (not implemented)
└── mocks/              # Test infrastructure
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
| `gpio_impl` | ESP32 GPIO control | `esp-hal` |
| `sensor_onewire` | 1-Wire via RMT | `esp-hal-rmt-onewire` |
| `sensor_i2c` | BME280 I2C sensor | `bme280` |
| `persistence_nvs` | NVS flash storage | `esp-hal` |
| `wifi` | WiFi connectivity | `esp-wifi` |
| `mqtt` | MQTT logging | `rust-mqtt` |

## Data Flow

### Startup Sequence

```
main()
  ├─> Load configuration (config.json)
  ├─> Initialize GPIO controller
  ├─> Initialize Redis state manager
  ├─> Create thermostat bridge
  │   ├─> Create thermostat for each zone
  │   │   ├─> Load persisted state
  │   │   ├─> Initialize sensors
  │   │   └─> Initialize GPIO pins
  │   └─> Initialize VCC controller
  ├─> Initialize Matter server
  │   └─> Register thermostat accessories
  ├─> Start Prometheus metrics server
  ├─> Spawn control loops (one per zone)
  └─> Wait for shutdown signal
```

### Control Loop (Per Zone)

```
Every 6 seconds:
  ├─> Read temperature from sensor
  │   ├─> Try 1-Wire sensor
  │   └─> Fallback to HTTP sensor (if configured)
  ├─> Validate temperature range
  │   └─> If invalid: Trigger power cycle
  ├─> Apply hysteresis control logic
  │   ├─> Calculate bounds (target ± 0.5°C)
  │   ├─> Determine desired relay state
  │   └─> Update relay if state changed
  ├─> Update metrics (Prometheus)
  │   ├─> Current temperature
  │   ├─> Target temperature
  │   ├─> Heat status
  │   └─> Response time
  └─> Persist state (Redis)
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

EmberHeat uses Tokio's async runtime with multiple concurrent tasks:

```
Main Thread (Tokio)
  ├─> Control Loop Task (Zone 1)
  ├─> Control Loop Task (Zone 2)
  ├─> Control Loop Task (Zone N)
  ├─> Matter Server Task
  └─> Metrics Server Task
```

### Shared State

Shared state is protected using `Arc<Mutex<T>>` from Embassy:

```rust
use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;

pub struct Thermostat {
    current_temp: Arc<Mutex<NoopRawMutex, f32>>,
    target_temp: Arc<Mutex<NoopRawMutex, f32>>,
    target_state: Arc<Mutex<NoopRawMutex, HeatingState>>,
    current_state: Arc<Mutex<NoopRawMutex, HeatingState>>,
    // ...
}
```

**Design rationale:**
- `Arc`: Multiple tasks share ownership
- `embassy_sync::Mutex`: Portable sync primitives for future ESP32 support
- `NoopRawMutex`: Appropriate for std targets with OS scheduling; use `CriticalSectionRawMutex` on bare-metal

### GPIO Access

GPIO controller is shared via `Arc<GpioController>`:

```rust
pub struct GpioController {
    relays: Arc<Mutex<HashMap<u8, RelayController>>>,
    vcc_controller: Arc<Mutex<Option<VccController>>>,
}
```

**Design rationale:**
- Single GPIO instance to prevent conflicts
- Interior mutability for relay state updates
- `Mutex` used (not `RwLock`) because GPIO operations are fast

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

### Persistent State (Redis)

State is persisted to Redis on changes:

```json
{
  "relay_pin": 23,
  "temp_pin": 4,
  "temp_id": "28-00000ab12345",
  "target_temp": 21.1,
  "target_state": 1,
  "current_state": 1,
  "last_updated": "2024-01-15T10:30:00Z"
}
```

**Persistence strategy:**
- Write-through: State saved immediately on changes
- Read on startup: Recover previous state after restart
- Asynchronous: Non-blocking writes

### State Recovery

On startup, each thermostat:
1. Attempts to load state from Redis
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
| Redis error | Log + continue | Can operate without persistence temporarily |
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
- Redis: Queue writes, retry on reconnect
- HTTP sensors: Fall back to 1-Wire sensor
- Matter: Buffer state changes, resync on reconnect

## Matter Integration

**Current Status:** Matter support is a **stub implementation** that is not functional. The `src/matter/mod.rs` file logs warnings and waits indefinitely without providing actual Matter protocol support.

**Future Plan:** Matter integration is planned for ESP32-C6 using the `rs-matter` pure Rust library. See [MATTER_INTEGRATION.md](MATTER_INTEGRATION.md) for implementation details.

### Planned Cluster Implementation (Thermostat 0x0201)

| Attribute | Type | Description |
|-----------|------|-------------|
| LocalTemperature | int16 | Current temp (centi-degrees C) |
| OccupiedHeatingSetpoint | int16 | Target temp (centi-degrees C) |
| SystemMode | enum8 | Off (0x00) or Heat (0x04) |
| ThermostatRunningState | bitmap8 | Heat state (bit 0) |
| ControlSequenceOfOperation | enum8 | Heating only (0x00) |
| TemperatureDisplayMode | enum8 | Celsius (0x00) |

### Planned Commissioning Flow (not yet implemented)

```
1. App starts Matter server
2. Generate QR code + pairing code
3. User scans QR code in HomeKit
4. Matter commissioning handshake
5. Device joins Matter fabric
6. Thermostats appear in Home app
```

## Design Decisions

### Why Rust?

- **Memory safety**: Eliminates entire classes of bugs
- **Performance**: ~10x lower memory usage than Python
- **Concurrency**: Fearless concurrency with ownership system
- **Reliability**: Type system catches errors at compile time

### Why Tokio + Embassy Hybrid?

The project uses a hybrid approach combining Tokio and Embassy primitives:

**Tokio (executor and network):**
- **Industry standard**: Required by `redis` and `reqwest` crates
- **Network I/O**: Async TCP, TLS, and HTTP support
- **Linux optimized**: Best performance on Raspberry Pi

**Embassy (sync and time primitives):**
- **Portability**: Same primitives work on bare-metal ESP32
- **rs-matter compatible**: rs-matter library uses Embassy primitives
- **Future-ready**: Enables ESP32-C6 port with minimal changes

```rust
// Time primitives from Embassy
use embassy_time::{Timer, Duration, Ticker};
Timer::after(Duration::from_secs(3)).await;

// Sync primitives from Embassy  
use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
```

**Tokio runtime bridge**: Redis and reqwest operations run on a dedicated
Tokio runtime, bridged via `tokio::runtime::Handle::block_on()` when called
from Embassy-based code.

### Why Redis?

- **Simple**: Key-value store, easy to use
- **Fast**: In-memory, microsecond latency
- **Reliable**: Persistent storage options
- **Ubiquitous**: Well-supported, easy to deploy

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

### Startup Time

- Configuration load: ~10ms
- GPIO init: ~50ms per pin
- Redis connect: ~100ms
- Matter init: ~500ms
- Total: ~1s (vs ~5s for Python)

## Future Improvements

### Completed

- [x] Hardware abstraction layer (HAL) for ESP32 portability
- [x] Embassy primitives for bare-metal compatibility
- [x] ESP32-C6 bare-metal implementation
- [x] 1-Wire via RMT peripheral on ESP32
- [x] I2C BME280 sensor support on ESP32
- [x] WiFi and MQTT integration on ESP32
- [x] NVS flash persistence on ESP32

### Short Term

- [ ] Complete rs-matter integration (ESP32)
- [ ] Web configuration UI
- [ ] More comprehensive tests
- [ ] Migrate legacy `src/` to workspace crates

### Long Term

- [ ] Weather integration for predictive heating
- [ ] Energy usage optimization
- [ ] Home Assistant integration

## References

- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [Tokio Documentation](https://tokio.rs/)
- [HVAC Control Theory](https://en.wikipedia.org/wiki/Bang%E2%80%93bang_control)
- [Raspberry Pi GPIO](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)
