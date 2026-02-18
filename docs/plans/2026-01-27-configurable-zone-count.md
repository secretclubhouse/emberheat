# Configurable Zone Count Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make the number of thermostat zones configurable so users can run 1-7 zones without code changes.

**Architecture:** Move zone count configuration to `emberheat-esp32/src/config.rs` as the single source of truth. The `emberheat-matter` crate will accept the zone states as a parameter rather than owning a hardcoded static array. Use const generics and conditional compilation where needed.

**Tech Stack:** Rust const generics, conditional compilation with `cfg` attributes

---

## Current State

The zone count is hardcoded in multiple places:

1. **`emberheat-esp32/src/config.rs`** - `NUM_ZONES = 7`, `ZONE_CONFIGS` array
2. **`emberheat-matter/src/device.rs`** - Separate `NUM_ZONES = 7`, static `ZONE_STATES` array
3. **`emberheat-esp32/src/main.rs`** - 7 static cells, 7 task functions, hardcoded GPIO initialization

## Design Decisions

1. **Single source of truth**: `emberheat-esp32/src/config.rs` owns `NUM_ZONES`
2. **emberheat-matter becomes stateless**: Remove `ZONE_STATES` from emberheat-matter; emberheat-esp32 owns the static array and passes references
3. **Maximum 7 zones**: Keep compile-time max of 7 to avoid dynamic allocation
4. **Conditional task spawning**: Use `if NUM_ZONES >= N` guards in main.rs

---

## Implementation Tasks

### Task 1: Remove Static State from emberheat-matter

**Files:**
- Modify: `emberheat-matter/src/device.rs`
- Modify: `emberheat-matter/src/lib.rs`

**Step 1: Update device.rs to remove ZONE_STATES and accept external state**

Replace `emberheat-matter/src/device.rs` with:

```rust
//! Matter device configuration for multi-zone thermostat
//!
//! Provides endpoint constants and helper functions for zone/endpoint mapping.
//! The actual zone states are owned by the application (emberheat-esp32).

/// Matter endpoint number for root node (device info, descriptors)
pub const ROOT_ENDPOINT: u16 = 0;

/// First zone endpoint (zones are 1-indexed in Matter)
pub const FIRST_ZONE_ENDPOINT: u16 = 1;

/// Maximum supported zones
pub const MAX_ZONES: usize = 7;

/// Convert zone index to Matter endpoint number
///
/// # Arguments
///
/// * `index` - Zone index (0-based)
/// * `num_zones` - Total number of configured zones
///
/// # Returns
///
/// Matter endpoint number (1-based), or `None` if index is out of range.
pub fn zone_index_to_endpoint(index: usize, num_zones: usize) -> Option<u16> {
    if index < num_zones && num_zones <= MAX_ZONES {
        Some((index + 1) as u16)
    } else {
        None
    }
}

/// Convert Matter endpoint to zone index
///
/// # Arguments
///
/// * `endpoint` - Matter endpoint number
/// * `num_zones` - Total number of configured zones
///
/// # Returns
///
/// Zone index (0-based), or `None` if endpoint is not a valid zone.
pub fn endpoint_to_zone_index(endpoint: u16, num_zones: usize) -> Option<usize> {
    let index = endpoint.checked_sub(FIRST_ZONE_ENDPOINT)? as usize;
    if index < num_zones && num_zones <= MAX_ZONES {
        Some(index)
    } else {
        None
    }
}

/// Device information for Matter commissioning
///
/// Contains vendor/product IDs and device metadata used during
/// Matter commissioning and discovery.
pub struct DeviceInfo {
    /// Matter vendor ID (0xFFF1 = test vendor)
    pub vendor_id: u16,
    /// Matter product ID
    pub product_id: u16,
    /// Human-readable device name
    pub device_name: &'static str,
    /// Device serial number
    pub serial_number: &'static str,
    /// Number of zones
    pub num_zones: usize,
}

impl DeviceInfo {
    /// Create device info with specified zone count
    pub fn new(num_zones: usize) -> Self {
        Self {
            vendor_id: 0xFFF1,  // Test vendor ID (for development)
            product_id: 0x8001, // Test product ID
            device_name: "EmberHeat Multi-Zone",
            serial_number: "EMBERHEAT-001",
            num_zones,
        }
    }

    /// Create device info with custom serial number
    pub fn with_serial(num_zones: usize, serial: &'static str) -> Self {
        Self {
            serial_number: serial,
            ..Self::new(num_zones)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_conversions_3_zones() {
        let num_zones = 3;

        // Valid conversions
        assert_eq!(zone_index_to_endpoint(0, num_zones), Some(1));
        assert_eq!(zone_index_to_endpoint(2, num_zones), Some(3));
        assert_eq!(zone_index_to_endpoint(3, num_zones), None); // Out of range

        assert_eq!(endpoint_to_zone_index(1, num_zones), Some(0));
        assert_eq!(endpoint_to_zone_index(3, num_zones), Some(2));
        assert_eq!(endpoint_to_zone_index(4, num_zones), None); // Out of range
        assert_eq!(endpoint_to_zone_index(0, num_zones), None); // Root endpoint
    }

    #[test]
    fn test_endpoint_conversions_7_zones() {
        let num_zones = 7;

        assert_eq!(zone_index_to_endpoint(0, num_zones), Some(1));
        assert_eq!(zone_index_to_endpoint(6, num_zones), Some(7));
        assert_eq!(zone_index_to_endpoint(7, num_zones), None);

        assert_eq!(endpoint_to_zone_index(1, num_zones), Some(0));
        assert_eq!(endpoint_to_zone_index(7, num_zones), Some(6));
        assert_eq!(endpoint_to_zone_index(8, num_zones), None);
    }

    #[test]
    fn test_max_zones_limit() {
        // Can't exceed MAX_ZONES
        assert_eq!(zone_index_to_endpoint(0, 8), None);
        assert_eq!(endpoint_to_zone_index(1, 8), None);
    }

    #[test]
    fn test_device_info() {
        let info = DeviceInfo::new(3);
        assert_eq!(info.num_zones, 3);
        assert_eq!(info.vendor_id, 0xFFF1);
    }
}
```

**Step 2: Update lib.rs exports**

Modify `emberheat-matter/src/lib.rs` to remove old exports and add new ones:

```rust
// Change the device re-exports from:
pub use device::{
    endpoint_to_zone_index, get_zone_state, get_zone_state_by_endpoint, zone_index_to_endpoint,
    DeviceInfo, NUM_ZONES, ROOT_ENDPOINT, ZONE1_ENDPOINT, ZONE2_ENDPOINT, ZONE3_ENDPOINT,
    ZONE4_ENDPOINT, ZONE5_ENDPOINT, ZONE6_ENDPOINT, ZONE7_ENDPOINT, ZONE_STATES,
};

// To:
pub use device::{
    endpoint_to_zone_index, zone_index_to_endpoint,
    DeviceInfo, MAX_ZONES, ROOT_ENDPOINT, FIRST_ZONE_ENDPOINT,
};
```

**Step 3: Run tests**

Run: `cargo test -p emberheat-matter`
Expected: All tests pass

**Step 4: Commit**

```bash
git add emberheat-matter/
git commit -m "refactor(matter): remove static ZONE_STATES, make zone count configurable

- Remove hardcoded NUM_ZONES and ZONE_STATES from emberheat-matter
- Add MAX_ZONES constant (7) as upper limit
- Update endpoint conversion functions to accept num_zones parameter
- DeviceInfo now takes num_zones in constructor
- Zone states will be owned by emberheat-esp32"
```

---

### Task 2: Create Zone States in emberheat-esp32

**Files:**
- Create: `emberheat-esp32/src/matter/zones.rs`
- Modify: `emberheat-esp32/src/matter/mod.rs`
- Modify: `emberheat-esp32/src/matter/bridge.rs`

**Step 1: Create zones.rs with static zone states**

Create `emberheat-esp32/src/matter/zones.rs`:

```rust
//! Zone state storage for Matter integration
//!
//! Owns the static ZoneThermostatState array, sized according to config.

use crate::config::NUM_ZONES;
use emberheat_matter::state::ZoneThermostatState;

/// Static state for all configured zones
///
/// Sized according to NUM_ZONES in config.rs.
/// The control loop and Matter handlers access these via atomic operations.
pub static ZONE_STATES: [ZoneThermostatState; NUM_ZONES] = {
    // Use a const block to initialize the array
    // This works because ZoneThermostatState::new() is const
    const INIT: ZoneThermostatState = ZoneThermostatState::new();
    [INIT; NUM_ZONES]
};

/// Get zone state by index
///
/// # Arguments
///
/// * `index` - Zone index (0 to NUM_ZONES-1)
///
/// # Returns
///
/// Reference to the zone state, or `None` if index is out of range.
pub fn get_zone_state(index: usize) -> Option<&'static ZoneThermostatState> {
    ZONE_STATES.get(index)
}

/// Get zone state by Matter endpoint number
///
/// # Arguments
///
/// * `endpoint` - Matter endpoint number (1 to NUM_ZONES)
///
/// # Returns
///
/// Reference to the zone state, or `None` if endpoint is not a valid zone.
pub fn get_zone_state_by_endpoint(endpoint: u16) -> Option<&'static ZoneThermostatState> {
    emberheat_matter::endpoint_to_zone_index(endpoint, NUM_ZONES)
        .and_then(get_zone_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_states_length() {
        assert_eq!(ZONE_STATES.len(), NUM_ZONES);
    }

    #[test]
    fn test_get_zone_state() {
        // Valid indices
        for i in 0..NUM_ZONES {
            assert!(get_zone_state(i).is_some());
        }
        // Out of range
        assert!(get_zone_state(NUM_ZONES).is_none());
    }

    #[test]
    fn test_get_zone_state_by_endpoint() {
        // Valid endpoints (1 to NUM_ZONES)
        for ep in 1..=NUM_ZONES as u16 {
            assert!(get_zone_state_by_endpoint(ep).is_some());
        }
        // Root endpoint
        assert!(get_zone_state_by_endpoint(0).is_none());
        // Out of range
        assert!(get_zone_state_by_endpoint(NUM_ZONES as u16 + 1).is_none());
    }
}
```

**Step 2: Update matter/mod.rs**

Modify `emberheat-esp32/src/matter/mod.rs`:

```rust
//! Matter protocol integration for emberheat-esp32
//!
//! Re-exports emberheat-matter and provides the bridge to emberheat-core.

pub mod bridge;
pub mod zones;

// Re-export everything from emberheat-matter
pub use emberheat_matter::*;

// Re-export local zone state functions
pub use zones::{get_zone_state, get_zone_state_by_endpoint, ZONE_STATES};

// Re-export bridge functions
pub use bridge::{
    get_matter_setpoint, is_zone_enabled_in_matter, set_matter_mode, set_matter_setpoint,
    sync_local_state_to_matter, update_matter_from_control_loop,
};
```

**Step 3: Update bridge.rs to use local zones**

Modify `emberheat-esp32/src/matter/bridge.rs` imports:

```rust
// Change from:
use emberheat_matter::device::get_zone_state;
use emberheat_matter::types::SystemMode;

// To:
use crate::config::NUM_ZONES;
use crate::matter::zones::get_zone_state;
use emberheat_matter::types::SystemMode;
```

Also update the test imports in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::zones::ZONE_STATES;
    // ... rest of tests
}
```

**Step 4: Verify compilation**

Run: `WIFI_SSID=test WIFI_PASSWORD=test cargo check -p emberheat-esp32 --target riscv32imac-unknown-none-elf`
Expected: Compiles successfully

**Step 5: Commit**

```bash
git add emberheat-esp32/src/matter/
git commit -m "feat(esp32): move ZONE_STATES to emberheat-esp32, sized by NUM_ZONES

- Create zones.rs with ZONE_STATES array sized by config::NUM_ZONES
- Update bridge.rs to use local zone state functions
- Re-export zone functions from matter/mod.rs"
```

---

### Task 3: Make main.rs Zone Initialization Conditional

**Files:**
- Modify: `emberheat-esp32/src/main.rs`

**Step 1: Add conditional compilation for static cells**

The static cells and task functions need to exist for zones that might be used. We'll keep all 7 but only initialize/spawn based on NUM_ZONES.

Update the static cell declarations in main.rs to use conditional compilation or keep them all but guard the usage. Since Embassy tasks must be concrete functions, we keep all 7 task definitions but guard spawning.

Modify the main function's zone initialization section:

```rust
// In main(), replace the zone initialization with conditional blocks:

    // Initialize zones based on NUM_ZONES configuration
    info!("Initializing {} zones...", NUM_ZONES);

    // Zone 1 (always initialized if NUM_ZONES >= 1)
    let (sensor1, pin1, state1) = if NUM_ZONES >= 1 {
        info!("  Initializing Zone1 GPIO{}...", ZONE_CONFIGS[0].relay_pin);
        let pin = PIN_1.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO0,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_1.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[0].name));
        let state = STATE_1.init(ZoneState::with_target(ZONE_CONFIGS[0].default_target_c));
        info!("  Zone1 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 2
    let (sensor2, pin2, state2) = if NUM_ZONES >= 2 {
        info!("  Initializing Zone2 GPIO{}...", ZONE_CONFIGS[1].relay_pin);
        let pin = PIN_2.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO1,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_2.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[1].name));
        let state = STATE_2.init(ZoneState::with_target(ZONE_CONFIGS[1].default_target_c));
        info!("  Zone2 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // ... repeat pattern for zones 3-7 ...
```

**Step 2: Add conditional task spawning**

```rust
    // Spawn control tasks only for configured zones
    info!("Spawning zone control tasks...");

    if let (Some(sensor), Some(pin), Some(state)) = (sensor1, pin1, state1) {
        spawner
            .spawn(zone1_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone1 task");
        info!("  Zone1 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor2, pin2, state2) {
        spawner
            .spawn(zone2_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone2 task");
        info!("  Zone2 task spawned");
    }

    // ... repeat for zones 3-7 ...

    info!("All {} zone control tasks running", NUM_ZONES);
```

**Step 3: Verify compilation with different zone counts**

Test with 3 zones - temporarily change `config.rs`:
```rust
pub const NUM_ZONES: usize = 3;
```

And trim ZONE_CONFIGS to 3 entries.

Run: `WIFI_SSID=test WIFI_PASSWORD=test cargo check -p emberheat-esp32 --target riscv32imac-unknown-none-elf`
Expected: Compiles with no warnings about unused zones

Restore to 7 zones after testing.

**Step 4: Commit**

```bash
git add emberheat-esp32/src/main.rs
git commit -m "feat(esp32): conditional zone initialization based on NUM_ZONES

- Only initialize GPIO, sensors, and state for configured zones
- Only spawn tasks for zones where NUM_ZONES >= zone_number
- Reduces memory usage when running fewer than 7 zones"
```

---

### Task 4: Update Documentation and Tests

**Files:**
- Modify: `emberheat-esp32/src/config.rs`
- Modify: `emberheat-matter/src/lib.rs` (doc comments)

**Step 1: Update config.rs documentation**

Update the module doc comment in `emberheat-esp32/src/config.rs`:

```rust
//! Multi-Zone Thermostat Configuration
//!
//! This module defines the compile-time configuration for the thermostat.
//! The number of zones is configurable via `NUM_ZONES` (1-7 supported).
//!
//! # Configuring Zone Count
//!
//! To change the number of zones:
//! 1. Set `NUM_ZONES` to desired value (1-7)
//! 2. Ensure `ZONE_CONFIGS` has exactly `NUM_ZONES` entries
//! 3. Rebuild the firmware
//!
//! # Example: 3-Zone Configuration
//!
//! ```ignore
//! pub const NUM_ZONES: usize = 3;
//!
//! pub const ZONE_CONFIGS: [ZoneConfigData; NUM_ZONES] = [
//!     ZoneConfigData { name: "Zone1", relay_pin: 0, ... },
//!     ZoneConfigData { name: "Zone2", relay_pin: 1, ... },
//!     ZoneConfigData { name: "Zone3", relay_pin: 2, ... },
//! ];
//! ```
```

**Step 2: Fix test that asserts NUM_ZONES == 7**

Update the test in config.rs:

```rust
#[test]
fn test_num_zones() {
    // NUM_ZONES should match ZONE_CONFIGS length
    assert_eq!(ZONE_CONFIGS.len(), NUM_ZONES);
    // NUM_ZONES should be in valid range
    assert!(NUM_ZONES >= 1 && NUM_ZONES <= 7);
}
```

**Step 3: Run all tests**

Run: `cargo test -p emberheat-core -p emberheat-matter`
Expected: All tests pass

**Step 4: Commit**

```bash
git add emberheat-esp32/src/config.rs emberheat-matter/
git commit -m "docs: update documentation for configurable zone count

- Update config.rs module docs with zone configuration instructions
- Fix test to validate NUM_ZONES is in range rather than exactly 7
- Add example for 3-zone configuration"
```

---

### Task 5: Update CI to Test with Different Zone Counts (Optional)

**Files:**
- Modify: `.github/workflows/ci.yml` (optional)

This task is optional but recommended. Add a matrix build that tests compilation with different zone counts.

**Step 1: Add zone count matrix to CI**

This would require a way to set NUM_ZONES at build time, which would need more refactoring (feature flags or build.rs). Skip for now unless explicitly requested.

**Step 2: Commit**

Skip - marked as optional future work.

---

## Validation Checklist

Before considering this complete:

- [ ] `cargo test -p emberheat-matter` passes
- [ ] `cargo test -p emberheat-core` passes  
- [ ] `cargo check -p emberheat-esp32 --target riscv32imac-unknown-none-elf` succeeds
- [ ] Setting `NUM_ZONES = 3` and reducing `ZONE_CONFIGS` to 3 entries compiles
- [ ] Setting `NUM_ZONES = 7` with full `ZONE_CONFIGS` compiles
- [ ] Flash and boot with 7 zones works (existing behavior)
- [ ] Flash and boot with 3 zones works (new behavior)

---

## Summary

After implementation:

| Component | Change |
|-----------|--------|
| `emberheat-matter` | No longer owns zone states; provides helper functions only |
| `emberheat-esp32/config.rs` | Single source of truth for `NUM_ZONES` |
| `emberheat-esp32/matter/zones.rs` | New file owning `ZONE_STATES` array |
| `emberheat-esp32/main.rs` | Conditional initialization based on `NUM_ZONES` |

To configure zone count, users edit one constant in `config.rs` and ensure `ZONE_CONFIGS` array matches.
