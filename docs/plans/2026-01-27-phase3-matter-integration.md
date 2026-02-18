# Phase 3: Matter Integration on ESP32

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Integrate rs-matter-embassy with emberheat-esp32 to enable Apple Home, Google Home, and Alexa control of the thermostat.

**Approach:** Start with 1 zone to validate integration, expand to 3 zones for test harness, keep architecture flexible for up to 16 zones.

**Prerequisites:**
- Phase 2 complete (emberheat-matter crate with types, state, handlers)
- WiFi and MQTT already working on ESP32-C6
- Test harness available with 3 zones

---

## Dependencies

Use the **sysgrok forks** of the rs-matter ecosystem, which have been updated for Embassy 0.5+ compatibility:

```toml
# In emberheat-esp32/Cargo.toml

[dependencies]
rs-matter-embassy = { git = "https://github.com/sysgrok/rs-matter-embassy", features = ["esp", "log"] }

[patch.crates-io]
rs-matter = { git = "https://github.com/sysgrok/rs-matter", branch = "next" }
rs-matter-stack = { git = "https://github.com/sysgrok/rs-matter-stack", branch = "next" }
```

These match existing emberheat-esp32 versions:
- embassy-time 0.5 ✓
- embassy-executor 0.9 ✓
- esp-hal 1.0 ✓
- esp-radio 0.17 ✓

---

## Architecture

The Matter stack runs as a separate Embassy task alongside existing control loop and MQTT tasks. State is shared via existing `ZoneThermostatState` atomics.

```
┌─────────────────────────────────────────────────────────────────┐
│                        Embassy Runtime                          │
├─────────────────┬─────────────────┬─────────────────────────────┤
│  Control Loop   │   MQTT Task     │      Matter Stack           │
│  (per zone)     │                 │  (rs-matter-embassy)        │
│                 │                 │                             │
│  - Read sensor  │  - Publish      │  - WiFi transport           │
│  - Hysteresis   │    state        │  - BLE commissioning        │
│  - Toggle relay │  - Periodic     │  - Thermostat cluster(s)    │
└────────┬────────┴────────┬────────┴──────────────┬──────────────┘
         │                 │                       │
         └─────────────────┴───────────────────────┘
                           │
              ┌────────────▼────────────┐
              │  ZoneThermostatState    │
              │  (atomic operations)    │
              │                         │
              │  - local_temperature    │
              │  - setpoint             │
              │  - system_mode          │
              │  - heating (running)    │
              └─────────────────────────┘
```

### Data Flow

1. **Matter → Thermostat:** HomeKit sends setpoint change → Matter handler writes to `ZoneThermostatState` → Control loop picks it up on next iteration

2. **Thermostat → Matter:** Sensor reads temperature → Control loop updates `ZoneThermostatState` → Matter handler reads it and notifies subscribed controllers

3. **MQTT:** Continues unchanged - reads from same `ZoneThermostatState`

The existing `ZoneThermostatState` with atomic operations already provides thread-safe sharing between tasks. No new synchronization needed.

---

## Cluster Handler Integration

### Design Decision: Thin Wrapper (Option B)

rs-matter-embassy uses a handler chaining pattern. We'll create a thin wrapper in emberheat-esp32 that delegates to `ZoneThermostatState`, rather than adapting the existing `ThermostatHandler` in emberheat-matter.

**Rationale:**
- Simpler to debug during initial integration
- Matches rs-matter-embassy example patterns
- Direct access to rs-matter primitives

**Trade-offs documented for future refactor:**

| Aspect | Option A: Adapt ThermostatHandler | Option B: Thin Wrapper (chosen) |
|--------|-----------------------------------|--------------------------------|
| Code location | emberheat-matter | emberheat-esp32 |
| Abstraction | Higher - our traits | Lower - rs-matter traits directly |
| Testability | Can unit test on host | Requires ESP32 or mocking |
| Maintenance | Single handler implementation | May duplicate logic |
| Flexibility | Works with any Matter stack | Tied to rs-matter-embassy |

**Future refactor path:** Once integration is stable, consider moving handler logic back to emberheat-matter by implementing rs-matter's `AsyncHandler` trait on our `ThermostatHandler`. This would enable host-based testing and better separation of concerns.

### Handler Pattern

```rust
// rs-matter's chaining pattern
let handler = EmptyHandler
    .chain(ENDPOINT_0, descriptor::ID, DescHandler::new(&desc))
    .chain(ENDPOINT_1, thermostat::ID, ZoneThermostatHandler::new(&zone_states[0]))
    .chain(ENDPOINT_2, thermostat::ID, ZoneThermostatHandler::new(&zone_states[1]))
    // ... more zones
```

---

## File Structure

```
emberheat-esp32/src/
├── matter/
│   ├── mod.rs              # (modify) Add stack module export
│   ├── bridge.rs           # (existing) Already has sync functions
│   ├── zones.rs            # (existing) Zone state storage
│   └── stack.rs            # (NEW) Matter stack integration
│       ├── ZoneThermostatHandler   # Thin wrapper for rs-matter
│       ├── create_matter_stack()   # Stack initialization
│       └── matter_task()           # Embassy task entry point
├── config.rs               # (modify) Add Matter commissioning config
├── main.rs                 # (modify) Spawn matter_task
└── Cargo.toml              # (modify) Add rs-matter-embassy deps
```

---

## Configuration Additions

```rust
// In config.rs

// Matter commissioning configuration
pub const MATTER_DISCRIMINATOR: u16 = 3840;  // Used in QR code/NFC
pub const MATTER_PASSCODE: u32 = 20202021;   // Default test passcode

// Note: For production, these should be unique per device and
// stored in secure flash. Test values are fine for development.
```

---

## Implementation Tasks

### Phase 3a: Single Zone Proof-of-Concept

#### Task 1: Add rs-matter-embassy Dependencies

**File:** `emberheat-esp32/Cargo.toml`

Add dependencies and patch section:

```toml
[dependencies]
# ... existing deps ...

# Matter protocol stack with Embassy integration
rs-matter-embassy = { git = "https://github.com/sysgrok/rs-matter-embassy", features = ["esp", "log"] }

[patch.crates-io]
rs-matter = { git = "https://github.com/sysgrok/rs-matter", branch = "next" }
rs-matter-stack = { git = "https://github.com/sysgrok/rs-matter-stack", branch = "next" }
```

**Validation:** `cargo check -p emberheat-esp32 --target riscv32imac-unknown-none-elf` compiles

**Commit:** `feat(matter): add rs-matter-embassy dependencies`

---

#### Task 2: Add Matter Configuration Constants

**File:** `emberheat-esp32/src/config.rs`

Add Matter commissioning configuration:

```rust
// =============================================================================
// Matter Configuration
// =============================================================================

/// Matter discriminator for commissioning (12-bit value, 0-4095)
/// Used in QR code and DNS-SD advertisement
pub const MATTER_DISCRIMINATOR: u16 = 3840;

/// Matter setup passcode for commissioning (27-bit value)
/// Default test passcode - change for production devices
pub const MATTER_PASSCODE: u32 = 20202021;

/// Matter vendor ID (0xFFF1-0xFFF4 are test vendor IDs)
pub const MATTER_VENDOR_ID: u16 = 0xFFF1;

/// Matter product ID (test product ID)
pub const MATTER_PRODUCT_ID: u16 = 0x8001;

/// Device name shown during commissioning
pub const MATTER_DEVICE_NAME: &str = "EmberHeat Multi-Zone";
```

**Commit:** `feat(matter): add commissioning configuration constants`

---

#### Task 3: Create ZoneThermostatHandler

**File:** `emberheat-esp32/src/matter/stack.rs` (new file)

Create thin wrapper that implements rs-matter's handler traits:

```rust
//! Matter stack integration for emberheat-esp32
//!
//! Provides the glue between rs-matter-embassy and our zone thermostat states.
//! This is a thin wrapper - see docs/plans/2026-01-27-phase3-matter-integration.md
//! for rationale and future refactor path.

use rs_matter::data_model::cluster_basic_information::BasicInfoConfig;
use rs_matter::data_model::objects::*;
use rs_matter::data_model::sdm::dev_att::DevAttDataFetcher;
use rs_matter::data_model::system_model::descriptor;
use rs_matter::error::Error;
use rs_matter::tlv::{FromTLV, TLVElement, ToTLV};
use rs_matter::transport::exchange::Exchange;

use emberheat_matter::ZoneThermostatState;
use emberheat_matter::types::{temperature, SystemMode};

use crate::config::{
    MATTER_DEVICE_NAME, MATTER_DISCRIMINATOR, MATTER_PASSCODE,
    MATTER_PRODUCT_ID, MATTER_VENDOR_ID, NUM_ZONES,
};
use crate::matter::zones::ZONE_STATES;

/// Thermostat cluster ID
pub const CLUSTER_THERMOSTAT: u32 = 0x0201;

/// Thermostat attribute IDs (heating-only subset)
mod attr {
    pub const LOCAL_TEMPERATURE: u16 = 0x0000;
    pub const OCCUPIED_HEATING_SETPOINT: u16 = 0x0012;
    pub const MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0015;
    pub const MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0016;
    pub const CONTROL_SEQUENCE_OF_OPERATION: u16 = 0x001B;
    pub const SYSTEM_MODE: u16 = 0x001C;
    pub const THERMOSTAT_RUNNING_STATE: u16 = 0x0029;
}

/// Handler for a single zone's thermostat cluster
pub struct ZoneThermostatHandler {
    endpoint: u16,
}

impl ZoneThermostatHandler {
    pub fn new(endpoint: u16) -> Self {
        Self { endpoint }
    }

    /// Get the zone state for this handler's endpoint
    fn zone_state(&self) -> Option<&'static ZoneThermostatState> {
        let index = (self.endpoint - 1) as usize;
        ZONE_STATES.get(index)
    }
}

// TODO: Implement AsyncHandler trait from rs-matter
// This will be filled in during Task 4 once we understand the exact
// trait signatures from rs-matter-embassy

/// Basic device information for Matter
pub fn basic_info_config() -> BasicInfoConfig<'static> {
    BasicInfoConfig {
        vid: MATTER_VENDOR_ID,
        pid: MATTER_PRODUCT_ID,
        hw_ver: 1,
        sw_ver: 1,
        sw_ver_str: "0.1.0",
        serial_no: "EMBERHEAT-001",
        device_name: MATTER_DEVICE_NAME,
        vendor_name: "EmberHeat",
        product_name: "Multi-Zone Thermostat",
    }
}
```

**Note:** The actual handler trait implementation will be completed in Task 4 after examining rs-matter-embassy's exact API.

**Commit:** `feat(matter): create ZoneThermostatHandler skeleton`

---

#### Task 4: Implement Matter Handler Traits

**File:** `emberheat-esp32/src/matter/stack.rs`

Study rs-matter-embassy's handler pattern from `light_wifi.rs` example and implement the required traits for `ZoneThermostatHandler`.

Key methods to implement:
- `read()` - Return attribute values from `ZoneThermostatState`
- `write()` - Update setpoint and mode in `ZoneThermostatState`
- `invoke()` - Handle any thermostat commands (likely none for heating-only)

**Research needed:** Examine rs-matter's `AsyncHandler` trait and how `OnOffHandler` implements it.

**Commit:** `feat(matter): implement thermostat cluster handler`

---

#### Task 5: Create Matter Task Entry Point

**File:** `emberheat-esp32/src/matter/stack.rs`

Add the Embassy task that runs the Matter stack:

```rust
use embassy_executor::Spawner;
use rs_matter_embassy::EmbassyWifiMatterStack;

/// Matter stack instance (statically allocated due to size ~35-50KB)
static MATTER_STACK: StaticCell<EmbassyWifiMatterStack<...>> = StaticCell::new();

/// Embassy task running the Matter protocol stack
#[embassy_executor::task]
pub async fn matter_task(/* wifi driver, etc */) {
    // 1. Initialize Matter stack with basic info
    // 2. Create handler chain with thermostat endpoints
    // 3. Create persister for commissioning state
    // 4. Run stack with run_coex() for concurrent commissioning

    // Follows pattern from rs-matter-embassy/examples/esp/src/bin/light_wifi.rs
}
```

**Commit:** `feat(matter): add matter_task entry point`

---

#### Task 6: Integrate Matter Task in main.rs

**File:** `emberheat-esp32/src/main.rs`

Spawn the Matter task alongside existing tasks:

```rust
// In main() after WiFi initialization:
spawner.spawn(matter_task(/* ... */)).unwrap();
```

Ensure WiFi driver is shared correctly between MQTT and Matter (both need network access).

**Commit:** `feat(matter): spawn matter task in main`

---

#### Task 7: Test Single Zone End-to-End

**Validation steps:**

1. Flash firmware to ESP32-C6
2. Open Apple Home app (or Google Home / Alexa)
3. Add accessory → Scan QR code or enter manual code
4. Verify device appears as "EmberHeat Multi-Zone"
5. Check current temperature displays
6. Adjust setpoint → verify relay responds
7. Toggle mode Off/Heat → verify behavior
8. Check MQTT still publishes correctly

**Expected QR code info:**
- Discriminator: 3840
- Passcode: 20202021

**Commit:** (none - validation only)

---

### Phase 3b: Expand to 3 Zones

#### Task 8: Add Handlers for Zones 2-3

**File:** `emberheat-esp32/src/matter/stack.rs`

Extend handler chain for endpoints 2 and 3:

```rust
let handler = EmptyHandler
    .chain(ENDPOINT_0, descriptor::ID, DescHandler::new(&desc))
    .chain(1, CLUSTER_THERMOSTAT, ZoneThermostatHandler::new(1))
    .chain(2, CLUSTER_THERMOSTAT, ZoneThermostatHandler::new(2))
    .chain(3, CLUSTER_THERMOSTAT, ZoneThermostatHandler::new(3));
```

**File:** `emberheat-esp32/src/config.rs`

Update zone count:
```rust
pub const NUM_ZONES: usize = 3;
```

**Commit:** `feat(matter): add endpoints for 3 zones`

---

#### Task 9: Test 3-Zone Configuration

**Validation:**

1. All 3 zones appear as separate thermostats in Home app
2. Each zone controls independently
3. Setpoint change on Zone 2 doesn't affect Zone 1 or 3
4. MQTT publishes state for all 3 zones
5. Control loops run independently per zone

**Commit:** (none - validation only)

---

### Phase 3c: Configurable N Zones

#### Task 10: Generalize Handler Chain Generation

**File:** `emberheat-esp32/src/matter/stack.rs`

Create macro or helper to generate handler chains based on `NUM_ZONES`:

```rust
/// Create handler chain for configured number of zones
macro_rules! zone_handlers {
    ($base:expr, $($zone:expr),*) => {
        $base
        $(
            .chain($zone, CLUSTER_THERMOSTAT, ZoneThermostatHandler::new($zone))
        )*
    };
}

// Usage depends on NUM_ZONES at compile time
```

**Alternative:** If macros are too complex, document the copy/paste pattern for adding zones.

**Commit:** `feat(matter): generalize handler chain for N zones`

---

#### Task 11: Document Zone Expansion

**File:** `CLAUDE.md` or inline docs

Document:
- How to add zones 8-16 (requires adding `zoneN_task` functions in main.rs)
- Embassy's requirement for concrete task functions
- Memory considerations (~1-2KB per zone)
- GPIO pin availability on ESP32-C6

**Commit:** `docs: document zone expansion process`

---

## Validation Checklist

Before Phase 3 is complete:

- [ ] Device commissions via BLE (Apple Home, Google Home, or Alexa)
- [ ] Temperature readings appear in Home app
- [ ] Setpoint changes from Home app reach control loop
- [ ] Mode toggle (Off/Heat) works
- [ ] Running state (heating indicator) updates in Home app
- [ ] MQTT continues working alongside Matter
- [ ] All configured zones (1, 3, or more) appear as separate thermostats
- [ ] Power cycle preserves commissioning state (NVS)
- [ ] System runs stable for extended period (hours)

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| rs-matter-embassy API instability | Code breaks on updates | Pin to specific git commit, not just branch |
| BLE commissioning failures | Can't pair device | Test with multiple controller apps early |
| WiFi conflicts (MQTT + Matter) | Network issues | Both use embassy-net, should share cleanly |
| Memory pressure | Stack overflow, OOM | Profile heap usage, adjust rs-matter-embassy sizing features |
| Handler trait mismatch | Won't compile | Study rs-matter examples carefully before implementing |

---

## References

- [rs-matter-embassy](https://github.com/sysgrok/rs-matter-embassy) - Embassy integration
- [rs-matter-embassy light_wifi.rs example](https://github.com/sysgrok/rs-matter-embassy/blob/master/examples/esp/src/bin/light_wifi.rs)
- [rs-matter](https://github.com/sysgrok/rs-matter) (sysgrok fork, next branch)
- [Matter Thermostat Cluster Spec](https://github.com/project-chip/connectedhomeip/blob/master/data_model/1.5/clusters/Thermostat.xml)
- [Phase 2 Plan](./2026-01-27-phase2-matter-thermostat-cluster.md)
- [Main Design Doc](./2026-01-26-esp32-matter-thermostat-design.md)
