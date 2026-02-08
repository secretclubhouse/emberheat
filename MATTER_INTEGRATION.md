# Matter Integration Architecture

## Overview

Thermos-Rust integrates with the Matter smart home protocol to expose each heating zone as a Matter Thermostat device. The implementation uses [rs-matter](https://github.com/project-chip/rs-matter) with Embassy on ESP32-C6, supporting commissioning via BLE and control via HomeKit, Google Home, Alexa, and other Matter controllers.

## Crate Structure

### `thermos-matter` (no_std library)

Platform-agnostic Matter types and state management:

```
thermos-matter/src/
├── lib.rs       # Module exports
├── types.rs     # MatterTemperature, SystemMode, etc.
├── state.rs     # ZoneThermostatState (atomic operations + dirty tracking)
├── cluster.rs   # Thermostat cluster (0x0201) attribute hooks
├── handler.rs   # Matter handler traits
└── device.rs    # Device info and endpoint mapping
```

### `thermos-esp32/src/matter/` (ESP32-specific)

Platform integration connecting Matter state to hardware:

```
thermos-esp32/src/matter/
├── mod.rs           # Module exports
├── stack.rs         # Matter stack setup, cluster handlers, endpoint config
├── zones.rs         # ZoneStateStore - compile-time sized zone storage
├── bridge.rs        # MatterBridge - connects Matter state to control loops
└── mqtt_bridge.rs   # MQTT in Matter mode (currently disabled)
```

### `thermos-esp32/src/persistence/` (Flash storage)

```
thermos-esp32/src/persistence/
├── mod.rs           # Module exports
├── flash_store.rs   # FlashKvBlobStore - sequential-storage wrapper
├── types.rs         # PersistedZoneState (5 bytes per zone)
├── task.rs          # Background persistence task (checks every 5s)
└── reset.rs         # Factory reset and zone state reset
```

## Data Flow

### Setting Temperature via Matter

```
Matter Controller (e.g., Apple Home)
    │
    ▼
Matter Stack (rs-matter-embassy)
    │  BLE commissioning → WiFi operational
    ▼
Thermostat Cluster Write Handler (stack.rs)
    │  Writes to OccupiedHeatingSetpoint (attr 0x0012)
    ▼
ZoneThermostatState::set_heating_setpoint()
    │  Atomic store (portable-atomic) + sets dirty flag
    ▼
Persistence Task (every 5s)              MatterBridge::get_matter_setpoint()
    │  Checks dirty flag, writes flash       │  Reads atomic state
    ▼                                        ▼
Flash (sequential-storage)               Control Loop
                                             │  Applies hysteresis with new target
                                             ▼
                                         GPIO Relay ON/OFF
```

### Reporting Temperature to Matter

```
Temperature Sensor (1-Wire / I2C / Mock)
    │
    ▼
Control Loop reads sensor
    │
    ▼
MatterBridge::update_matter_from_control_loop()
    │  Updates atomic state with current temp
    ▼
ZoneThermostatState::set_current_temp()
    │  Atomic store
    ▼
Thermostat Cluster Read Handler (stack.rs)
    │  Reads LocalTemperature (attr 0x0000)
    ▼
Matter Stack reports to controller
```

## Endpoint Configuration

Each zone maps to a Matter endpoint:

```
Thermostat Device (Endpoint 0 = Root Node)
├── Root: BasicInformation, GeneralCommissioning,
│         NetworkCommissioning, OperationalCredentials
│
├── Endpoint 1 = Zone 0 - Thermostat + Identify + Binding +
│                         ThermostatUIConfig + Descriptor
├── Endpoint 2 = Zone 1
└── ...
```

Device type: `0x0301` (Thermostat)

## Clusters Implemented

### Thermostat Cluster (0x0201)

**Feature Map: 0x03** (HEAT + COOL)

| Attribute | ID | Type | Access | Range |
|-----------|-----|------|--------|-------|
| LocalTemperature | 0x0000 | Nullable i16 | Read | 0.01°C units |
| AbsMinHeatSetpointLimit | 0x0003 | i16 | Read | 7°C (700) |
| AbsMaxHeatSetpointLimit | 0x0004 | i16 | Read | 30°C (3000) |
| AbsMinCoolSetpointLimit | 0x0005 | i16 | Read | 10°C (1000) |
| AbsMaxCoolSetpointLimit | 0x0006 | i16 | Read | 35°C (3500) |
| OccupiedCoolingSetpoint | 0x0011 | i16 | Read/Write | 10-35°C |
| OccupiedHeatingSetpoint | 0x0012 | i16 | Read/Write | 7-30°C |
| MinHeatSetpointLimit | 0x0015 | i16 | Read/Write | Adjustable |
| MaxHeatSetpointLimit | 0x0016 | i16 | Read/Write | Adjustable |
| MinCoolSetpointLimit | 0x0017 | i16 | Read/Write | Adjustable |
| MaxCoolSetpointLimit | 0x0018 | i16 | Read/Write | Adjustable |
| ControlSequenceOfOperation | 0x001B | enum8 | Read | CoolingAndHeating (0x04) |
| SystemMode | 0x001C | enum8 | Read/Write | Off (0) / Heat (4) / Cool (3) |
| ThermostatRunningState | 0x0029 | bitmap16 | Read | Heating/cooling active |

### Supporting Clusters

| Cluster | ID | Purpose |
|---------|-----|---------|
| Identify | 0x0003 | Device identification (HomeKit requirement) |
| Binding | 0x001E | External sensor support (HomeKit requirement) |
| Thermostat UI Config | 0x0204 | Temperature display format |
| Descriptor | 0x001D | Device type (0x0301) and cluster lists |

## Key Types

### `MatterTemperature` (thermos-matter/src/types.rs)

Matter uses temperatures in 0.01°C units as integers:

```rust
pub struct MatterTemperature(i16);

impl MatterTemperature {
    pub fn from_celsius(celsius: f32) -> Self {
        Self((celsius * 100.0) as i16)
    }

    pub fn to_celsius(&self) -> f32 {
        self.0 as f32 / 100.0
    }
}
```

### `SystemMode` (thermos-matter/src/types.rs)

Maps to Matter Thermostat SystemMode attribute:

```rust
pub enum SystemMode {
    Off = 0,
    Cool = 3,
    Heat = 4,
}
```

### `ZoneThermostatState` (thermos-matter/src/state.rs)

Lock-free state shared between Matter stack, control loop, and persistence task:

```rust
pub struct ZoneThermostatState {
    current_temp: AtomicI16,       // In 0.01°C units
    heating_setpoint: AtomicI16,   // In 0.01°C units
    cooling_setpoint: AtomicI16,   // In 0.01°C units
    system_mode: AtomicU8,         // SystemMode as u8
    running_state: AtomicU16,      // ThermostatRunningState bitmap
    dirty: AtomicBool,             // Needs persistence
}
```

**Why atomics?** The Matter stack, control loop, and persistence task run as separate Embassy tasks. Atomics provide safe concurrent access without mutexes, which is important for real-time control loops that shouldn't block.

## Flash Persistence

### Layout

Uses `sequential-storage` for flash wear-leveling:

- **Partition:** 0x310000 - 0x320000 (64KB)
- **Keys 0x0000-0x0FFF:** Reserved for Matter (fabrics, network credentials)
- **Keys 0x1000+:** Zone states (0x1000 = Zone 0, 0x1001 = Zone 1, etc.)

### Persisted Zone State

```rust
struct PersistedZoneState {
    heating_setpoint: i16,   // 2 bytes
    cooling_setpoint: i16,   // 2 bytes
    system_mode: u8,         // 1 byte
}
// Total: 5 bytes per zone
```

Current temperature and relay state are ephemeral and not persisted.

### Persistence Flow

1. Matter controller writes a new setpoint
2. `ZoneThermostatState` updates atomically and sets dirty flag
3. Background task (every 5s) checks dirty flags
4. Dirty zones are serialized and written to flash
5. On startup, `load_all_zones()` restores saved state before Matter task starts

### Factory Reset

- **Full reset** (`factory_reset()`): Erases entire flash partition + resets Matter stack. All pairings lost.
- **Zone reset** (`reset_zone_states()`): Resets zone setpoints/modes to defaults. Matter pairing preserved.
- Triggered via GPIO 9 button (hold 5 seconds).

## Radio Modes

### Single Radio (default)

ESP32-C6's built-in WiFi and BLE time-share the radio:
- BLE active during commissioning
- WiFi active during normal operation
- Managed by rs-matter-embassy coexistence layer

### Dual Radio

ESP32-C6 handles WiFi, external ESP32-H2 handles BLE over UART HCI:
- Better performance (no time-sharing)
- Requires additional H2 module connected via UART (115200 baud)
- Build with `make build-matter-dual-radio`

## Configuration

### Matter Credentials (thermos-esp32/src/config.rs)

```rust
pub const MATTER_DISCRIMINATOR: u16 = 3840;
pub const MATTER_PASSCODE: u32 = 20202021;
pub const MATTER_VENDOR_ID: u16 = 0xFFF1;  // Test vendor ID
pub const MATTER_PRODUCT_ID: u16 = 0x8001;
pub const MATTER_DEVICE_NAME: &str = "Thermos Multi-Zone";
```

Supports up to 5 Matter fabrics (`max-fabrics-5`).

## Known Limitations

1. **MQTT disabled in Matter mode** — rs-matter-embassy socket pool (4 sockets) is fully consumed by Matter UDP, DHCP, and DNS, leaving no room for TCP. Requires forking rs-matter-embassy to increase `ENET_MIN_SOCKET_SET`.

2. **Dependencies on forks** — Uses sysgrok/rs-matter (next branch) for Embassy 0.5+ compatibility and secretclubhouse/rs-matter-embassy (stack-public branch) for TCP stack access.

## Design Decisions

### Why Lock-Free Atomics?

The control loop runs every 5 seconds and must not be delayed. Using a mutex could cause priority inversion if the Matter stack holds the lock during a long operation. Atomics guarantee the control loop never blocks.

### Why Compile-Time Zone Count?

`no_std` environment without an allocator. All zone state must be statically allocated. The const generic `N` in `ZoneStateStore<N>` ensures the array is sized at compile time.

### Why Separate Crates?

- `thermos-matter` can be tested on the host without ESP32 toolchain
- Clean dependency boundaries (Matter types don't depend on hardware)
- `thermos-core` remains pure control logic, testable anywhere
