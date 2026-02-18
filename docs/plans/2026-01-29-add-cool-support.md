# Add COOL Support Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add COOL feature support to the Matter thermostat to improve Apple HomeKit compatibility, even though the physical device only supports heating.

**Architecture:** Extend the existing heating-only thermostat to advertise both HEAT and COOL features. The device will accept Cool mode and cooling setpoint values from HomeKit but will only physically activate heating. This mirrors the HAP approach where supporting both modes improves compatibility.

**Tech Stack:** Rust, rs-matter, ESP32-C6, no_std

---

## Background

Apple HomeKit shows "Not Supported" for our heating-only thermostat despite correct Matter protocol implementation. Adding COOL support (even if unused physically) may resolve this, similar to HAP where supporting both heat/cool improves compatibility.

## Files Overview

| File | Purpose |
|------|---------|
| `emberheat-matter/src/types.rs` | Add cooling setpoint defaults |
| `emberheat-matter/src/state.rs` | Add cooling setpoint state + is_cooling |
| `emberheat-matter/src/cluster.rs` | Update to HEATING_AND_COOLING_CLUSTER |
| `emberheat-esp32/src/matter/stack.rs` | Add cooling attributes + update feature map |

---

### Task 1: Add Cooling Defaults to types.rs

**Files:**
- Modify: `emberheat-matter/src/types.rs:174-184`
- Test: `emberheat-matter/src/types.rs` (existing test module)

**Step 1: Add cooling setpoint defaults**

Add to the `defaults` module after the heating constants:

```rust
/// Default cooling setpoint: 24°C (2400 centi-degrees)
pub const OCCUPIED_COOLING_SETPOINT: i16 = 2400;

/// Minimum cooling setpoint: 10°C
pub const MIN_COOL_SETPOINT: i16 = 1000;

/// Maximum cooling setpoint: 35°C
pub const MAX_COOL_SETPOINT: i16 = 3500;
```

**Step 2: Update is_valid_for_heating_only to is_valid_for_heat_cool**

In the `SystemMode` impl, add a new validation method:

```rust
/// Check if this mode is valid for a heating+cooling thermostat
pub fn is_valid_for_heat_cool(&self) -> bool {
    matches!(self, Self::Off | Self::Heat | Self::Cool)
}
```

**Step 3: Add RunningState cooling methods**

Add to the `RunningState` impl:

```rust
/// Set whether cooling is active
pub fn set_cooling(&mut self, active: bool) {
    if active {
        self.0 |= Self::COOL;
    } else {
        self.0 &= !Self::COOL;
    }
}

/// Check if cooling is currently active
pub fn is_cooling(&self) -> bool {
    self.0 & Self::COOL != 0
}
```

**Step 4: Add tests for new functionality**

```rust
#[test]
fn test_cooling_defaults() {
    assert_eq!(defaults::OCCUPIED_COOLING_SETPOINT, 2400); // 24°C
    assert_eq!(defaults::MIN_COOL_SETPOINT, 1000); // 10°C
    assert_eq!(defaults::MAX_COOL_SETPOINT, 3500); // 35°C
}

#[test]
fn test_system_mode_valid_for_heat_cool() {
    assert!(SystemMode::Off.is_valid_for_heat_cool());
    assert!(SystemMode::Heat.is_valid_for_heat_cool());
    assert!(SystemMode::Cool.is_valid_for_heat_cool());
    assert!(!SystemMode::Auto.is_valid_for_heat_cool());
}

#[test]
fn test_running_state_cooling() {
    let mut state = RunningState::new();
    assert!(!state.is_cooling());

    state.set_cooling(true);
    assert!(state.is_cooling());
    assert_eq!(state.0, RunningState::COOL);

    state.set_cooling(false);
    assert!(!state.is_cooling());
}
```

**Step 5: Run tests**

Run: `cargo test -p emberheat-matter`
Expected: All tests pass

**Step 6: Commit**

```bash
git add emberheat-matter/src/types.rs
git commit -m "feat(matter): add cooling type definitions and defaults"
```

---

### Task 2: Add Cooling State to ZoneThermostatState

**Files:**
- Modify: `emberheat-matter/src/state.rs`

**Step 1: Add occupied_cooling_setpoint field**

Add to `ZoneThermostatState` struct after `occupied_heating_setpoint`:

```rust
/// Target cooling setpoint in Matter centi-degrees
occupied_cooling_setpoint: AtomicI16,
```

**Step 2: Initialize in new()**

Add to the `new()` function:

```rust
occupied_cooling_setpoint: AtomicI16::new(defaults::OCCUPIED_COOLING_SETPOINT),
```

**Step 3: Add cooling setpoint getters/setters**

Add after the heating setpoint section:

```rust
// ---- Cooling Setpoint getters/setters ----

/// Get the occupied cooling setpoint in Matter centi-degrees
pub fn get_occupied_cooling_setpoint(&self) -> i16 {
    self.occupied_cooling_setpoint.load(Ordering::Relaxed)
}

/// Set the occupied cooling setpoint in Matter centi-degrees
///
/// # Errors
///
/// Returns `SetpointError::BelowMinimum` if below 10°C.
/// Returns `SetpointError::AboveMaximum` if above 35°C.
pub fn set_occupied_cooling_setpoint(
    &self,
    setpoint_centi_c: i16,
) -> Result<(), SetpointError> {
    if setpoint_centi_c < defaults::MIN_COOL_SETPOINT {
        return Err(SetpointError::BelowMinimum);
    }
    if setpoint_centi_c > defaults::MAX_COOL_SETPOINT {
        return Err(SetpointError::AboveMaximum);
    }
    self.occupied_cooling_setpoint
        .store(setpoint_centi_c, Ordering::Relaxed);
    Ok(())
}

/// Get the cooling setpoint in Celsius
pub fn get_cooling_setpoint_celsius(&self) -> f32 {
    temperature::matter_to_celsius(self.get_occupied_cooling_setpoint())
}
```

**Step 4: Add set_cooling_active method**

Add after `set_heating_active`:

```rust
/// Set whether cooling is currently active
pub fn set_cooling_active(&self, active: bool) {
    let mut state = self.get_running_state();
    state.set_cooling(active);
    self.running_state.store(state.0, Ordering::Relaxed);
}

/// Check if cooling is currently active
pub fn is_cooling(&self) -> bool {
    self.get_running_state().is_cooling()
}
```

**Step 5: Update is_enabled() to include Cool mode**

```rust
/// Check if the thermostat is enabled (mode is Heat or Cool)
pub fn is_enabled(&self) -> bool {
    matches!(self.get_system_mode(), SystemMode::Heat | SystemMode::Cool)
}
```

**Step 6: Add tests**

```rust
#[test]
fn test_cooling_setpoint_operations() {
    let state = ZoneThermostatState::new();

    // Default is 24°C
    assert_eq!(state.get_occupied_cooling_setpoint(), 2400);
    assert_eq!(state.get_cooling_setpoint_celsius(), 24.0);

    // Valid setpoint
    assert!(state.set_occupied_cooling_setpoint(2600).is_ok());
    assert_eq!(state.get_cooling_setpoint_celsius(), 26.0);

    // Below minimum
    assert_eq!(
        state.set_occupied_cooling_setpoint(500),
        Err(SetpointError::BelowMinimum)
    );

    // Above maximum
    assert_eq!(
        state.set_occupied_cooling_setpoint(4000),
        Err(SetpointError::AboveMaximum)
    );
}

#[test]
fn test_cooling_state() {
    let state = ZoneThermostatState::new();

    assert!(!state.is_cooling());

    state.set_cooling_active(true);
    assert!(state.is_cooling());

    state.set_cooling_active(false);
    assert!(!state.is_cooling());
}

#[test]
fn test_is_enabled_includes_cool() {
    let state = ZoneThermostatState::new();

    state.set_system_mode(SystemMode::Cool);
    assert!(state.is_enabled());

    state.set_system_mode(SystemMode::Off);
    assert!(!state.is_enabled());
}
```

**Step 7: Run tests**

Run: `cargo test -p emberheat-matter`
Expected: All tests pass

**Step 8: Commit**

```bash
git add emberheat-matter/src/state.rs
git commit -m "feat(matter): add cooling setpoint state management"
```

---

### Task 3: Update ThermostatHooks and Cluster Definition

**Files:**
- Modify: `emberheat-matter/src/cluster.rs`
- Modify: `emberheat-matter/src/state.rs` (ThermostatHooks impl)

**Step 1: Create HEATING_AND_COOLING_CLUSTER in cluster.rs**

Add after `HEATING_ONLY_CLUSTER`:

```rust
/// Heating and cooling thermostat cluster configuration
///
/// Supports both HEAT and COOL features for better HomeKit compatibility.
pub const HEATING_AND_COOLING_CLUSTER: Cluster<'static> = thermostat::FULL_CLUSTER
    .with_features((thermostat::Feature::HEATING | thermostat::Feature::COOLING).bits())
    .with_attrs(with!(
        required;
        thermostat::AttributeId::LocalTemperature
            | thermostat::AttributeId::OccupiedHeatingSetpoint
            | thermostat::AttributeId::OccupiedCoolingSetpoint
            | thermostat::AttributeId::MinHeatSetpointLimit
            | thermostat::AttributeId::MaxHeatSetpointLimit
            | thermostat::AttributeId::MinCoolSetpointLimit
            | thermostat::AttributeId::MaxCoolSetpointLimit
            | thermostat::AttributeId::ControlSequenceOfOperation
            | thermostat::AttributeId::SystemMode
            | thermostat::AttributeId::ThermostatRunningState
    ));
```

**Step 2: Add cooling methods to ThermostatHooks trait**

Add after the heating setpoint methods:

```rust
// --- Cooling Setpoint ---

/// Get occupied cooling setpoint in centi-degrees
fn occupied_cooling_setpoint(&self) -> i16;

/// Set occupied cooling setpoint in centi-degrees
fn set_occupied_cooling_setpoint(&self, value: i16) -> Result<(), Error>;

/// Get minimum cool setpoint limit in centi-degrees
fn min_cool_setpoint_limit(&self) -> i16;

/// Get maximum cool setpoint limit in centi-degrees
fn max_cool_setpoint_limit(&self) -> i16;
```

**Step 3: Update control_sequence_of_operation default**

Change the default implementation:

```rust
fn control_sequence_of_operation(&self) -> ControlSequenceOfOperationEnum {
    ControlSequenceOfOperationEnum::CoolingAndHeating
}
```

**Step 4: Update reference impl for ThermostatHooks**

Add to the `impl<T> ThermostatHooks for &T` block:

```rust
fn occupied_cooling_setpoint(&self) -> i16 {
    (*self).occupied_cooling_setpoint()
}

fn set_occupied_cooling_setpoint(&self, value: i16) -> Result<(), Error> {
    (*self).set_occupied_cooling_setpoint(value)
}

fn min_cool_setpoint_limit(&self) -> i16 {
    (*self).min_cool_setpoint_limit()
}

fn max_cool_setpoint_limit(&self) -> i16 {
    (*self).max_cool_setpoint_limit()
}
```

**Step 5: Update ThermostatHooks impl for ZoneThermostatState in state.rs**

Change the CLUSTER constant and add cooling methods:

```rust
impl ThermostatHooks for ZoneThermostatState {
    const CLUSTER: Cluster<'static> = HEATING_AND_COOLING_CLUSTER;

    // ... existing methods ...

    fn occupied_cooling_setpoint(&self) -> i16 {
        self.get_occupied_cooling_setpoint()
    }

    fn set_occupied_cooling_setpoint(&self, value: i16) -> Result<(), Error> {
        ZoneThermostatState::set_occupied_cooling_setpoint(self, value)
            .map_err(|_| ErrorCode::ConstraintError.into())
    }

    fn min_cool_setpoint_limit(&self) -> i16 {
        defaults::MIN_COOL_SETPOINT
    }

    fn max_cool_setpoint_limit(&self) -> i16 {
        defaults::MAX_COOL_SETPOINT
    }

    fn control_sequence_of_operation(&self) -> ControlSequenceOfOperationEnum {
        ControlSequenceOfOperationEnum::CoolingAndHeating
    }
}
```

**Step 6: Update set_system_mode to accept Cool**

In the `set_system_mode` method:

```rust
fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error> {
    let our_mode = match mode {
        SystemModeEnum::Off => SystemMode::Off,
        SystemModeEnum::Heat => SystemMode::Heat,
        SystemModeEnum::Cool => SystemMode::Cool,
        // Reject unsupported modes (Auto, etc.)
        _ => return Err(ErrorCode::ConstraintError.into()),
    };
    ZoneThermostatState::set_system_mode(self, our_mode);
    Ok(())
}
```

**Step 7: Update imports in state.rs**

Change the import to include the new cluster:

```rust
use crate::cluster::{
    ControlSequenceOfOperationEnum, RelayStateBitmap, SystemModeEnum, ThermostatHooks,
    HEATING_AND_COOLING_CLUSTER,
};
```

**Step 8: Run tests**

Run: `cargo test -p emberheat-matter`
Expected: All tests pass

**Step 9: Commit**

```bash
git add emberheat-matter/src/cluster.rs emberheat-matter/src/state.rs
git commit -m "feat(matter): add cooling support to ThermostatHooks trait"
```

---

### Task 4: Update ESP32 Matter Stack Handler

**Files:**
- Modify: `emberheat-esp32/src/matter/stack.rs`

**Step 1: Add cooling attribute IDs**

Add to the `attr` module:

```rust
/// Absolute minimum cool setpoint limit (hardware limit) - mandatory with COOL feature
pub const ABS_MIN_COOL_SETPOINT_LIMIT: u16 = 0x0005;
/// Absolute maximum cool setpoint limit (hardware limit) - mandatory with COOL feature
pub const ABS_MAX_COOL_SETPOINT_LIMIT: u16 = 0x0006;
/// Target cooling temperature in 0.01°C units (i16)
pub const OCCUPIED_COOLING_SETPOINT: u16 = 0x0011;
/// Minimum allowed cooling setpoint (user-adjustable limit, i16)
pub const MIN_COOL_SETPOINT_LIMIT: u16 = 0x0017;
/// Maximum allowed cooling setpoint (user-adjustable limit, i16)
pub const MAX_COOL_SETPOINT_LIMIT: u16 = 0x0018;
```

**Step 2: Update thermostat_attr_name function**

Add the cooling attribute names:

```rust
0x0005 => "AbsMinCoolSetpointLimit",
0x0006 => "AbsMaxCoolSetpointLimit",
0x0011 => "OccupiedCoolingSetpoint",
0x0017 => "MinCoolSetpointLimit",
0x0018 => "MaxCoolSetpointLimit",
```

**Step 3: Add cooling attribute read handlers**

Add to the `read` method's match block (after heating attributes):

```rust
// OccupiedCoolingSetpoint - i16
x if x == attr::OCCUPIED_COOLING_SETPOINT as u32 => {
    let setpoint = zone.get_occupied_cooling_setpoint();
    let setpoint_c = setpoint as f32 / 100.0;
    info!("  -> OccupiedCoolingSetpoint: {} ({}°C)", setpoint, setpoint_c);
    Reply::set(writer, setpoint)?;
}

// AbsMinCoolSetpointLimit - i16
x if x == attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32 => {
    info!("  -> AbsMinCoolSetpointLimit: {}", defaults::MIN_COOL_SETPOINT);
    Reply::set(writer, defaults::MIN_COOL_SETPOINT)?;
}

// AbsMaxCoolSetpointLimit - i16
x if x == attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32 => {
    info!("  -> AbsMaxCoolSetpointLimit: {}", defaults::MAX_COOL_SETPOINT);
    Reply::set(writer, defaults::MAX_COOL_SETPOINT)?;
}

// MinCoolSetpointLimit - i16
x if x == attr::MIN_COOL_SETPOINT_LIMIT as u32 => {
    info!("  -> MinCoolSetpointLimit: {}", defaults::MIN_COOL_SETPOINT);
    Reply::set(writer, defaults::MIN_COOL_SETPOINT)?;
}

// MaxCoolSetpointLimit - i16
x if x == attr::MAX_COOL_SETPOINT_LIMIT as u32 => {
    info!("  -> MaxCoolSetpointLimit: {}", defaults::MAX_COOL_SETPOINT);
    Reply::set(writer, defaults::MAX_COOL_SETPOINT)?;
}
```

**Step 4: Update ControlSequenceOfOperation read handler**

Change from HeatingOnly to CoolingAndHeating:

```rust
x if x == attr::CONTROL_SEQUENCE_OF_OPERATION as u32 => {
    let val = ControlSequenceOfOperationEnum::CoolingAndHeating as u8;
    info!("  -> ControlSequenceOfOperation: {} (CoolingAndHeating)", val);
    Reply::set(writer, val)?;
}
```

**Step 5: Update SystemMode read to handle Cool**

```rust
x if x == attr::SYSTEM_MODE as u32 => {
    let mode = zone.get_system_mode();
    let val = match mode {
        SystemMode::Off => SystemModeEnum::Off as u8,
        SystemMode::Heat => SystemModeEnum::Heat as u8,
        SystemMode::Cool => SystemModeEnum::Cool as u8,
        _ => SystemModeEnum::Off as u8,
    };
    info!("  -> SystemMode: {} ({:?})", val, mode);
    Reply::set(writer, val)?;
}
```

**Step 6: Update FeatureMap to include COOL**

```rust
0xFFFC => {
    let feature_map: u32 = 0x03; // HEAT (0x01) + COOL (0x02)
    info!("  -> FeatureMap: 0x{:08X} (HEAT+COOL)", feature_map);
    Reply::set(writer, feature_map)?;
}
```

**Step 7: Update AttributeList to include cooling attributes**

```rust
0xFFFB => {
    info!("  -> AttributeList: returning supported attributes");
    let attrs: &[u32] = &[
        attr::LOCAL_TEMPERATURE as u32,
        attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32,
        attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32,
        attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32,
        attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32,
        attr::OCCUPIED_HEATING_SETPOINT as u32,
        attr::OCCUPIED_COOLING_SETPOINT as u32,
        attr::MIN_HEAT_SETPOINT_LIMIT as u32,
        attr::MAX_HEAT_SETPOINT_LIMIT as u32,
        attr::MIN_COOL_SETPOINT_LIMIT as u32,
        attr::MAX_COOL_SETPOINT_LIMIT as u32,
        attr::CONTROL_SEQUENCE_OF_OPERATION as u32,
        attr::SYSTEM_MODE as u32,
        attr::THERMOSTAT_RUNNING_STATE as u32,
        0xFFF8, // GeneratedCommandList
        0xFFF9, // AcceptedCommandList
        0xFFFB, // AttributeList
        0xFFFC, // FeatureMap
        0xFFFD, // ClusterRevision
    ];
    Reply::set(writer, attrs)?;
}
```

**Step 8: Add cooling setpoint write handler**

Add to the `write` method:

```rust
// OccupiedCoolingSetpoint - writable
x if x == attr::OCCUPIED_COOLING_SETPOINT as u32 => {
    let value: i16 = ctx.data().i16()?;

    if value < defaults::MIN_COOL_SETPOINT || value > defaults::MAX_COOL_SETPOINT {
        return Err(ErrorCode::ConstraintError.into());
    }

    zone.set_occupied_cooling_setpoint(value)
        .map_err(|_| Error::from(ErrorCode::ConstraintError))?;

    self.dataver_changed();
    ctx.notify_changed();
}
```

**Step 9: Update SystemMode write to accept Cool**

```rust
x if x == attr::SYSTEM_MODE as u32 => {
    let value: u8 = ctx.data().u8()?;

    let mode = match value {
        x if x == SystemModeEnum::Off as u8 => SystemMode::Off,
        x if x == SystemModeEnum::Heat as u8 => SystemMode::Heat,
        x if x == SystemModeEnum::Cool as u8 => SystemMode::Cool,
        _ => return Err(ErrorCode::ConstraintError.into()),
    };

    zone.set_system_mode(mode);
    self.dataver_changed();
    ctx.notify_changed();
}
```

**Step 10: Update read-only attribute list**

Add the cooling setpoint limits to the read-only check:

```rust
x if x == attr::LOCAL_TEMPERATURE as u32
    || x == attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32
    || x == attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32
    || x == attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32
    || x == attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32
    || x == attr::MIN_HEAT_SETPOINT_LIMIT as u32
    || x == attr::MAX_HEAT_SETPOINT_LIMIT as u32
    || x == attr::MIN_COOL_SETPOINT_LIMIT as u32
    || x == attr::MAX_COOL_SETPOINT_LIMIT as u32
    || x == attr::CONTROL_SEQUENCE_OF_OPERATION as u32
    || x == attr::THERMOSTAT_RUNNING_STATE as u32 =>
{
    return Err(ErrorCode::UnsupportedAccess.into());
}
```

**Step 11: Run build**

Run: `make build-esp32`
Expected: Build succeeds

**Step 12: Commit**

```bash
git add emberheat-esp32/src/matter/stack.rs
git commit -m "feat(esp32): add cooling attribute handlers to Matter stack"
```

---

### Task 5: Update THERMOSTAT_ATTRIBUTES and THERMOSTAT_CLUSTER

**Files:**
- Modify: `emberheat-esp32/src/matter/stack.rs`

**Step 1: Add cooling attributes to THERMOSTAT_ATTRIBUTES**

Update the const to include cooling attributes:

```rust
const THERMOSTAT_ATTRIBUTES: &[Attribute] = &[
    // LocalTemperature (0x0000) - nullable i16, read-only
    Attribute::new(attr::LOCAL_TEMPERATURE as u32, Access::RV, Quality::X),
    // AbsMinHeatSetpointLimit (0x0003) - i16, read-only
    Attribute::new(attr::ABS_MIN_HEAT_SETPOINT_LIMIT as u32, Access::RV, Quality::F),
    // AbsMaxHeatSetpointLimit (0x0004) - i16, read-only
    Attribute::new(attr::ABS_MAX_HEAT_SETPOINT_LIMIT as u32, Access::RV, Quality::F),
    // AbsMinCoolSetpointLimit (0x0005) - i16, read-only
    Attribute::new(attr::ABS_MIN_COOL_SETPOINT_LIMIT as u32, Access::RV, Quality::F),
    // AbsMaxCoolSetpointLimit (0x0006) - i16, read-only
    Attribute::new(attr::ABS_MAX_COOL_SETPOINT_LIMIT as u32, Access::RV, Quality::F),
    // OccupiedCoolingSetpoint (0x0011) - i16, read-write
    Attribute::new(attr::OCCUPIED_COOLING_SETPOINT as u32, Access::RWVM, Quality::SN),
    // OccupiedHeatingSetpoint (0x0012) - i16, read-write
    Attribute::new(attr::OCCUPIED_HEATING_SETPOINT as u32, Access::RWVM, Quality::SN),
    // MinHeatSetpointLimit (0x0015) - i16, read-only
    Attribute::new(attr::MIN_HEAT_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // MaxHeatSetpointLimit (0x0016) - i16, read-only
    Attribute::new(attr::MAX_HEAT_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // MinCoolSetpointLimit (0x0017) - i16, read-only
    Attribute::new(attr::MIN_COOL_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // MaxCoolSetpointLimit (0x0018) - i16, read-only
    Attribute::new(attr::MAX_COOL_SETPOINT_LIMIT as u32, Access::RV, Quality::N),
    // ControlSequenceOfOperation (0x001B) - enum8, read-only
    Attribute::new(attr::CONTROL_SEQUENCE_OF_OPERATION as u32, Access::RV, Quality::N),
    // SystemMode (0x001C) - enum8, read-write
    Attribute::new(attr::SYSTEM_MODE as u32, Access::RWVM, Quality::SN),
    // ThermostatRunningState (0x0029) - bitmap16, read-only
    Attribute::new(attr::THERMOSTAT_RUNNING_STATE as u32, Access::RV, Quality::NONE),
];
```

**Step 2: Update THERMOSTAT_CLUSTER feature map**

```rust
pub const THERMOSTAT_CLUSTER: Cluster<'static> = Cluster::new(
    CLUSTER_THERMOSTAT,    // cluster ID 0x0201
    6,                     // revision (Matter 1.3 thermostat spec)
    0x03,                  // feature map: Heating + Cooling features
    THERMOSTAT_ATTRIBUTES,
    &[],
    |_, _, _| true,
    |_, _, _| true,
);
```

**Step 3: Update logging in matter_task functions**

Update the info! statements in both `matter_task` and `matter_task_dual_radio`:

```rust
info!("  Endpoint 1 clusters: Identify(0x03), Binding(0x1E), ThermostatUI(0x204), Descriptor(0x1D), Thermostat(0x201) [HEAT+COOL]");
```

**Step 4: Run build**

Run: `make build-esp32`
Expected: Build succeeds

**Step 5: Run tests**

Run: `make test`
Expected: All tests pass

**Step 6: Commit**

```bash
git add emberheat-esp32/src/matter/stack.rs
git commit -m "feat(esp32): update cluster definition for HEAT+COOL support"
```

---

### Task 6: Test with HomeKit

**Step 1: Flash firmware**

Run: `make flash-monitor`

**Step 2: Factory reset and re-commission with HomeKit**

Commission the device fresh with HomeKit and observe:
1. Device should show thermostat controls (not "Not Supported")
2. Logs should show cooling attribute reads
3. SystemMode writes for Heat/Cool should work

**Step 3: Verify in logs**

Expected log output should include:
```
INFO - Matter: Device Configuration:
INFO -   Endpoint 1 clusters: ... Thermostat(0x201) [HEAT+COOL]
INFO -   -> FeatureMap: 0x00000003 (HEAT+COOL)
INFO -   -> ControlSequenceOfOperation: 4 (CoolingAndHeating)
```

**Step 4: Document results**

Create a note about whether HomeKit now works with the HEAT+COOL configuration.

---

## Summary

This plan adds COOL support by:
1. Adding cooling type definitions and defaults
2. Adding cooling setpoint state management
3. Updating the ThermostatHooks trait
4. Adding cooling attribute handlers to the ESP32 stack
5. Updating the cluster definition to advertise HEAT+COOL

The physical device still only heats - Cool mode is accepted but has no physical effect. This mirrors the HAP approach where supporting both modes improves Apple compatibility.
