# HomeKit "Not Supported" Issue - Debugging Report

**Date:** 2026-01-29
**Status:** Resolved
**Root Cause:** Missing global attributes in THERMOSTAT_ATTRIBUTES array

## Summary

After commissioning a Matter thermostat (device type 0x0301) to Apple HomeKit, the device showed "This accessory is not currently supported by the Home" despite successful commissioning and correct Matter protocol implementation.

## Symptoms

- Device commissioned successfully to HomeKit via BLE+WiFi
- HomeKit displayed "Not Supported" message
- No thermostat controls visible in Home app
- Device worked correctly with Home Assistant

## Investigation Timeline

### Initial Hypothesis: Missing Features

We suspected HomeKit required both HEAT and COOL features, similar to how HAP (HomeKit Accessory Protocol) worked better with combined heat/cool support.

**Actions taken:**
1. Added COOL feature support (FeatureMap 0x03 = HEAT + COOL)
2. Added OccupiedCoolingSetpoint attribute and handler
3. Added cooling setpoint limits (AbsMin/Max, Min/Max)
4. Changed ControlSequenceOfOperation from HeatingOnly (2) to CoolingAndHeating (4)

**Result:** Still showed "Not Supported"

### Second Hypothesis: Developer Profile Required

Research indicated Apple might reject devices with test vendor IDs (0xFFF1) without the Matter Developer Profile installed.

**Actions taken:**
- Verified developer profile was installed on iOS device
- Confirmed profile should also be on Home Hub (Apple TV/HomePod)

**Result:** Profile was already correctly installed

### Third Hypothesis: Panic on Multi-Fabric

Observed a panic when adding a 3rd fabric (chip-tool after HomeKit's 2 fabrics):
```
panicked at rs-matter: No space
```

**Actions taken:**
- Verified MAX_FABRICS=5 feature was enabled
- Created GitHub issue #38 to track

**Result:** Unrelated to HomeKit issue, but blocked chip-tool debugging

### Breakthrough: chip-tool Testing

Used chip-tool with fresh KVS directory to test directly:

```bash
chip-tool any read-by-id 0x0201 0xFFFC 1 1
```

**Result:**
```
status = 0x86 (UNSUPPORTED_ATTRIBUTE)
```

FeatureMap (0xFFFC) was returning UNSUPPORTED_ATTRIBUTE, even though we had a handler for it.

### Root Cause Identified

The ESP32 serial logs revealed:
```
ERROR - Error processing attribute read: AttrStatus {
  path: AttrPath { endpoint: Some(1), cluster: Some(513), attr: Some(65532) },
  status: Status { status: UnsupportedAttribute }
}
```

The error was coming from rs-matter's attribute filtering, NOT our handler. rs-matter checks if an attribute is in the cluster's attribute array BEFORE calling the handler. Since global attributes (0xFFF8-0xFFFD) weren't in `THERMOSTAT_ATTRIBUTES`, they were rejected upstream.

## Solution

Added global attributes to `THERMOSTAT_ATTRIBUTES` in `thermos-esp32/src/matter/stack.rs`:

```rust
const THERMOSTAT_ATTRIBUTES: &[Attribute] = &[
    // ... existing thermostat attributes ...

    // --- Global Attributes (required by Matter spec) ---
    // GeneratedCommandList (0xFFF8)
    Attribute::new(0xFFF8, Access::RV, Quality::NONE),
    // AcceptedCommandList (0xFFF9)
    Attribute::new(0xFFF9, Access::RV, Quality::NONE),
    // AttributeList (0xFFFB)
    Attribute::new(0xFFFB, Access::RV, Quality::NONE),
    // FeatureMap (0xFFFC)
    Attribute::new(0xFFFC, Access::RV, Quality::NONE),
    // ClusterRevision (0xFFFD)
    Attribute::new(0xFFFD, Access::RV, Quality::F),
];
```

**Commit:** 53838bb

## Verification

After fix:
```bash
chip-tool any read-by-id 0x0201 0xFFFC 1 1
# Returns: FeatureMap: 3 (HEAT + COOL)
```

HomeKit now recognizes and controls the thermostat.

## Lessons Learned

1. **rs-matter requires explicit attribute listing**: Unlike some Matter stacks, rs-matter doesn't automatically include global attributes. They must be in the cluster's attribute array.

2. **UNSUPPORTED_ATTRIBUTE can come from filtering, not handlers**: When debugging attribute issues, check if the attribute is even reaching your handler code.

3. **Use `chip-tool any read-by-id` for explicit cluster targeting**: The standard `chip-tool thermostat read` command had issues targeting the correct cluster in our testing.

4. **ESP32 serial logs are essential**: The device-side error messages revealed the true source of the UNSUPPORTED_ATTRIBUTE response.

## Related Issues

- GitHub #38: Panic when adding 3rd fabric (still open)
- GitHub #39: Feature flag for cooling support (enhancement)

## Commits

1. 570f70d - feat(matter): add cooling type definitions and defaults
2. 7c9e12c - feat(matter): add cooling setpoint state management
3. 22a341d - feat(matter): add cooling support to ThermostatHooks trait
4. 7b2760e - feat(esp32): add cooling attribute handlers to Matter stack
5. 6183746 - feat(esp32): update cluster definition for HEAT+COOL support
6. **53838bb** - fix(matter): add global attributes to THERMOSTAT_ATTRIBUTES ← **Actual fix**
