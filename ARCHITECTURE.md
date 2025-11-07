# Thermos-Rust Architecture

This document describes the internal architecture and design decisions of Thermos-Rust.

## Table of Contents

- [Overview](#overview)
- [Module Structure](#module-structure)
- [Data Flow](#data-flow)
- [Concurrency Model](#concurrency-model)
- [State Management](#state-management)
- [Error Handling](#error-handling)
- [Matter Integration](#matter-integration)
- [Design Decisions](#design-decisions)

## Overview

Thermos-Rust is a multi-zone thermostat control system built with Rust, designed for:

- **Safety**: Memory safety and thread safety via Rust's type system
- **Performance**: Low resource usage suitable for Raspberry Pi
- **Reliability**: Robust error handling and automatic recovery
- **Modularity**: Clean separation of concerns

## Module Structure

```
src/
├── main.rs                 # Application entry point, orchestration
├── config/                 # Configuration management
│   └── mod.rs             # JSON config loading, validation
├── devices/                # Thermostat control logic
│   └── mod.rs             # Hysteresis control, state management
├── gpio/                   # Hardware abstraction
│   └── mod.rs             # GPIO control, relay management
├── sensors/                # Temperature sensing
│   ├── mod.rs             # Sensor trait, composite sensor
│   └── onewire.rs         # 1-Wire DS18B20 implementation
├── persistence/            # State persistence
│   └── mod.rs             # Redis integration
├── metrics/                # Observability
│   └── mod.rs             # Prometheus metrics export
├── matter/                 # Smart home integration
│   └── mod.rs             # Matter protocol support via Connected Home IP
└── mocks/                  # Test mocking infrastructure
    ├── mod.rs             # Mock module exports
    ├── gpio_mock.rs       # Mock GPIO controller
    ├── sensor_mock.rs     # Mock temperature sensors
    └── state_mock.rs      # Mock state manager
```

### Module Responsibilities

| Module | Responsibility | External Dependencies |
|--------|----------------|----------------------|
| `config` | Configuration loading and validation | `serde`, `serde_json` |
| `devices` | Thermostat control logic | `tokio`, `tracing` |
| `gpio` | GPIO hardware interface | `rppal` |
| `sensors` | Temperature reading | `reqwest`, `tokio` |
| `persistence` | State storage | `redis` |
| `metrics` | Metrics export | `prometheus` |
| `matter` | Matter protocol | Connected Home IP (C++ FFI) |
| `mocks` | Test infrastructure | `mockall`, `tokio-test` |

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

Thermos-Rust uses Tokio's async runtime with multiple concurrent tasks:

```
Main Thread (Tokio)
  ├─> Control Loop Task (Zone 1)
  ├─> Control Loop Task (Zone 2)
  ├─> Control Loop Task (Zone N)
  ├─> Matter Server Task
  └─> Metrics Server Task
```

### Shared State

Shared state is protected using `Arc<RwLock<T>>`:

```rust
pub struct Thermostat {
    current_temp: Arc<RwLock<f32>>,
    target_temp: Arc<RwLock<f32>>,
    target_state: Arc<RwLock<HeatingState>>,
    current_state: Arc<RwLock<HeatingState>>,
    // ...
}
```

**Design rationale:**
- `Arc`: Multiple tasks share ownership
- `RwLock`: Multiple readers, single writer (read-heavy workload)
- Alternative considered: `Mutex` (simpler but less performant for reads)

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

### Cluster Implementation

Thermos-Rust implements the Thermostat cluster (0x0201):

| Attribute | Type | Description |
|-----------|------|-------------|
| LocalTemperature | int16 | Current temp (centi-degrees C) |
| OccupiedHeatingSetpoint | int16 | Target temp (centi-degrees C) |
| SystemMode | enum8 | Off (0x00) or Heat (0x04) |
| ThermostatRunningState | bitmap8 | Heat state (bit 0) |
| ControlSequenceOfOperation | enum8 | Heating only (0x00) |
| TemperatureDisplayMode | enum8 | Celsius (0x00) |

### Commissioning Flow

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

### Why Tokio?

- **Industry standard**: Most popular Rust async runtime
- **Feature rich**: Timers, I/O, synchronization primitives
- **Well maintained**: Active development and support
- **Performance**: Efficient task scheduling

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

### Short Term

- [ ] Complete rs-matter integration
- [ ] Hardware abstraction for testing
- [ ] Web configuration UI
- [ ] More comprehensive tests

### Long Term

- [ ] Machine learning for occupancy detection
- [ ] Weather integration for predictive heating
- [ ] Energy usage optimization
- [ ] Multi-protocol support (Zigbee, Z-Wave)

## References

- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [Tokio Documentation](https://tokio.rs/)
- [HVAC Control Theory](https://en.wikipedia.org/wiki/Bang%E2%80%93bang_control)
- [Raspberry Pi GPIO](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html)
