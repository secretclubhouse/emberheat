# ESP32 Matter Thermostat - Design Document

**Date:** 2026-01-26  
**Status:** Approved  
**Author:** Claude + Josh

## Overview

Complete the emberheat project by building a production-ready 7-zone heating thermostat on ESP32-C6 with Matter smart home integration and MQTT state reporting.

## Goals

1. **ESP32 as primary platform** - Reliable multi-zone thermostat operation
2. **Matter integration** - Control via Apple Home, Google Home, Alexa
3. **MQTT reporting** - State logging for monitoring/Grafana
4. **Production stability** - Survive power cycles, run unattended

## Architecture

### Software Stack

```
┌─────────────────────────────────────────────────────┐
│                  Matter Controller                   │
│         (Apple Home / Google Home / Alexa)          │
└─────────────────────┬───────────────────────────────┘
                      │ Matter over WiFi
┌─────────────────────▼───────────────────────────────┐
│                 rs-matter-embassy                    │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  │
│  │ BLE Commis. │  │ WiFi Stack  │  │ NVS Storage │  │
│  │  (trouble)  │  │             │  │ (KvBlobStore)│  │
│  └─────────────┘  └─────────────┘  └─────────────┘  │
└─────────────────────┬───────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────┐
│              Thermostat Cluster (0x0201)             │
│  ┌──────────────────┐  ┌──────────────────────────┐ │
│  │ LocalTemperature │  │ OccupiedHeatingSetpoint  │ │
│  │ SystemMode       │  │ ThermostatRunningState   │ │
│  └──────────────────┘  └──────────────────────────┘ │
└─────────────────────┬───────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────┐
│                 emberheat-esp32                        │
│  ┌───────────┐  ┌───────────┐  ┌───────────────┐   │
│  │ DS18B20   │  │ GPIO      │  │ MQTT Logger   │   │
│  │ (RMT)     │  │ (Relay)   │  │               │   │
│  └───────────┘  └───────────┘  └───────────────┘   │
│                       │                             │
│            emberheat-core (hysteresis)                │
└─────────────────────────────────────────────────────┘
```

### Key Design Decisions

**Single ESP32-C6 for all 7 zones:**
- All wiring comes to a central point
- 1-Wire bus easily supports 7+ DS18B20 sensors
- 7 relay GPIOs well within ESP32-C6 capacity
- Single device in Matter fabric with 7 thermostat endpoints
- Simpler deployment than distributed controllers

**Matter library choice: rs-matter-embassy**
- Specifically designed for Embassy async runtime (already used by emberheat-esp32)
- Provides WiFi Matter stack with BLE commissioning
- Supports ESP32 chips with heap allocation strategy
- Repository: https://github.com/sysgrok/rs-matter-embassy

**Heating-only thermostat cluster:**
- Minimal attributes for hydronic heating use case
- Simpler implementation than full HVAC thermostat
- Matches physical system capabilities

### Memory Budget (ESP32-C6: 512KB SRAM)

| Component | Allocation |
|-----------|------------|
| rs-matter-embassy stack | ~100KB heap |
| Embassy runtime + tasks | ~50KB |
| WiFi/BLE buffers | ~80KB |
| Application code | ~30KB |
| **Headroom** | **~250KB** |

## Hardware

### Required Parts

| Part | Qty | Est. Cost | Notes |
|------|-----|-----------|-------|
| ESP32-C6 DevKit | 1 | $8-12 | ESP32-C6-DevKitC-1 recommended |
| DS18B20 sensors | 7 | $2-4 each | Waterproof probe version preferred |
| 4.7kΩ resistor | 1 | <$1 | Pull-up for 1-Wire bus |
| 8-channel relay module | 1 | $8-15 | 3.3V logic compatible, active-low |
| 5V power supply | 1 | $5-10 | 2A minimum for ESP32 + relays |

### Reused from Existing Pi Setup

- Zone valve relay wiring
- Potentially existing DS18B20 sensors
- Power infrastructure

### Optional Recommended

| Part | Qty | Notes |
|------|-----|-------|
| Level shifter (3.3V↔5V) | 1 | If relay module needs 5V logic |
| Screw terminal breakout | 1 | Cleaner wiring than dupont jumpers |
| Enclosure | 1 | Project box for tidy installation |

**Estimated total: $40-70** (assuming sensor/relay reuse)

### Future Hardware (Phase 5)

| Part | Qty | Est. Cost | Notes |
|------|-----|-----------|-------|
| ESP32-H2 DevKit | 1 | $8-10 | 802.15.4 radio for Thread support |

## Thermostat Cluster Implementation

### Cluster ID: 0x0201 (Thermostat)

**Attributes (heating-only subset):**

| Attribute | ID | Type | Access | Description |
|-----------|----|------|--------|-------------|
| LocalTemperature | 0x0000 | int16 | Read | Current temp (centi-°C) |
| OccupiedHeatingSetpoint | 0x0012 | int16 | Read/Write | Target temp (centi-°C) |
| SystemMode | 0x001C | enum8 | Read/Write | Off (0x00) or Heat (0x04) |
| ThermostatRunningState | 0x0029 | bitmap8 | Read | Bit 0 = heating active |
| ControlSequenceOfOperation | 0x001B | enum8 | Read | Heating only (0x02) |
| MinHeatSetpointLimit | 0x0015 | int16 | Read | Minimum setpoint |
| MaxHeatSetpointLimit | 0x0016 | int16 | Read | Maximum setpoint |

**Temperature Conversion:**
```rust
// Celsius to Matter centi-degrees
let centi_c = (temp_c * 100.0) as i16;
// Example: 21.5°C → 2150

// Matter centi-degrees to Celsius  
let temp_c = (centi_c as f32) / 100.0;
// Example: 2150 → 21.5°C
```

### Device Type

- Device Type ID: 0x0301 (Thermostat)
- 7 endpoints (one per zone), each with thermostat cluster

## Implementation Phases

### Phase 1: Multi-Zone Thermostat (No Matter)

**Goal:** Get 7-zone control working reliably before adding Matter.

**Tasks:**
- [ ] Refactor emberheat-esp32 for multi-zone support
- [ ] Configure 7 DS18B20 sensors on shared 1-Wire bus  
- [ ] Configure 7 relay GPIO outputs
- [ ] Independent hysteresis control loop per zone
- [ ] MQTT publishes state for all 7 zones
- [ ] NVS persists target temps for all zones
- [ ] Zone configuration (sensor ID → relay pin mapping)

**Validation:**
- Run alongside Pi system
- Compare relay behavior between systems
- Verify MQTT messages match expected state

### Phase 2: Thermostat Cluster Implementation

**Goal:** Build heating-only thermostat cluster for rs-matter.

**Tasks:**
- [ ] Study rs-matter cluster implementation patterns
- [ ] Implement Thermostat cluster (0x0201) with heating-only attributes
- [ ] Temperature conversion utilities (Celsius ↔ centi-degrees)
- [ ] Unit tests for attribute read/write
- [ ] Unit tests for mode changes
- [ ] Single-zone test harness (host-side, no ESP32)

**Validation:**
- Cluster compiles
- All unit tests pass
- Attribute behavior matches Matter spec

### Phase 3: Matter Integration on ESP32

**Goal:** Wire up rs-matter-embassy with thermostat cluster.

**Tasks:**
- [ ] Add rs-matter-embassy dependency
- [ ] Configure WiFi Matter stack
- [ ] Configure BLE commissioning (trouble library)
- [ ] Create 7 endpoints with thermostat cluster each
- [ ] Bridge: Matter setpoint → emberheat-core target_temp
- [ ] Bridge: emberheat-core state → Matter attributes
- [ ] Handle concurrent Matter + MQTT operation

**Validation:**
- Device commissions via BLE
- Appears in Apple Home with 7 thermostats
- Setpoint changes from Home app reach emberheat-core
- Temperature readings appear in Home app

### Phase 4: End-to-End Testing & Hardening

**Goal:** Production readiness.

**Tasks:**
- [ ] Test with Apple Home app
- [ ] Test with Google Home app
- [ ] Test with Alexa
- [ ] Verify MQTT continues working alongside Matter
- [ ] Power cycle testing (NVS state recovery)
- [ ] Run parallel with Pi system for 1-2 weeks
- [ ] Document BLE commissioning process
- [ ] Document MQTT topic structure
- [ ] Error handling and recovery
- [ ] Watchdog timer configuration

**Validation:**
- "Hey Siri, set Zone 1 to 68 degrees" works
- All 3 ecosystems can control all 7 zones
- System recovers from power loss
- No intervention needed for 2+ weeks

### Phase 5 (Future): Thread Support via ESP32-H2

**Goal:** Add Thread networking as alternative to WiFi.

**Why Thread:**
- Matter-over-Thread is more reliable (mesh network)
- No WiFi dependency for smart home control
- Lower power consumption
- Apple/Google preferred transport

**Architecture Options:**

A) **ESP32-H2 as Thread Border Router**
   - C6 stays as main controller on WiFi
   - H2 provides Thread connectivity to Matter fabric
   - Other Thread devices can join mesh

B) **ESP32-H2 replaces C6**
   - Run thermostat on H2 with Matter-over-Thread
   - Limitation: H2 has less RAM than C6

C) **Hybrid: C6 + H2 co-processor**
   - C6 does heavy lifting
   - H2 acts as Thread radio

**Tasks (future):**
- [ ] Evaluate rs-matter-embassy Thread support maturity
- [ ] Select architecture option
- [ ] Hardware integration
- [ ] Testing with Thread border router

## Integration Points

### Matter → EmberHeat

When HomeKit/Google sends a setpoint change:
1. rs-matter receives attribute write for `OccupiedHeatingSetpoint`
2. Convert centi-degrees to Celsius
3. Update emberheat-core `target_temp` for zone
4. Trigger hysteresis recalculation
5. Update relay if needed
6. Persist to NVS
7. Publish to MQTT

### EmberHeat → Matter

When temperature changes or relay toggles:
1. Sensor read updates emberheat-core `current_temp`
2. Hysteresis may toggle relay
3. Convert Celsius to centi-degrees
4. Update Matter `LocalTemperature` attribute
5. Update Matter `ThermostatRunningState` if relay changed
6. Matter notifies subscribed controllers
7. Publish to MQTT

### MQTT Topics (existing pattern)

```
emberheat/zone1/temperature  → 21.5
emberheat/zone1/target       → 22.0
emberheat/zone1/heating      → on|off
emberheat/zone1/mode         → heat|off
```

## Testing Instructions

### Prerequisites

1. Install Rust ESP32 toolchain:
   ```bash
   rustup target add riscv32imac-unknown-none-elf
   cargo install espflash
   ```

2. Connect ESP32-C6 dev board via USB

### Step 1: Test with Mock Sensors

This validates the 7-zone control logic without requiring real sensors.

1. Enable mock mode in `emberheat-esp32/src/config.rs`:
   ```rust
   pub const USE_MOCK_SENSORS: bool = true;
   ```

2. Build and flash:
   ```bash
   cd emberheat-esp32
   WIFI_SSID="your_network" WIFI_PASSWORD="your_password" \
     cargo build --release --target riscv32imac-unknown-none-elf
   espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32
   ```

3. Expected serial output:
   ```
   [INFO] EmberHeat ESP32 Multi-Zone Thermostat starting...
   [INFO] Initializing 7 zones with mock sensors
   [INFO] [Zone1] Starting control loop
   [INFO] [Zone2] Starting control loop
   ...
   [DEBUG] [Zone1] temp=20.0°C target=21.1°C heating=true
   ```

4. Verify:
   - All 7 zones start
   - Temperature readings appear (mock value 20.0°C)
   - Heating state follows hysteresis logic (ON since 20.0 < 21.1 - 0.5)

### Step 2: Discover Real Sensor Addresses

1. Wire DS18B20 sensors to GPIO 4 (1-Wire bus) with 4.7kΩ pull-up

2. Flash the sensor scan utility:
   ```bash
   WIFI_SSID="test" WIFI_PASSWORD="test" \
     cargo build --release --bin scan_sensors --target riscv32imac-unknown-none-elf
   espflash flash --monitor target/riscv32imac-unknown-none-elf/release/scan_sensors
   ```

3. Note the discovered addresses printed in the format:
   ```
   Found sensor: [0x28, 0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78]
   ```

4. Update `ZONE_CONFIGS` in `config.rs` with real addresses

### Step 3: Test with Real Sensors

1. Disable mock mode:
   ```rust
   pub const USE_MOCK_SENSORS: bool = false;
   ```

2. Wire relays to GPIO pins 5, 6, 7, 15, 16, 17, 18

3. Build and flash main application (same as Step 1)

4. Verify:
   - Real temperature readings appear
   - Relays toggle based on temperature vs target
   - System runs stable for extended period

### Step 4: Parallel Testing with Pi

1. Run ESP32 thermostat alongside existing Pi system
2. Compare relay behavior between systems
3. Verify temperatures match
4. Run for 1-2 weeks before switching over

### Troubleshooting

**"No sensors found" during scan:**
- Check wiring: GPIO 4 → Data, 3.3V → VCC, GND → GND
- Verify 4.7kΩ pull-up resistor between Data and VCC
- Try shorter wires

**Build fails with esp-hal errors:**
- The `esp-hal-rmt-onewire` crate has compatibility issues
- Use mock sensors until dependency is updated
- Check `emberheat-esp32/Cargo.toml` for `onewire-rmt` feature

**Relay not switching:**
- Relays are active-low (GPIO LOW = relay ON)
- Check relay module is 3.3V logic compatible
- Verify GPIO pin matches config

## Success Criteria

1. **Voice control works:** "Hey Siri, set the living room to 70 degrees"
2. **All ecosystems supported:** Apple Home, Google Home, Alexa
3. **7 zones visible:** Each zone appears as separate thermostat
4. **Reliable operation:** Runs unattended for weeks
5. **State persistence:** Survives power cycles
6. **MQTT logging:** Grafana dashboards continue working
7. **Parallel operation:** Can run alongside Pi during transition

## Risks & Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| rs-matter-embassy immaturity | May hit bugs | Keep Pi as fallback, contribute fixes upstream |
| 7 endpoints memory pressure | Stack overflow | Profile early, reduce if needed |
| Matter certification | Can't sell commercially | Personal use only, not a concern |
| BLE commissioning issues | Can't pair device | Test early with multiple controllers |

## References

- [rs-matter-embassy](https://github.com/sysgrok/rs-matter-embassy) - Embassy integration
- [rs-matter-stack](https://github.com/sysgrok/rs-matter-stack) - Stack assembly
- [rs-matter](https://github.com/project-chip/rs-matter) - Core Matter library
- [Matter Thermostat Cluster Spec](https://github.com/project-chip/connectedhomeip/blob/master/data_model/1.5/clusters/Thermostat.xml)
- [ESP32-C6 Datasheet](https://www.espressif.com/en/products/socs/esp32-c6)

## Phase Summary

| Phase | Focus | Outcome |
|-------|-------|---------|
| 1 | Multi-zone thermostat | 7 zones working, MQTT reporting |
| 2 | Thermostat cluster | Matter cluster implementation |
| 3 | Matter integration | HomeKit/Google/Alexa control |
| 4 | Testing & hardening | Production-ready |
| 5 | Thread (future) | ESP32-H2 for Thread mesh |
