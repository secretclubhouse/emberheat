# Phase 2: Matter Thermostat Cluster Implementation

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a heating-only thermostat cluster implementation for rs-matter that can be integrated with the ESP32 multi-zone thermostat from Phase 1.

**Architecture:** Implement Matter Thermostat Cluster (0x0201) as a standalone Rust module with heating-only attributes, designed to integrate with rs-matter on ESP32-C6. The cluster will bridge Matter commands to emberheat-core's hysteresis control.

**Tech Stack:** rs-matter, esp-hal 1.0, Embassy async runtime

**Prerequisites:** 
- Phase 1 complete (multi-zone thermostat with real DS18B20 sensors working)
- ESP32-C6 DevKitC-1 with 7 zones tested and stable

---

## Background Research Summary

### rs-matter Ecosystem

The Matter implementation in Rust consists of several crates:
- **rs-matter** - Core Matter protocol library ([GitHub](https://github.com/project-chip/rs-matter))
- **rs-matter-stack** - Stack assembly utilities (has embassy-time version conflicts with esp-rtos)
- **rs-matter-embassy** - Embassy async integration for MCUs ([GitHub](https://github.com/sysgrok/rs-matter-embassy))

### Thermostat Cluster Status

**Key Finding:** rs-matter DOES have the Thermostat cluster defined in its IDL files, but no built-in handler implementation. We can:
1. Use `rs_matter::import!(Thermostat)` to get cluster definition and types
2. Implement `ThermostatHooks` trait (similar to `OnOffHooks` pattern)
3. Create `ThermostatHandler` struct that manages the state machine

The available built-in handlers are:
- OnOffHandler (lights) - good reference implementation
- LevelControlHandler (dimmers)
- DescHandler (descriptors)

### Handler Pattern (from OnOff cluster analysis)

rs-matter uses a "Hooks" pattern for device-specific logic:

```rust
// The Hooks trait defines device-specific callbacks
pub trait ThermostatHooks {
    const CLUSTER: Cluster<'static>;  // From import! macro
    
    fn local_temperature(&self) -> Option<i16>;
    fn occupied_heating_setpoint(&self) -> i16;
    fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), Error>;
    fn system_mode(&self) -> SystemModeEnum;
    fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error>;
    fn thermostat_running_state(&self) -> RelayStateBitmap;
    
    async fn run<F: Fn(OutOfBandMessage)>(&self, notify: F);
}

// The Handler struct uses hooks and implements ClusterAsyncHandler
pub struct ThermostatHandler<H: ThermostatHooks> {
    hooks: H,
    dataver: Dataver,
    endpoint_id: EndptId,
}
```

### Memory Requirements

From rs-matter-embassy documentation:
- HEAP_SIZE: ~140KB for ESP32
- Stack: ~35-50KB for Matter stack
- BUMP_SIZE: ~16.5KB for futures allocation

With Phase 1's thermostat using ~30KB, total should fit within ESP32-C6's 512KB SRAM.

### Dependency Note

**rs-matter-stack** has embassy-time version conflicts with esp-rtos (requires 0.1.x, we use 0.5.x). Using rs-matter directly without the stack utilities. Manual integration will be needed for Phase 3.

---

## Matter Thermostat Cluster Specification

### Cluster ID: 0x0201 (513)

**Attributes (heating-only subset we'll implement):**

| Attribute | ID | Type | Access | Default | Description |
|-----------|----|------|--------|---------|-------------|
| LocalTemperature | 0x0000 | int16 | Read | null | Current temp in 0.01°C units |
| OccupiedHeatingSetpoint | 0x0012 | int16 | R/W | 2000 (20°C) | Target temp in 0.01°C |
| MinHeatSetpointLimit | 0x0015 | int16 | Read | 700 (7°C) | Minimum allowed setpoint |
| MaxHeatSetpointLimit | 0x0016 | int16 | Read | 3000 (30°C) | Maximum allowed setpoint |
| ControlSequenceOfOperation | 0x001B | enum8 | Read | 0x02 | HeatingOnly |
| SystemMode | 0x001C | enum8 | R/W | 0x04 | Off=0x00, Heat=0x04 |
| ThermostatRunningState | 0x0029 | bitmap16 | Read | 0 | Bit0=Heat active |

**Temperature Conversion:**
```rust
// Celsius to Matter centi-degrees (0.01°C units)
fn celsius_to_matter(temp_c: f32) -> i16 {
    (temp_c * 100.0) as i16
}

// Matter centi-degrees to Celsius
fn matter_to_celsius(centi_c: i16) -> f32 {
    (centi_c as f32) / 100.0
}

// Examples:
// 21.5°C → 2150
// 2150 → 21.5°C
```

**SystemMode Values:**
- 0x00: Off
- 0x01: Auto (not used for heating-only)
- 0x03: Cool (not used)
- 0x04: Heat
- 0x05: Emergency Heat (not used)

**ThermostatRunningState Bits:**
- Bit 0: Heat (1 = heating active)
- Bit 1: Cool (not used)
- Bit 2: Fan (not used)
- Bits 3-5: Second stage (not used)

**Feature Flags (from IDL):**
```rust
bitmap Feature : bitmap32 {
    kHeating = 0x1;        // We support this
    kCooling = 0x2;        // Not supported
    kOccupancy = 0x4;      // Not supported
    kScheduleConfiguration = 0x8;  // Not supported
    kSetback = 0x10;       // Not supported
    kAutoMode = 0x20;      // Not supported
    kLocalTemperatureNotExposed = 0x40;  // Not used
    kMatterScheduleConfiguration = 0x80; // Not supported
    kPresets = 0x100;      // Not supported
}
```

---

## Implementation Tasks

### Task 1: Set Up rs-matter Development Environment ✅ COMPLETED

**Status:** Done - rs-matter dependency added, matter module created.

**What was done:**
- Added `rs-matter` with `rustcrypto` feature to Cargo.toml
- Note: rs-matter-stack has embassy-time version conflicts, using rs-matter directly
- Created `emberheat-esp32/src/matter/mod.rs` module structure

---

### Task 2: Implement Thermostat Type Definitions ✅ COMPLETED

**Status:** Done - types.rs with all Matter thermostat types.

**What was done:**
- Cluster ID (0x0201) and device type ID (0x0301)
- Attribute ID constants
- SystemMode enum with conversions
- ControlSequence enum
- RunningState bitmap
- Temperature conversion utilities
- Default values (20°C setpoint, 7-30°C limits)
- Unit tests

---

### Task 3: Implement Thermostat State Container ✅ COMPLETED

**Status:** Done - thread-safe ZoneThermostatState.

**What was done:**
- `ZoneThermostatState` with atomic operations
- Local temperature, setpoint, system mode, running state
- Setpoint validation with `SetpointError`
- Celsius conversion helpers
- Unit tests

---

### Task 4: Import Thermostat Cluster and Create Hooks Trait

**Files:**
- Create: `emberheat-esp32/src/matter/cluster.rs`
- Modify: `emberheat-esp32/src/matter/mod.rs`

**Step 1: Import Thermostat cluster from IDL**

Create `emberheat-esp32/src/matter/cluster.rs`:
```rust
//! Thermostat cluster definition imported from Matter IDL
//!
//! Uses rs-matter's import! macro to generate cluster metadata and types
//! from the official Matter specification.

// Import the Thermostat cluster from Matter IDL
// This generates:
// - FULL_CLUSTER: Cluster<'static> with all attributes/commands
// - AttributeId, CommandId enums
// - Feature bitmap
// - All struct/enum types (SystemModeEnum, RelayStateBitmap, etc.)
rs_matter::import!(Thermostat);

use rs_matter::dm::Cluster;

/// Heating-only thermostat cluster configuration
/// 
/// Uses FULL_CLUSTER but restricts to heating-only features and attributes.
pub const HEATING_ONLY_CLUSTER: Cluster<'static> = FULL_CLUSTER
    .with_features(Feature::HEATING.bits())
    .with_attrs(with!(
        required,
        LocalTemperature,
        OccupiedHeatingSetpoint,
        MinHeatSetpointLimit,
        MaxHeatSetpointLimit,
        ControlSequenceOfOperation,
        SystemMode,
        ThermostatRunningState
    ));

/// Re-export commonly used types
pub use self::thermostat::{
    AttributeId,
    CommandId,
    ControlSequenceOfOperationEnum,
    Feature,
    RelayStateBitmap,
    SystemModeEnum,
};
```

**Step 2: Create ThermostatHooks trait**

Add to `emberheat-esp32/src/matter/cluster.rs`:
```rust
use rs_matter::dm::Cluster;
use rs_matter::error::Error;
use rs_matter::tlv::Nullable;
use core::future::Future;

/// Messages for out-of-band state changes (physical button, timer, etc.)
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OutOfBandMessage {
    /// Temperature reading updated
    TemperatureUpdate,
    /// Setpoint changed externally
    SetpointChanged,
    /// Heating state changed
    HeatingStateChanged,
    /// System mode changed
    ModeChanged,
}

/// Trait for device-specific thermostat logic
///
/// Implement this trait to provide the actual thermostat behavior.
/// The handler will call these methods to read/write state.
///
/// Pattern based on rs-matter's OnOffHooks trait.
pub trait ThermostatHooks {
    /// Cluster metadata for this thermostat implementation
    const CLUSTER: Cluster<'static>;

    // --- Temperature ---
    
    /// Get current local temperature in Matter centi-degrees (0.01°C)
    /// Returns None if sensor is unavailable
    fn local_temperature(&self) -> Nullable<i16>;

    // --- Setpoint ---
    
    /// Get occupied heating setpoint in centi-degrees
    fn occupied_heating_setpoint(&self) -> i16;
    
    /// Set occupied heating setpoint in centi-degrees
    /// Returns error if out of valid range
    fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), Error>;
    
    /// Get minimum heat setpoint limit
    fn min_heat_setpoint_limit(&self) -> i16;
    
    /// Get maximum heat setpoint limit
    fn max_heat_setpoint_limit(&self) -> i16;

    // --- System Mode ---
    
    /// Get current system mode (Off, Heat, etc.)
    fn system_mode(&self) -> SystemModeEnum;
    
    /// Set system mode
    /// For heating-only, only Off and Heat are valid
    fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error>;

    // --- Running State ---
    
    /// Get thermostat running state bitmap
    fn thermostat_running_state(&self) -> RelayStateBitmap;

    // --- Control Sequence ---
    
    /// Get control sequence of operation
    /// For heating-only, always returns HeatingOnly
    fn control_sequence_of_operation(&self) -> ControlSequenceOfOperationEnum {
        ControlSequenceOfOperationEnum::HeatingOnly
    }

    // --- Background Task ---
    
    /// Background task for out-of-band notifications
    ///
    /// This future MUST NOT return. Implementations should loop forever
    /// or await core::future::pending::<()>().
    ///
    /// Call `notify(OutOfBandMessage::TemperatureUpdate)` when the temperature
    /// changes from the control loop so Matter can push updates to controllers.
    async fn run<F: Fn(OutOfBandMessage)>(&self, _notify: F) {
        core::future::pending::<()>().await
    }
}
```

**Step 3: Update mod.rs**

```rust
pub mod cluster;

pub use cluster::{
    ThermostatHooks,
    OutOfBandMessage,
    HEATING_ONLY_CLUSTER,
    AttributeId,
    SystemModeEnum,
    RelayStateBitmap,
    ControlSequenceOfOperationEnum,
};
```

**Step 4: Verify compilation**

Run: `cd emberheat-esp32 && WIFI_SSID="test" WIFI_PASSWORD="test" cargo check`
Expected: Compiles with rs-matter types

**Step 5: Commit**

```bash
git add emberheat-esp32/src/matter/
git commit -m "feat(matter): import thermostat cluster and create ThermostatHooks trait

- Use rs_matter::import!(Thermostat) for IDL-based cluster definition
- Create HEATING_ONLY_CLUSTER with restricted feature set
- Define ThermostatHooks trait following OnOffHooks pattern
- Add OutOfBandMessage enum for async notifications"
```

---

### Task 5: Implement ThermostatHandler

**Files:**
- Create: `emberheat-esp32/src/matter/handler.rs`
- Modify: `emberheat-esp32/src/matter/mod.rs`

**Step 1: Create ThermostatHandler struct**

Create `emberheat-esp32/src/matter/handler.rs`:
```rust
//! Matter Thermostat Cluster handler implementation
//!
//! Implements the thermostat cluster using the ThermostatHooks trait
//! for device-specific logic. Based on rs-matter's OnOffHandler pattern.

use core::cell::Cell;
use core::pin::pin;

use embassy_futures::select::{select, Either};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::signal::Signal;

use rs_matter::dm::{Cluster, Dataver, HandlerContext, ReadContext, WriteContext};
use rs_matter::dm::types::EndptId;
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::Nullable;

use crate::matter::cluster::{
    ThermostatHooks, OutOfBandMessage, 
    AttributeId, SystemModeEnum, RelayStateBitmap,
    ControlSequenceOfOperationEnum,
};

/// Handler for Matter Thermostat Cluster (0x0201)
///
/// This handler manages the thermostat state machine and bridges
/// Matter commands to the device-specific ThermostatHooks implementation.
pub struct ThermostatHandler<H: ThermostatHooks> {
    dataver: Dataver,
    endpoint_id: EndptId,
    hooks: H,
    state_change_signal: Signal<NoopRawMutex, OutOfBandMessage>,
}

impl<H: ThermostatHooks> ThermostatHandler<H> {
    /// Create a new ThermostatHandler
    pub fn new(dataver: Dataver, endpoint_id: EndptId, hooks: H) -> Self {
        Self {
            dataver,
            endpoint_id,
            hooks,
            state_change_signal: Signal::new(),
        }
    }

    /// Validate cluster configuration on init
    fn validate(&self) {
        // Ensure required attributes are present
        if H::CLUSTER.attribute(AttributeId::LocalTemperature as _).is_none() {
            panic!("Thermostat validation: missing required attribute: LocalTemperature");
        }
        if H::CLUSTER.attribute(AttributeId::SystemMode as _).is_none() {
            panic!("Thermostat validation: missing required attribute: SystemMode");
        }
    }

    /// Initialize the handler
    pub fn init(&self) {
        self.validate();
    }

    /// Notify that state has changed (for out-of-band updates)
    pub fn notify_state_changed(&self, message: OutOfBandMessage) {
        self.state_change_signal.signal(message);
    }

    /// Get current local temperature
    pub fn local_temperature(&self) -> Nullable<i16> {
        self.hooks.local_temperature()
    }

    /// Get current setpoint
    pub fn occupied_heating_setpoint(&self) -> i16 {
        self.hooks.occupied_heating_setpoint()
    }

    /// Set setpoint (called from Matter write)
    pub fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), Error> {
        self.hooks.set_occupied_heating_setpoint(value)
    }

    /// Get current system mode
    pub fn system_mode(&self) -> SystemModeEnum {
        self.hooks.system_mode()
    }

    /// Set system mode (called from Matter write)
    pub fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error> {
        // For heating-only, only allow Off or Heat
        match mode {
            SystemModeEnum::Off | SystemModeEnum::Heat => {
                self.hooks.set_system_mode(mode)
            }
            _ => Err(ErrorCode::ConstraintError.into()),
        }
    }

    /// Get thermostat running state
    pub fn thermostat_running_state(&self) -> RelayStateBitmap {
        self.hooks.thermostat_running_state()
    }
}

// Implement ClusterAsyncHandler trait
// Note: The exact trait methods depend on rs-matter version
// This is based on OnOffHandler implementation pattern

impl<H: ThermostatHooks> ThermostatHandler<H> {
    /// The cluster metadata for this handler
    pub const CLUSTER: Cluster<'static> = H::CLUSTER;

    /// Get current data version
    pub fn dataver(&self) -> u32 {
        self.dataver.get()
    }

    /// Mark data version as changed
    pub fn dataver_changed(&self) {
        self.dataver.changed();
    }

    /// Run the handler's background task
    pub async fn run(&self, ctx: impl HandlerContext) -> Result<(), Error> {
        let mut hooks_fut = pin!(self.hooks.run(|msg| self.notify_state_changed(msg)));

        loop {
            match select(&mut hooks_fut, self.state_change_signal.wait()).await {
                Either::First(_) => {
                    // Hooks future should never complete
                    panic!("ThermostatHooks::run returned unexpectedly");
                }
                Either::Second(message) => {
                    // Handle out-of-band state change
                    match message {
                        OutOfBandMessage::TemperatureUpdate => {
                            self.dataver_changed();
                            ctx.notify_attribute_changed(
                                self.endpoint_id,
                                Self::CLUSTER.id,
                                AttributeId::LocalTemperature as _,
                            );
                        }
                        OutOfBandMessage::SetpointChanged => {
                            self.dataver_changed();
                            ctx.notify_attribute_changed(
                                self.endpoint_id,
                                Self::CLUSTER.id,
                                AttributeId::OccupiedHeatingSetpoint as _,
                            );
                        }
                        OutOfBandMessage::HeatingStateChanged => {
                            self.dataver_changed();
                            ctx.notify_attribute_changed(
                                self.endpoint_id,
                                Self::CLUSTER.id,
                                AttributeId::ThermostatRunningState as _,
                            );
                        }
                        OutOfBandMessage::ModeChanged => {
                            self.dataver_changed();
                            ctx.notify_attribute_changed(
                                self.endpoint_id,
                                Self::CLUSTER.id,
                                AttributeId::SystemMode as _,
                            );
                        }
                    }
                }
            }
        }
    }
}
```

**Step 2: Implement ZoneThermostatState as ThermostatHooks**

Modify `emberheat-esp32/src/matter/state.rs` to implement the hooks trait:
```rust
// Add at the end of state.rs

use crate::matter::cluster::{
    ThermostatHooks, OutOfBandMessage,
    HEATING_ONLY_CLUSTER,
    SystemModeEnum, RelayStateBitmap, ControlSequenceOfOperationEnum,
};
use rs_matter::dm::Cluster;
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::Nullable;

impl ThermostatHooks for ZoneThermostatState {
    const CLUSTER: Cluster<'static> = HEATING_ONLY_CLUSTER;

    fn local_temperature(&self) -> Nullable<i16> {
        let temp = self.get_local_temperature();
        if temp == temperature::NULL_TEMPERATURE {
            Nullable::null()
        } else {
            Nullable::some(temp)
        }
    }

    fn occupied_heating_setpoint(&self) -> i16 {
        self.get_occupied_heating_setpoint()
    }

    fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), Error> {
        self.set_occupied_heating_setpoint(value)
            .map_err(|_| ErrorCode::ConstraintError.into())
    }

    fn min_heat_setpoint_limit(&self) -> i16 {
        defaults::MIN_HEAT_SETPOINT
    }

    fn max_heat_setpoint_limit(&self) -> i16 {
        defaults::MAX_HEAT_SETPOINT
    }

    fn system_mode(&self) -> SystemModeEnum {
        match self.get_system_mode() {
            SystemMode::Off => SystemModeEnum::Off,
            SystemMode::Heat => SystemModeEnum::Heat,
            _ => SystemModeEnum::Off, // Default to off for unsupported modes
        }
    }

    fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error> {
        let our_mode = match mode {
            SystemModeEnum::Off => SystemMode::Off,
            SystemModeEnum::Heat => SystemMode::Heat,
            _ => return Err(ErrorCode::ConstraintError.into()),
        };
        self.set_system_mode(our_mode);
        Ok(())
    }

    fn thermostat_running_state(&self) -> RelayStateBitmap {
        let mut bitmap = RelayStateBitmap::empty();
        if self.is_heating() {
            bitmap |= RelayStateBitmap::HEAT;
        }
        bitmap
    }
}
```

**Step 3: Update mod.rs**

```rust
pub mod handler;

pub use handler::ThermostatHandler;
```

**Step 4: Verify compilation**

Run: `cd emberheat-esp32 && WIFI_SSID="test" WIFI_PASSWORD="test" cargo check`
Expected: Compiles with handler implementation

**Step 5: Commit**

```bash
git add emberheat-esp32/src/matter/
git commit -m "feat(matter): implement ThermostatHandler with hooks pattern

- Create ThermostatHandler struct following OnOffHandler pattern
- Implement background task for out-of-band notifications
- Implement ThermostatHooks for ZoneThermostatState
- Bridge between Matter types and our internal types"
```

---

### Task 6: Create Multi-Zone Matter Device

**Files:**
- Create: `emberheat-esp32/src/matter/device.rs`
- Modify: `emberheat-esp32/src/matter/mod.rs`

**Step 1: Define 7-endpoint device structure**

Create `emberheat-esp32/src/matter/device.rs`:
```rust
//! Matter device configuration for 7-zone thermostat
//!
//! Defines the Matter node with 7 thermostat endpoints (one per zone).

use crate::matter::state::ZoneThermostatState;

/// Matter endpoint numbers (0 is root, 1-7 are zones)
pub const ROOT_ENDPOINT: u16 = 0;
pub const ZONE1_ENDPOINT: u16 = 1;
pub const ZONE2_ENDPOINT: u16 = 2;
pub const ZONE3_ENDPOINT: u16 = 3;
pub const ZONE4_ENDPOINT: u16 = 4;
pub const ZONE5_ENDPOINT: u16 = 5;
pub const ZONE6_ENDPOINT: u16 = 6;
pub const ZONE7_ENDPOINT: u16 = 7;

/// Number of thermostat zones
pub const NUM_ZONES: usize = 7;

/// Static state for all 7 zones
pub static ZONE_STATES: [ZoneThermostatState; NUM_ZONES] = [
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
];

/// Get zone state by index (0-6)
pub fn get_zone_state(index: usize) -> Option<&'static ZoneThermostatState> {
    ZONE_STATES.get(index)
}

/// Get zone state by Matter endpoint (1-7)
pub fn get_zone_state_by_endpoint(endpoint: u16) -> Option<&'static ZoneThermostatState> {
    if endpoint >= ZONE1_ENDPOINT && endpoint <= ZONE7_ENDPOINT {
        get_zone_state((endpoint - 1) as usize)
    } else {
        None
    }
}

/// Device information for Matter commissioning
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_name: &'static str,
    pub serial_number: &'static str,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            vendor_id: 0xFFF1,      // Test vendor ID
            product_id: 0x8001,     // Test product ID  
            device_name: "EmberHeat 7-Zone",
            serial_number: "EMBERHEAT-001",
        }
    }
}
```

**Step 2: Update mod.rs**

Add to `emberheat-esp32/src/matter/mod.rs`:
```rust
pub mod device;
pub use device::{ZONE_STATES, get_zone_state, get_zone_state_by_endpoint, DeviceInfo, NUM_ZONES};
```

**Step 3: Commit**

```bash
git add emberheat-esp32/src/matter/device.rs emberheat-esp32/src/matter/mod.rs
git commit -m "feat(matter): add 7-zone device configuration"
```

---

### Task 7: Bridge Matter State to EmberHeat-Core ✅ COMPLETED

**Status:** Done - bridge.rs with bidirectional sync functions.

**What was done:**
- Created `bridge.rs` with sync functions between Matter and control loop
- `update_matter_from_control_loop()` - sync temp/heating to Matter
- `get_matter_setpoint()` - read setpoint from Matter
- `is_zone_enabled_in_matter()` - check zone mode
- `set_matter_setpoint()`/`set_matter_mode()` - sync local changes
- `sync_local_state_to_matter()` - bulk sync for NVS restore
- Unit tests for all bridge functions

---

### Task 7.5: Extract emberheat-matter Crate

**Status:** Pending

**Goal:** Extract the Matter module into a separate `emberheat-matter` crate to enable:
1. Host-based unit testing (can't run emberheat-esp32 tests on host due to ESP32 dependencies)
2. Optional `std` feature for future non-embedded use cases
3. Cleaner dependency graph and independent versioning

**Files:**
- Create: `emberheat-matter/Cargo.toml`
- Create: `emberheat-matter/src/lib.rs`
- Move: `emberheat-esp32/src/matter/types.rs` → `emberheat-matter/src/types.rs`
- Move: `emberheat-esp32/src/matter/state.rs` → `emberheat-matter/src/state.rs`
- Move: `emberheat-esp32/src/matter/cluster.rs` → `emberheat-matter/src/cluster.rs`
- Move: `emberheat-esp32/src/matter/handler.rs` → `emberheat-matter/src/handler.rs`
- Move: `emberheat-esp32/src/matter/device.rs` → `emberheat-matter/src/device.rs`
- Keep: `emberheat-esp32/src/matter/bridge.rs` (integration layer, stays in emberheat-esp32)
- Modify: `emberheat-esp32/src/matter/mod.rs` (re-export from emberheat-matter)
- Modify: `emberheat-esp32/Cargo.toml` (add emberheat-matter dependency)
- Modify: `Cargo.toml` (add emberheat-matter to workspace members)

**Step 1: Create emberheat-matter crate**

Create `emberheat-matter/Cargo.toml`:
```toml
[package]
name = "emberheat-matter"
version = "0.1.0"
edition = "2021"
license = "MIT"
description = "Matter Thermostat Cluster implementation for heating-only thermostats"

[dependencies]
# Matter protocol
rs-matter = { git = "https://github.com/project-chip/rs-matter", default-features = false, features = ["rustcrypto"] }

# Embassy async (for Signal, select, etc.)
embassy-futures = "0.1"
embassy-sync = "0.7"

# Utilities
log = { workspace = true }
heapless = "0.9"

[features]
default = []
std = []

[dev-dependencies]
tokio = { version = "1.48", features = ["rt", "macros", "sync", "time"] }
```

**Step 2: Create lib.rs**

Create `emberheat-matter/src/lib.rs`:
```rust
//! Matter Thermostat Cluster implementation
//!
//! Provides a heating-only Matter Thermostat Cluster (0x0201) implementation
//! designed for integration with rs-matter on embedded systems.
//!
//! # Features
//!
//! - `std` - Enable std library support (default: no_std)
//!
//! # Architecture
//!
//! - [`cluster`]: Cluster definition from Matter IDL and ThermostatHooks trait
//! - [`types`]: Type definitions (attribute IDs, enums, temperature conversions)
//! - [`state`]: Thread-safe state container for zone thermostat data
//! - [`handler`]: Matter cluster handler implementation
//! - [`device`]: Multi-zone device configuration

#![cfg_attr(not(feature = "std"), no_std)]

pub mod cluster;
pub mod device;
pub mod handler;
pub mod state;
pub mod types;

// Re-exports for convenience
pub use cluster::{
    AttributeId, ControlSequenceOfOperationEnum, Feature, OutOfBandMessage, 
    RelayStateBitmap, SystemModeEnum, ThermostatHooks, HEATING_ONLY_CLUSTER,
};
pub use device::{
    endpoint_to_zone_index, get_zone_state, get_zone_state_by_endpoint, 
    zone_index_to_endpoint, DeviceInfo, NUM_ZONES, ROOT_ENDPOINT, 
    ZONE1_ENDPOINT, ZONE2_ENDPOINT, ZONE3_ENDPOINT, ZONE4_ENDPOINT, 
    ZONE5_ENDPOINT, ZONE6_ENDPOINT, ZONE7_ENDPOINT, ZONE_STATES,
};
pub use handler::ThermostatHandler;
pub use state::{SetpointError, ZoneThermostatState};
```

**Step 3: Move source files**

Move these files from `emberheat-esp32/src/matter/` to `emberheat-matter/src/`:
- `types.rs`
- `state.rs`
- `cluster.rs`
- `handler.rs`
- `device.rs`

Update imports in moved files:
- Change `crate::matter::` to `crate::`

**Step 4: Update emberheat-esp32**

Modify `emberheat-esp32/Cargo.toml`:
```toml
# Add under [dependencies]
emberheat-matter = { path = "../emberheat-matter" }
```

Modify `emberheat-esp32/src/matter/mod.rs`:
```rust
//! Matter protocol integration for emberheat-esp32
//!
//! Re-exports emberheat-matter and provides the bridge to emberheat-core.

pub mod bridge;

// Re-export everything from emberheat-matter
pub use emberheat_matter::*;

// Re-export bridge functions
pub use bridge::{
    get_matter_setpoint, is_zone_enabled_in_matter, set_matter_mode, 
    set_matter_setpoint, sync_local_state_to_matter, update_matter_from_control_loop,
};
```

Update `emberheat-esp32/src/matter/bridge.rs` imports:
```rust
// Change from:
use crate::matter::device::get_zone_state;
use crate::matter::types::SystemMode;

// To:
use emberheat_matter::device::get_zone_state;
use emberheat_matter::types::SystemMode;
```

**Step 5: Update workspace**

Modify root `Cargo.toml`:
```toml
[workspace]
members = [
    "emberheat-core",
    "emberheat-matter",  # Add this
    "emberheat-esp32",
    "emberheat-linux",
]
```

**Step 6: Verify compilation**

```bash
# Test emberheat-matter builds and tests pass
cargo test -p emberheat-matter

# Test emberheat-esp32 still compiles
WIFI_SSID=test WIFI_PASSWORD=test cargo check -p emberheat-esp32 --target riscv32imac-unknown-none-elf
```

**Step 7: Commit**

```bash
git add emberheat-matter/ Cargo.toml emberheat-esp32/
git commit -m "refactor: extract emberheat-matter crate for testability

- Create emberheat-matter crate with no_std default, optional std feature
- Move types, state, cluster, handler, device modules from emberheat-esp32
- Keep bridge.rs in emberheat-esp32 as integration layer
- Re-export emberheat-matter from emberheat-esp32 for compatibility
- Tests now run on host with cargo test -p emberheat-matter"
```

---

### Task 8: Verify Unit Tests Pass ✅ BLOCKED (depends on Task 7.5)

**Status:** Blocked - waiting for emberheat-matter extraction

**Depends on:** Task 7.5

After Task 7.5 is complete:

```bash
# Run all emberheat-matter tests on host
cargo test -p emberheat-matter

# Expected: All tests pass including:
# - types.rs tests (temperature conversion, enums)
# - state.rs tests (ZoneThermostatState operations)
# - device.rs tests (endpoint mapping)
# - bridge.rs tests remain in emberheat-esp32 (they use both crates)
```

---

## Validation Checklist

Before proceeding to Phase 3 (Matter stack integration):

- [ ] All unit tests pass
- [ ] Types match Matter Thermostat Cluster specification  
- [ ] State container is thread-safe (atomic operations)
- [ ] ThermostatHooks trait properly implemented for ZoneThermostatState
- [ ] ThermostatHandler compiles with rs-matter types
- [ ] Bridge correctly converts between Matter and emberheat-core formats
- [ ] No compilation errors with rs-matter dependencies
- [ ] Code documented with rustdoc comments

---

## Phase 3 Preview: Matter Stack Integration

Phase 3 will focus on:
1. Integrating with rs-matter-embassy (or manual embassy integration)
2. Setting up BLE commissioning
3. WiFi connectivity for Matter
4. NVS storage for Matter fabrics
5. Testing with Apple Home, Google Home, Alexa

This requires resolving the embassy-time version conflict, either by:
- Waiting for rs-matter-stack to update to embassy-time 0.5
- Implementing manual stack assembly without rs-matter-stack
- Using a compatibility shim

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| rs-matter API changes | Handler code breaks | Pin dependency version, check changelog |
| Embassy version conflicts | Build fails | Monitor rs-matter updates, use compatible versions |
| Memory pressure with Matter stack | OOM on ESP32 | Profile early, reduce HEAP_SIZE if needed |
| BLE commissioning complexity | Pairing issues | Test with multiple controllers early |
| IDL import issues | Cluster types wrong | Verify against Matter spec, test with chip-tool |

---

## References

- [Matter Specification - Thermostat Cluster](https://github.com/project-chip/connectedhomeip/blob/master/data_model/1.5/clusters/Thermostat.xml)
- [rs-matter GitHub](https://github.com/project-chip/rs-matter)
- [rs-matter OnOffHandler](https://github.com/project-chip/rs-matter/blob/main/rs-matter/src/dm/clusters/on_off.rs) - Reference implementation
- [rs-matter IDL Files](https://github.com/project-chip/rs-matter/tree/main/rs-matter-macros/src/idl/parser) - Cluster definitions
- [rs-matter-embassy GitHub](https://github.com/sysgrok/rs-matter-embassy)
