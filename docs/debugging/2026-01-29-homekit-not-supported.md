# HomeKit "Not Supported" Issue - Debugging Report

**Date Range:** 2026-01-27 to 2026-01-29
**Status:** Resolved
**Root Cause:** Missing global attributes in THERMOSTAT_ATTRIBUTES array

## Summary

After commissioning a Matter thermostat (device type 0x0301) to Apple HomeKit, the device showed "Not Supported" despite successful commissioning and correct Matter protocol implementation.

## Symptoms

- Device commissioned successfully to HomeKit via BLE+WiFi
- HomeKit displayed "Not Supported" message
- No thermostat controls visible in Home app
- Device worked correctly with Home Assistant

## Timeline of Investigation

### Day 1 (Jan 27): Initial Matter Integration

Started integrating rs-matter-embassy into the ESP32-C6 thermostat firmware. Initial commits:
- Added rs-matter-embassy dependencies
- Created ZoneThermostatCluster handler skeleton
- Implemented Handler trait for thermostat attributes
- Added matter_task entry point with WiFi+BLE coexistence

**Issue discovered:** Single-radio time-sharing between WiFi and BLE caused unreliable commissioning on ESP32-C6.

### Day 2 (Jan 28): Dual-Radio Architecture

Implemented dual-radio solution using ESP32-H2 as BLE co-processor:

1. **Added ESP32-H2 HCI UART support:**
   - Flashed ESP32-H2 with NimBLE HCI firmware
   - Created DualRadioDriver for external BLE over UART
   - Wired ESP32-C6 (WiFi) to ESP32-H2 (BLE) via UART

2. **Encountered UART issues:**
   - At 921600 baud, RX FIFO overflow during BLE commissioning
   - **Fix:** Lowered to 115200 baud (commit 45c0788)
   - Added hardware flow control (RTS/CTS)

3. **Fixed ESP32-H2 configuration:**
   - Wrong example initially (`controller_hci_uart_esp32c3_and_esp32s3`)
   - **Fix:** Used `nimble/hci` example which supports H2 (commit 2a4e003)
   - Fixed pin assignments (H2 has no GPIO6/7)

**Result:** Commissioning now worked reliably. Home Assistant could control the thermostat. But HomeKit still showed "Not Supported."

### Day 3 (Jan 29): HomeKit-Specific Debugging

#### Attempt 1: Temperature Sync Issues

Noticed LocalTemperature was stuck at NULL_TEMPERATURE (0x8000).

**Fixes applied:**
- Sync temperature readings to Matter state after each sensor read (commit e09a044)
- Sync initial temperature before Matter task starts (commit fdc105d)

**Result:** Temperature now visible in Home Assistant. HomeKit still "Not Supported."

#### Attempt 2: Multi-Fabric Panic

Observed panic when adding 3rd fabric (chip-tool after HomeKit's 2 fabrics):
```
panicked at rs-matter: No space
```

**Fix:** Enabled max-fabrics-5 feature in rs-matter (commit 5c4ff24)

**Result:** Could now add more fabrics. HomeKit still "Not Supported."

#### Attempt 3: Add Cooling Support

Hypothesis: Apple might require HEAT+COOL support like HAP did.

**Changes made (commits 570f70d through 6183746):**
- Added cooling type definitions and defaults
- Added cooling setpoint state management
- Updated ThermostatHooks trait with cooling methods
- Added cooling attribute handlers
- Changed FeatureMap from 0x01 (HEAT) to 0x03 (HEAT+COOL)
- Changed ControlSequenceOfOperation to CoolingAndHeating

**Result:** HomeKit still "Not Supported."

#### Attempt 4: Direct Attribute Testing with chip-tool

Used chip-tool to test attribute reads directly:

```bash
# First attempt - wrong cluster!
chip-tool thermostat read feature-map 1 1
# Returned cluster 0x001D (Descriptor), not 0x0201 (Thermostat)

# Explicit cluster targeting
chip-tool any read-by-id 0x0201 0xFFFC 1 1
```

**Result:**
```
status = 0x86 (UNSUPPORTED_ATTRIBUTE)
```

FeatureMap was returning UNSUPPORTED_ATTRIBUTE even though we had a handler.

#### Root Cause Identified

ESP32 serial logs revealed:
```
ERROR - Error processing attribute read: AttrStatus {
  path: AttrPath { endpoint: Some(1), cluster: Some(513), attr: Some(65532) },
  status: Status { status: UnsupportedAttribute }
}
```

The error came from rs-matter's attribute filtering, NOT our handler. rs-matter checks if an attribute is in the cluster's attribute array BEFORE calling the handler. Global attributes (0xFFF8-0xFFFD) weren't in `THERMOSTAT_ATTRIBUTES`, so they were rejected upstream.

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

3. **Use `chip-tool any read-by-id` for explicit cluster targeting**: The standard `chip-tool thermostat read` command had issues targeting the correct cluster.

4. **ESP32 serial logs are essential**: The device-side error messages revealed the true source of the UNSUPPORTED_ATTRIBUTE response.

5. **Dual-radio can solve WiFi+BLE coexistence issues**: ESP32-C6's single radio had time-sharing issues. Adding ESP32-H2 for BLE made commissioning reliable.

6. **Temperature must be valid during commissioning**: NULL_TEMPERATURE can cause ecosystems to mark devices as unsupported.

## Related Issues

- GitHub #38: Panic when adding 3rd fabric (still open, likely session/exchange buffer exhaustion)
- GitHub #39: Feature flag for cooling support (enhancement for future)

## Key Commits

### Day 1: Initial Integration
- a3c346b - docs: add Phase 3 Matter integration design
- 5fb6d19 - feat(matter): add rs-matter-embassy dependencies
- c2c4afc - feat(matter): implement Handler trait for ZoneThermostatCluster
- e0b8188 - feat(matter): add matter_task entry point with WiFi+BLE coexistence

### Day 2: Dual-Radio Setup
- 2cd5d79 - feat(matter): add bt-hci dependency for external BLE controller
- 5b34304 - feat(matter): add DualRadioDriver for external BLE over UART
- 4a18cc2 - feat(matter): add matter_task_dual_radio for external BLE
- 45c0788 - fix(matter): use 115200 baud for HCI UART to prevent FIFO overflow
- 6dbd4f0 - feat(matter): enable hardware flow control for HCI UART

### Day 3: HomeKit Fixes
- e09a044 - fix(matter): sync temperature readings to Matter state
- fdc105d - fix(matter): sync initial temperature before Matter task starts
- 5c4ff24 - fix(matter): increase MAX_FABRICS to 5 for multi-ecosystem support
- 570f70d through 6183746 - feat: add COOL support (6 commits)
- **53838bb** - fix(matter): add global attributes to THERMOSTAT_ATTRIBUTES ← **Actual fix**
