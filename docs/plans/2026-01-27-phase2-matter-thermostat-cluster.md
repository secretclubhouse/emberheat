# Phase 2: Matter Thermostat Cluster Implementation

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a heating-only thermostat cluster implementation for rs-matter that can be integrated with the ESP32 multi-zone thermostat from Phase 1.

**Architecture:** Implement Matter Thermostat Cluster (0x0201) as a standalone Rust module with heating-only attributes, designed to integrate with rs-matter-embassy on ESP32-C6. The cluster will bridge Matter commands to thermos-core's hysteresis control.

**Tech Stack:** rs-matter, rs-matter-embassy, esp-hal 1.0, Embassy async runtime

**Prerequisites:** 
- Phase 1 complete (multi-zone thermostat with real DS18B20 sensors working)
- ESP32-C6 DevKitC-1 with 7 zones tested and stable

---

## Background Research Summary

### rs-matter Ecosystem

The Matter implementation in Rust consists of several crates:
- **rs-matter** - Core Matter protocol library ([GitHub](https://github.com/project-chip/rs-matter))
- **rs-matter-embassy** - Embassy async integration for MCUs ([GitHub](https://github.com/sysgrok/rs-matter-embassy))
- **rs-matter-stack** - Stack assembly utilities

### Thermostat Cluster Status

**Important:** rs-matter does NOT have a built-in thermostat cluster implementation. The available clusters are:
- OnOffHandler (lights)
- LevelControlHandler (dimmers)
- DescHandler (descriptors)

We must implement the Thermostat cluster (0x0201) ourselves using rs-matter's `ClusterAsyncHandler` trait.

### Memory Requirements

From rs-matter-embassy documentation:
- HEAP_SIZE: ~140KB for ESP32
- Stack: ~35-50KB for Matter stack
- BUMP_SIZE: ~16.5KB for futures allocation

With Phase 1's thermostat using ~30KB, total should fit within ESP32-C6's 512KB SRAM.

---

## Matter Thermostat Cluster Specification

### Cluster ID: 0x0201

**Attributes (heating-only subset):**

| Attribute | ID | Type | Access | Default | Description |
|-----------|----|------|--------|---------|-------------|
| LocalTemperature | 0x0000 | int16 | Read | null | Current temp in 0.01°C units |
| OccupiedHeatingSetpoint | 0x0012 | int16 | R/W | 2000 (20°C) | Target temp in 0.01°C |
| MinHeatSetpointLimit | 0x0015 | int16 | Read | 700 (7°C) | Minimum allowed setpoint |
| MaxHeatSetpointLimit | 0x0016 | int16 | Read | 3000 (30°C) | Maximum allowed setpoint |
| ControlSequenceOfOperation | 0x001B | enum8 | Read | 0x02 | HeatingOnly |
| SystemMode | 0x001C | enum8 | R/W | 0x01 | Off=0x00, Heat=0x04 |
| ThermostatRunningState | 0x0029 | bitmap16 | Read | 0 | Bit0=Heat, Bit1=Cool, etc. |

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

---

## Implementation Tasks

### Task 1: Set Up rs-matter Development Environment

**Files:**
- Create: `thermos-esp32/Cargo.toml` (update dependencies)
- Create: `thermos-esp32/src/matter/mod.rs`

**Step 1: Add rs-matter dependencies to Cargo.toml**

Add to `[dependencies]`:
```toml
# Matter protocol support
rs-matter = { git = "https://github.com/project-chip/rs-matter", features = ["embassy"] }
rs-matter-embassy = { git = "https://github.com/sysgrok/rs-matter-embassy", features = ["esp32"] }
```

**Step 2: Create matter module structure**

Create `thermos-esp32/src/matter/mod.rs`:
```rust
//! Matter protocol integration for thermos-esp32
//!
//! Implements Matter Thermostat Cluster (0x0201) for HomeKit/Google/Alexa control.

pub mod cluster;
pub mod handler;
pub mod types;

pub use cluster::ThermostatCluster;
pub use handler::ThermostatHandler;
```

**Step 3: Verify compilation**

Run: `cd thermos-esp32 && WIFI_SSID="test" WIFI_PASSWORD="test" cargo check`
Expected: Compiles (may have warnings about unused modules)

**Step 4: Commit**

```bash
git add thermos-esp32/Cargo.toml thermos-esp32/src/matter/
git commit -m "feat(matter): add rs-matter dependencies and module structure"
```

---

### Task 2: Implement Thermostat Type Definitions

**Files:**
- Create: `thermos-esp32/src/matter/types.rs`

**Step 1: Define thermostat attribute types**

Create `thermos-esp32/src/matter/types.rs`:
```rust
//! Matter Thermostat Cluster type definitions
//!
//! Based on Matter Specification 1.2, Cluster 0x0201

/// Thermostat Cluster ID
pub const CLUSTER_ID: u32 = 0x0201;

/// Thermostat Device Type ID
pub const DEVICE_TYPE_ID: u32 = 0x0301;

/// Attribute IDs for Thermostat Cluster
pub mod attribute {
    pub const LOCAL_TEMPERATURE: u16 = 0x0000;
    pub const OCCUPIED_HEATING_SETPOINT: u16 = 0x0012;
    pub const MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0015;
    pub const MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0016;
    pub const CONTROL_SEQUENCE_OF_OPERATION: u16 = 0x001B;
    pub const SYSTEM_MODE: u16 = 0x001C;
    pub const THERMOSTAT_RUNNING_STATE: u16 = 0x0029;
}

/// System Mode values (enum8)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SystemMode {
    Off = 0x00,
    Auto = 0x01,  // Not used for heating-only
    Cool = 0x03,  // Not used
    Heat = 0x04,
    EmergencyHeat = 0x05,  // Not used
}

impl SystemMode {
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::Auto),
            0x03 => Some(Self::Cool),
            0x04 => Some(Self::Heat),
            0x05 => Some(Self::EmergencyHeat),
            _ => None,
        }
    }
}

impl From<SystemMode> for u8 {
    fn from(mode: SystemMode) -> u8 {
        mode as u8
    }
}

/// Control Sequence of Operation values (enum8)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ControlSequence {
    CoolingOnly = 0x00,
    CoolingWithReheat = 0x01,
    HeatingOnly = 0x02,
    HeatingWithReheat = 0x03,
    CoolingAndHeating = 0x04,
    CoolingAndHeatingWithReheat = 0x05,
}

/// Thermostat Running State bitmap (bitmap16)
#[derive(Debug, Clone, Copy, Default)]
pub struct RunningState(pub u16);

impl RunningState {
    pub const HEAT: u16 = 1 << 0;
    pub const COOL: u16 = 1 << 1;
    pub const FAN: u16 = 1 << 2;
    pub const HEAT_STAGE_2: u16 = 1 << 3;
    pub const COOL_STAGE_2: u16 = 1 << 4;
    pub const FAN_STAGE_2: u16 = 1 << 5;
    pub const FAN_STAGE_3: u16 = 1 << 6;

    pub fn new() -> Self {
        Self(0)
    }

    pub fn set_heating(&mut self, active: bool) {
        if active {
            self.0 |= Self::HEAT;
        } else {
            self.0 &= !Self::HEAT;
        }
    }

    pub fn is_heating(&self) -> bool {
        self.0 & Self::HEAT != 0
    }
}

/// Temperature conversion utilities
pub mod temperature {
    /// Convert Celsius to Matter centi-degrees (0.01°C units)
    pub fn celsius_to_matter(temp_c: f32) -> i16 {
        (temp_c * 100.0) as i16
    }

    /// Convert Matter centi-degrees to Celsius
    pub fn matter_to_celsius(centi_c: i16) -> f32 {
        (centi_c as f32) / 100.0
    }

    /// Null temperature value (sensor unavailable)
    pub const NULL_TEMPERATURE: i16 = 0x8000;
}

/// Default configuration values
pub mod defaults {
    use super::temperature::celsius_to_matter;
    
    /// Default setpoint: 20°C
    pub const OCCUPIED_HEATING_SETPOINT: i16 = 2000;
    
    /// Minimum setpoint: 7°C (frost protection)
    pub const MIN_HEAT_SETPOINT: i16 = 700;
    
    /// Maximum setpoint: 30°C
    pub const MAX_HEAT_SETPOINT: i16 = 3000;
}
```

**Step 2: Run tests**

Run: `cd thermos-esp32 && cargo test --lib -- types`
Expected: Type conversion tests pass

**Step 3: Commit**

```bash
git add thermos-esp32/src/matter/types.rs
git commit -m "feat(matter): add thermostat cluster type definitions"
```

---

### Task 3: Implement Thermostat State Container

**Files:**
- Create: `thermos-esp32/src/matter/state.rs`
- Modify: `thermos-esp32/src/matter/mod.rs`

**Step 1: Create thread-safe state container**

Create `thermos-esp32/src/matter/state.rs`:
```rust
//! Thermostat state management
//!
//! Provides thread-safe access to thermostat state for bridging
//! between Matter cluster and thermos-core control loops.

use core::sync::atomic::{AtomicI16, AtomicU8, AtomicU16, Ordering};
use crate::matter::types::{SystemMode, RunningState, temperature, defaults};

/// Thread-safe thermostat state for a single zone
pub struct ZoneThermostatState {
    /// Current temperature in Matter centi-degrees (0.01°C)
    local_temperature: AtomicI16,
    
    /// Target setpoint in Matter centi-degrees
    occupied_heating_setpoint: AtomicI16,
    
    /// Current system mode (Off/Heat)
    system_mode: AtomicU8,
    
    /// Running state bitmap
    running_state: AtomicU16,
}

impl ZoneThermostatState {
    pub const fn new() -> Self {
        Self {
            local_temperature: AtomicI16::new(temperature::NULL_TEMPERATURE),
            occupied_heating_setpoint: AtomicI16::new(defaults::OCCUPIED_HEATING_SETPOINT),
            system_mode: AtomicU8::new(SystemMode::Heat as u8),
            running_state: AtomicU16::new(0),
        }
    }

    // Temperature getters/setters
    pub fn get_local_temperature(&self) -> i16 {
        self.local_temperature.load(Ordering::Relaxed)
    }

    pub fn set_local_temperature(&self, temp_centi_c: i16) {
        self.local_temperature.store(temp_centi_c, Ordering::Relaxed);
    }

    pub fn set_local_temperature_celsius(&self, temp_c: f32) {
        self.set_local_temperature(temperature::celsius_to_matter(temp_c));
    }

    // Setpoint getters/setters  
    pub fn get_occupied_heating_setpoint(&self) -> i16 {
        self.occupied_heating_setpoint.load(Ordering::Relaxed)
    }

    pub fn set_occupied_heating_setpoint(&self, setpoint_centi_c: i16) -> Result<(), SetpointError> {
        if setpoint_centi_c < defaults::MIN_HEAT_SETPOINT {
            return Err(SetpointError::BelowMinimum);
        }
        if setpoint_centi_c > defaults::MAX_HEAT_SETPOINT {
            return Err(SetpointError::AboveMaximum);
        }
        self.occupied_heating_setpoint.store(setpoint_centi_c, Ordering::Relaxed);
        Ok(())
    }

    pub fn get_setpoint_celsius(&self) -> f32 {
        temperature::matter_to_celsius(self.get_occupied_heating_setpoint())
    }

    // System mode getters/setters
    pub fn get_system_mode(&self) -> SystemMode {
        SystemMode::from_u8(self.system_mode.load(Ordering::Relaxed))
            .unwrap_or(SystemMode::Off)
    }

    pub fn set_system_mode(&self, mode: SystemMode) {
        self.system_mode.store(mode as u8, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.get_system_mode() == SystemMode::Heat
    }

    // Running state getters/setters
    pub fn get_running_state(&self) -> RunningState {
        RunningState(self.running_state.load(Ordering::Relaxed))
    }

    pub fn set_heating_active(&self, active: bool) {
        let mut state = self.get_running_state();
        state.set_heating(active);
        self.running_state.store(state.0, Ordering::Relaxed);
    }

    pub fn is_heating(&self) -> bool {
        self.get_running_state().is_heating()
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SetpointError {
    BelowMinimum,
    AboveMaximum,
}
```

**Step 2: Update mod.rs**

Add to `thermos-esp32/src/matter/mod.rs`:
```rust
pub mod state;
pub use state::ZoneThermostatState;
```

**Step 3: Commit**

```bash
git add thermos-esp32/src/matter/state.rs thermos-esp32/src/matter/mod.rs
git commit -m "feat(matter): add thread-safe thermostat state container"
```

---

### Task 4: Implement Cluster Handler Trait

**Files:**
- Create: `thermos-esp32/src/matter/handler.rs`
- Modify: `thermos-esp32/src/matter/mod.rs`

**Step 1: Implement ClusterAsyncHandler for thermostat**

This task requires studying the rs-matter API to understand:
1. How `ClusterAsyncHandler` trait works
2. How to read/write attributes
3. How to respond to Matter commands

Create `thermos-esp32/src/matter/handler.rs`:
```rust
//! Matter Thermostat Cluster handler implementation
//!
//! Implements rs-matter's ClusterAsyncHandler trait for the Thermostat cluster.

use crate::matter::types::{self, attribute, defaults, temperature, SystemMode, ControlSequence};
use crate::matter::state::ZoneThermostatState;

// NOTE: The exact rs-matter API needs to be verified by examining the crate.
// This is a template based on the OnOffHandler pattern shown in rs-matter-embassy.

/// Handler for Matter Thermostat Cluster (0x0201)
pub struct ThermostatHandler<'a> {
    /// Zone index (0-6 for 7 zones)
    zone_index: usize,
    
    /// Reference to zone state (shared with thermos-core)
    state: &'a ZoneThermostatState,
    
    /// Callback when setpoint changes (to trigger NVS persist)
    on_setpoint_change: Option<fn(zone: usize, setpoint_c: f32)>,
}

impl<'a> ThermostatHandler<'a> {
    pub fn new(zone_index: usize, state: &'a ZoneThermostatState) -> Self {
        Self {
            zone_index,
            state,
            on_setpoint_change: None,
        }
    }

    pub fn with_setpoint_callback(
        mut self, 
        callback: fn(zone: usize, setpoint_c: f32)
    ) -> Self {
        self.on_setpoint_change = Some(callback);
        self
    }

    /// Handle attribute read request
    pub fn read_attribute(&self, attr_id: u16) -> Option<AttributeValue> {
        match attr_id {
            attribute::LOCAL_TEMPERATURE => {
                Some(AttributeValue::Int16(self.state.get_local_temperature()))
            }
            attribute::OCCUPIED_HEATING_SETPOINT => {
                Some(AttributeValue::Int16(self.state.get_occupied_heating_setpoint()))
            }
            attribute::MIN_HEAT_SETPOINT_LIMIT => {
                Some(AttributeValue::Int16(defaults::MIN_HEAT_SETPOINT))
            }
            attribute::MAX_HEAT_SETPOINT_LIMIT => {
                Some(AttributeValue::Int16(defaults::MAX_HEAT_SETPOINT))
            }
            attribute::CONTROL_SEQUENCE_OF_OPERATION => {
                Some(AttributeValue::Enum8(ControlSequence::HeatingOnly as u8))
            }
            attribute::SYSTEM_MODE => {
                Some(AttributeValue::Enum8(self.state.get_system_mode() as u8))
            }
            attribute::THERMOSTAT_RUNNING_STATE => {
                Some(AttributeValue::Bitmap16(self.state.get_running_state().0))
            }
            _ => None, // Unknown attribute
        }
    }

    /// Handle attribute write request
    pub fn write_attribute(&self, attr_id: u16, value: &AttributeValue) -> WriteResult {
        match attr_id {
            attribute::OCCUPIED_HEATING_SETPOINT => {
                if let AttributeValue::Int16(setpoint) = value {
                    match self.state.set_occupied_heating_setpoint(*setpoint) {
                        Ok(()) => {
                            // Notify callback for persistence
                            if let Some(callback) = self.on_setpoint_change {
                                let celsius = temperature::matter_to_celsius(*setpoint);
                                callback(self.zone_index, celsius);
                            }
                            WriteResult::Ok
                        }
                        Err(_) => WriteResult::ConstraintError,
                    }
                } else {
                    WriteResult::InvalidDataType
                }
            }
            attribute::SYSTEM_MODE => {
                if let AttributeValue::Enum8(mode) = value {
                    if let Some(system_mode) = SystemMode::from_u8(*mode) {
                        // Only allow Off or Heat for heating-only thermostat
                        match system_mode {
                            SystemMode::Off | SystemMode::Heat => {
                                self.state.set_system_mode(system_mode);
                                WriteResult::Ok
                            }
                            _ => WriteResult::ConstraintError,
                        }
                    } else {
                        WriteResult::ConstraintError
                    }
                } else {
                    WriteResult::InvalidDataType
                }
            }
            _ => WriteResult::UnsupportedAttribute,
        }
    }
}

// Placeholder types - actual types come from rs-matter
#[derive(Debug)]
pub enum AttributeValue {
    Int16(i16),
    Enum8(u8),
    Bitmap16(u16),
}

#[derive(Debug)]
pub enum WriteResult {
    Ok,
    ConstraintError,
    InvalidDataType,
    UnsupportedAttribute,
}
```

**Step 2: Update mod.rs**

Add to `thermos-esp32/src/matter/mod.rs`:
```rust
pub mod handler;
pub use handler::ThermostatHandler;
```

**Step 3: Commit**

```bash
git add thermos-esp32/src/matter/handler.rs thermos-esp32/src/matter/mod.rs  
git commit -m "feat(matter): implement thermostat cluster handler (template)"
```

---

### Task 5: Study rs-matter API and Adapt Handler

**Goal:** Examine actual rs-matter crate API and adapt the handler template.

**Files:**
- Modify: `thermos-esp32/src/matter/handler.rs`

**Step 1: Clone rs-matter and examine API**

```bash
cd /tmp
git clone https://github.com/project-chip/rs-matter.git
# Examine:
# - rs-matter/src/data_model/cluster_basic_information.rs (example cluster)
# - rs-matter/src/data_model/objects/mod.rs (ClusterAsyncHandler trait)
# - rs-matter/src/data_model/sdm/on_off.rs (OnOff cluster implementation)
```

**Step 2: Adapt handler to actual rs-matter API**

Update `handler.rs` based on actual trait requirements from rs-matter.

The handler likely needs to implement:
```rust
impl ClusterAsyncHandler for ThermostatHandler {
    async fn read(&self, attr_id: AttrId, writer: &mut AttrDataWriter<'_>) -> Result<(), Error>;
    async fn write(&self, attr_id: AttrId, data: &AttrData<'_>) -> Result<(), Error>;
    // Possibly invoke() for commands
}
```

**Step 3: Verify compilation**

Run: `cd thermos-esp32 && cargo check`
Expected: Compiles with actual rs-matter types

**Step 4: Commit**

```bash
git add thermos-esp32/src/matter/handler.rs
git commit -m "feat(matter): adapt handler to rs-matter ClusterAsyncHandler API"
```

---

### Task 6: Create Multi-Zone Matter Device

**Files:**
- Create: `thermos-esp32/src/matter/device.rs`
- Modify: `thermos-esp32/src/matter/mod.rs`

**Step 1: Define 7-endpoint device structure**

Create `thermos-esp32/src/matter/device.rs`:
```rust
//! Matter device configuration for 7-zone thermostat
//!
//! Defines the Matter node with 7 thermostat endpoints (one per zone).

use crate::matter::types::{CLUSTER_ID, DEVICE_TYPE_ID};
use crate::matter::state::ZoneThermostatState;
use crate::matter::handler::ThermostatHandler;
use crate::config::ZONE_CONFIGS;

/// Matter endpoint numbers (0 is root, 1-7 are zones)
pub const ROOT_ENDPOINT: u16 = 0;
pub const ZONE1_ENDPOINT: u16 = 1;
pub const ZONE2_ENDPOINT: u16 = 2;
pub const ZONE3_ENDPOINT: u16 = 3;
pub const ZONE4_ENDPOINT: u16 = 4;
pub const ZONE5_ENDPOINT: u16 = 5;
pub const ZONE6_ENDPOINT: u16 = 6;
pub const ZONE7_ENDPOINT: u16 = 7;

/// Static state for all 7 zones
pub static ZONE_STATES: [ZoneThermostatState; 7] = [
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

// NOTE: Actual device/node configuration depends on rs-matter-embassy API
// This is a template - adapt based on rs-matter-embassy examples

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
            device_name: "Thermos 7-Zone",
            serial_number: "THERMOS-001",
        }
    }
}
```

**Step 2: Update mod.rs**

Add to `thermos-esp32/src/matter/mod.rs`:
```rust
pub mod device;
pub use device::{ZONE_STATES, get_zone_state, get_zone_state_by_endpoint, DeviceInfo};
```

**Step 3: Commit**

```bash
git add thermos-esp32/src/matter/device.rs thermos-esp32/src/matter/mod.rs
git commit -m "feat(matter): add 7-zone device configuration"
```

---

### Task 7: Bridge Matter State to Thermos-Core

**Files:**
- Create: `thermos-esp32/src/matter/bridge.rs`
- Modify: `thermos-esp32/src/matter/mod.rs`
- Modify: `thermos-esp32/src/controller.rs`

**Step 1: Create bidirectional bridge**

Create `thermos-esp32/src/matter/bridge.rs`:
```rust
//! Bridge between Matter cluster state and thermos-core control loop
//!
//! Syncs temperature readings, setpoints, and heating state between:
//! - Matter cluster (for HomeKit/Google/Alexa)
//! - thermos-core hysteresis control (actual heating logic)

use crate::matter::device::{get_zone_state, ZONE_STATES};
use crate::matter::types::temperature;
use crate::zone::ZoneState;
use thermos_core::ThermostatState;

/// Update Matter state from thermos-core (called in control loop)
pub fn update_matter_from_thermos(zone_index: usize, thermos_state: &ThermostatState) {
    if let Some(matter_state) = get_zone_state(zone_index) {
        // Update current temperature
        matter_state.set_local_temperature_celsius(thermos_state.current_temp);
        
        // Update heating state
        matter_state.set_heating_active(thermos_state.heat_on);
    }
}

/// Get setpoint from Matter for thermos-core (called in control loop)
pub fn get_matter_setpoint(zone_index: usize) -> Option<f32> {
    get_zone_state(zone_index).map(|state| state.get_setpoint_celsius())
}

/// Check if zone is enabled in Matter
pub fn is_zone_enabled_in_matter(zone_index: usize) -> bool {
    get_zone_state(zone_index)
        .map(|state| state.is_enabled())
        .unwrap_or(false)
}

/// Callback for when Matter setpoint changes (persist to NVS)
pub fn on_matter_setpoint_change(zone_index: usize, setpoint_c: f32) {
    log::info!(
        "[Zone{}] Matter setpoint changed to {:.1}°C",
        zone_index + 1,
        setpoint_c
    );
    // NVS persistence will be triggered by control loop on next iteration
}
```

**Step 2: Update controller.rs to use bridge**

Modify the control loop in `controller.rs` to:
1. Read setpoint from Matter state instead of (or in addition to) local config
2. Call `update_matter_from_thermos()` after each control cycle

**Step 3: Update mod.rs**

Add to `thermos-esp32/src/matter/mod.rs`:
```rust
pub mod bridge;
pub use bridge::{update_matter_from_thermos, get_matter_setpoint, is_zone_enabled_in_matter};
```

**Step 4: Commit**

```bash
git add thermos-esp32/src/matter/bridge.rs thermos-esp32/src/matter/mod.rs thermos-esp32/src/controller.rs
git commit -m "feat(matter): add bridge between Matter state and thermos-core"
```

---

### Task 8: Add Unit Tests

**Files:**
- Create: `thermos-esp32/src/matter/tests.rs`
- Modify: `thermos-esp32/src/matter/mod.rs`

**Step 1: Write unit tests for types and state**

Create `thermos-esp32/src/matter/tests.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::types::{temperature, SystemMode, RunningState, defaults};
    use crate::matter::state::ZoneThermostatState;

    #[test]
    fn test_temperature_conversion() {
        // Celsius to Matter
        assert_eq!(temperature::celsius_to_matter(21.5), 2150);
        assert_eq!(temperature::celsius_to_matter(0.0), 0);
        assert_eq!(temperature::celsius_to_matter(-10.0), -1000);
        
        // Matter to Celsius
        assert_eq!(temperature::matter_to_celsius(2150), 21.5);
        assert_eq!(temperature::matter_to_celsius(0), 0.0);
        assert_eq!(temperature::matter_to_celsius(-1000), -10.0);
    }

    #[test]
    fn test_system_mode() {
        assert_eq!(SystemMode::from_u8(0x00), Some(SystemMode::Off));
        assert_eq!(SystemMode::from_u8(0x04), Some(SystemMode::Heat));
        assert_eq!(SystemMode::from_u8(0xFF), None);
        
        assert_eq!(u8::from(SystemMode::Off), 0x00);
        assert_eq!(u8::from(SystemMode::Heat), 0x04);
    }

    #[test]
    fn test_running_state() {
        let mut state = RunningState::new();
        assert!(!state.is_heating());
        
        state.set_heating(true);
        assert!(state.is_heating());
        assert_eq!(state.0, RunningState::HEAT);
        
        state.set_heating(false);
        assert!(!state.is_heating());
        assert_eq!(state.0, 0);
    }

    #[test]
    fn test_zone_state_temperature() {
        let state = ZoneThermostatState::new();
        
        // Initial value is null
        assert_eq!(state.get_local_temperature(), temperature::NULL_TEMPERATURE);
        
        // Set via Celsius
        state.set_local_temperature_celsius(21.5);
        assert_eq!(state.get_local_temperature(), 2150);
    }

    #[test]
    fn test_zone_state_setpoint() {
        let state = ZoneThermostatState::new();
        
        // Default setpoint
        assert_eq!(state.get_occupied_heating_setpoint(), defaults::OCCUPIED_HEATING_SETPOINT);
        
        // Valid setpoint
        assert!(state.set_occupied_heating_setpoint(2200).is_ok());
        assert_eq!(state.get_setpoint_celsius(), 22.0);
        
        // Below minimum
        assert!(state.set_occupied_heating_setpoint(500).is_err());
        
        // Above maximum
        assert!(state.set_occupied_heating_setpoint(3500).is_err());
    }

    #[test]
    fn test_zone_state_mode() {
        let state = ZoneThermostatState::new();
        
        // Default is Heat
        assert_eq!(state.get_system_mode(), SystemMode::Heat);
        assert!(state.is_enabled());
        
        // Turn off
        state.set_system_mode(SystemMode::Off);
        assert_eq!(state.get_system_mode(), SystemMode::Off);
        assert!(!state.is_enabled());
    }
}
```

**Step 2: Update mod.rs**

Add to `thermos-esp32/src/matter/mod.rs`:
```rust
#[cfg(test)]
mod tests;
```

**Step 3: Run tests**

Run: `cd thermos-esp32 && cargo test --lib`
Expected: All tests pass

**Step 4: Commit**

```bash
git add thermos-esp32/src/matter/tests.rs thermos-esp32/src/matter/mod.rs
git commit -m "test(matter): add unit tests for thermostat types and state"
```

---

## Validation Checklist

Before proceeding to Phase 3 (rs-matter-embassy integration):

- [ ] All unit tests pass
- [ ] Types match Matter Thermostat Cluster specification
- [ ] State container is thread-safe (atomic operations)
- [ ] Handler implements all required attributes
- [ ] Bridge correctly converts between Matter and thermos-core formats
- [ ] No compilation errors with rs-matter dependencies
- [ ] Code documented with rustdoc comments

---

## Risk Mitigation

| Risk | Impact | Mitigation |
|------|--------|------------|
| rs-matter API changes | Handler code breaks | Pin dependency version, check changelog |
| No thermostat cluster in rs-matter | Must implement from scratch | Use OnOff cluster as template, follow spec |
| Memory pressure with Matter stack | OOM on ESP32 | Profile early, reduce HEAP_SIZE if needed |
| BLE commissioning complexity | Pairing issues | Test with multiple controllers early |

---

## References

- [Matter Specification - Thermostat Cluster](https://github.com/project-chip/connectedhomeip/blob/master/data_model/1.5/clusters/Thermostat.xml)
- [rs-matter GitHub](https://github.com/project-chip/rs-matter)
- [rs-matter-embassy GitHub](https://github.com/sysgrok/rs-matter-embassy)
- [Implementing a Matter Thermostat on ESP32](https://tomasmcguinness.com/2025/09/25/implementing-a-matter-thermostat-on-esp32/)
- [Building a heating monitor using Matter and ESP32](https://tomasmcguinness.com/2025/11/27/building-a-new-heating-monitor-using-matter-and-esp32/)
