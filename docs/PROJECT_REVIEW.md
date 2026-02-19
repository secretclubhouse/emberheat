# EmberHeat Project Review

**Date:** 2026-02-18
**Scope:** Full codebase review across all crates

---

## 1. Project Goals

EmberHeat is a multi-zone thermostat system for hydronic (hot water) heating. The primary goals are:

1. **Multi-zone temperature control** — Independently control 1-16 heating zones with hysteresis-based relay switching
2. **ESP32-C6 bare-metal firmware** — Run on embedded hardware with no_std Rust using Embassy async runtime
3. **Matter smart home integration** — Expose each zone as a Matter Thermostat device for HomeKit, Google Home, and Alexa
4. **MQTT telemetry** — Publish temperature readings and events for monitoring and logging
5. **Persistent state** — Survive reboots by persisting setpoints and modes to flash storage
6. **Safety-first design** — Default to heating OFF on any error, validate all sensor readings, prevent relay chatter

---

## 2. Architecture Summary

```
┌─────────────────────────────────────────────────────┐
│                   emberheat-esp32                    │
│  ┌──────────┐  ┌──────────┐  ┌───────────────────┐  │
│  │ Sensors  │  │  GPIO    │  │  Matter Stack     │  │
│  │ (1-Wire, │  │ (Relays) │  │  (rs-matter)      │  │
│  │  BME280, │  │          │  │                   │  │
│  │  Mock)   │  │          │  │  ┌─────────────┐  │  │
│  └────┬─────┘  └────┬─────┘  │  │ MQTT Bridge │  │  │
│       │              │        │  └─────────────┘  │  │
│       └──────┬───────┘        └────────┬──────────┘  │
│              │                         │             │
│         ┌────▼─────────────────────────▼────┐        │
│         │       Zone Control Loop (6s)      │        │
│         │  emberheat-core hysteresis algo   │        │
│         └────────────┬──────────────────────┘        │
│                      │                               │
│              ┌───────▼───────┐                       │
│              │  Flash NVS    │                       │
│              │  Persistence  │                       │
│              └───────────────┘                       │
├─────────────────────────────────────────────────────┤
│  emberheat-core (no_std)  │  emberheat-matter       │
│  - Hysteresis control     │  - Thermostat cluster   │
│  - Sensor/GPIO/Storage    │  - Atomic zone state    │
│  - Type definitions       │  - Handler framework    │
│  - Temperature validation │  - Endpoint mapping     │
└───────────────────────────┴─────────────────────────┘
```

---

## 3. Crate-by-Crate Status

### 3.1 emberheat-core — COMPLETE

The platform-agnostic core library is production-ready.

| Component | Status | Notes |
|-----------|--------|-------|
| Hysteresis control algorithm | Complete | ±0.5°C dead band, well-tested |
| Temperature types & validation | Complete | 3-37°C valid range |
| TemperatureSensor trait | Complete | Async, no_std compatible |
| OutputPin / GpioManager traits | Complete | Active-low relay logic |
| StateStorage trait | Complete | Async persistence abstraction |
| ZoneConfig / HysteresisConfig | Complete | Serializable configuration types |
| Error types | Complete | Domain-specific, no_std compatible |
| **Test coverage** | **32 tests** | All passing, good edge case coverage |

**No TODOs or FIXMEs.**

### 3.2 emberheat-matter — COMPLETE

The Matter protocol integration layer is production-ready.

| Component | Status | Notes |
|-----------|--------|-------|
| Thermostat cluster (0x0201) | Complete | Both heating-only and heating+cooling variants |
| ZoneThermostatState | Complete | Thread-safe atomic state container |
| ThermostatHooks trait | Complete | Getters, setters, background task |
| ThermostatHandler | Complete | Out-of-band notifications, data versioning |
| DeviceInfo / endpoint mapping | Complete | Zone index ↔ Matter endpoint (1-16) |
| Temperature conversions | Complete | Celsius ↔ Matter centi-degrees |
| SystemMode / RunningState | Complete | Full enum support with validation |
| Setpoint validation | Complete | Heating 7-30°C, Cooling 10-35°C |
| **Test coverage** | **42 tests** | Comprehensive, including edge cases |

**No TODOs or FIXMEs.**

### 3.3 emberheat-esp32 — MOSTLY COMPLETE (MVP gaps remain)

The ESP32-C6 firmware has a solid architecture but has key gaps for real-world deployment.

| Component | Status | Notes |
|-----------|--------|-------|
| Embassy task architecture | Complete | Static zone tasks (up to 7 compiled) |
| Mock sensor | Complete | Works for development and testing |
| GPIO relay control | Complete | esp-hal wrapper, active-low |
| WiFi connectivity | Complete | esp-radio, DHCP |
| MQTT publishing | Complete | JSON payloads, exponential backoff retry |
| Flash persistence | Complete | sequential-storage, wear-leveled |
| Matter stack integration | Complete | Commissioning, subscriptions, attributes |
| Matter+MQTT dual mode | Complete | MQTT runs as UserTask inside Matter |
| DS18B20 1-Wire sensor | **Blocked** | Code written but `esp-hal-rmt-onewire` has compatibility issues |
| BME280 I2C sensor | **Partial** | Simplified calibration, untested on hardware |
| SharedOneWireBus | **Placeholder** | Scan and read methods are stubs with TODOs |
| Zone configuration | **Hardcoded** | Static compile-time config in `config.rs` |
| Sensor addresses | **Placeholders** | Need real ROM addresses from hardware scan |
| NUM_ZONES | **Set to 1** | Reduced for Matter commissioning testing |
| Test DAC credentials | **Test only** | Using test vendor ID (0xFFF1) and test passcode |
| Dual-radio (H2 BLE) | **Untested** | Implemented but not tested with real hardware |

**TODOs found:**
- `sensors.rs:108` — SharedOneWireBus::scan() placeholder
- `sensors.rs:138` — SharedOneWireBus::read_temperature() placeholder

### 3.4 emberheat-linux — PRESERVED (not active)

The Raspberry Pi implementation is feature-complete for its original scope but is not being actively developed. It serves as a reference implementation.

| Component | Status | Notes |
|-----------|--------|-------|
| JSON config file loading | Complete | Multi-path search, validation |
| Redis persistence | Complete | State manager with connection pooling |
| rppal GPIO | Complete | Active-low relays, VCC power cycling |
| 1-Wire via sysfs | Complete | DS18B20 with CRC validation |
| HTTP fallback sensor | Complete | JSON endpoint for extra sensors |
| Composite sensor | Complete | Primary + fallback |
| Prometheus metrics | Complete | Temperature, heat status, response time |
| Control loop | Complete | 6s interval, error recovery |
| Matter integration | **Stub** | Removed; docs describe re-integration path |

---

## 4. What's Complete (MVP Checklist)

- [x] Core hysteresis control algorithm with configurable dead band
- [x] Platform-agnostic trait abstractions (sensor, GPIO, storage)
- [x] ESP32-C6 Embassy async runtime integration
- [x] Matter Thermostat cluster implementation (heat + cool)
- [x] Thread-safe atomic zone state management
- [x] Flash persistence with wear-leveling (sequential-storage)
- [x] WiFi connectivity
- [x] MQTT telemetry publishing
- [x] Matter commissioning (BLE + WiFi)
- [x] CI/CD pipeline (format, lint, test, security audit, coverage)
- [x] Pre-commit hooks
- [x] Comprehensive documentation

## 5. What Still Needs to Be Done (MVP Gaps)

### 5.1 Hardware Sensor Support (Critical)

The system currently runs with mock sensors only. Real deployment requires:

1. **Resolve `esp-hal-rmt-onewire` compatibility** — The DS18B20 1-Wire sensor code is written but blocked on a dependency compatibility issue with esp-hal 1.0. This is the single biggest blocker for real-world use.

2. **Implement SharedOneWireBus** — The scan and read methods are stubs. Multiple DS18B20 sensors share a single 1-Wire bus, so the shared bus implementation is essential for multi-zone operation.

3. **Validate BME280 I2C sensor** — The implementation has simplified calibration. Needs hardware testing to confirm accuracy.

4. **Replace placeholder sensor addresses** — The 7-zone config uses `[0x28, 0, 0, 0, 0, 0, 0, N]` placeholders. Real ROM addresses must be discovered via bus scan.

### 5.2 Zone Configuration

Currently, zone configuration is entirely compile-time in `config.rs`. This means:
- Adding/removing zones requires recompilation
- Sensor ROM addresses are hardcoded
- Zone names are hardcoded
- No runtime configuration possible

### 5.3 Production Credentials

- Replace test DAC (Device Attestation Certificate)
- Replace test vendor ID (0xFFF1) with real vendor ID
- Replace test passcode (20202021)
- Generate unique discriminators per device

### 5.4 Re-enable Multi-Zone

`NUM_ZONES` is currently set to 1 for testing. Need to restore the 7-zone (or desired) configuration once sensor support is working.

---

## 6. Feature Proposals

### 6.1 Runtime Zone Configuration (User Request #1)

**Problem:** Zones are configured at compile time. Changing zone names, sensor assignments, or the number of active zones requires editing `config.rs` and recompiling.

**Proposed Solution: NVS-backed runtime configuration**

Since the ESP32 is a no_std embedded target, a traditional config file (JSON/TOML on a filesystem) isn't practical. Instead, we can store zone configuration in the same NVS flash partition already used for state persistence.

**Design:**
```
┌─────────────────────────────────────────┐
│           NVS Flash Layout              │
│                                         │
│  0x0000-0x0FFF  Matter fabric data      │
│  0x1000-0x10FF  Zone state (setpoints)  │
│  0x2000-0x20FF  Zone config             │
│    - Zone count                         │
│    - Per-zone: name, relay_pin,         │
│      sensor_address, enabled flag       │
│  0x3000+        Reserved                │
└─────────────────────────────────────────┘
```

**Configuration would include per zone:**
- Zone name (up to 32 chars)
- Relay GPIO pin number
- Sensor ROM address (8 bytes for DS18B20)
- Enabled/disabled flag
- Default target temperature

**How to configure:**
- **Option A: Matter custom cluster** — Add a vendor-specific cluster for configuration. Complex but fully integrated.
- **Option B: MQTT commands** — Subscribe to a config topic and accept JSON config updates. Simpler to implement.
- **Option C: USB serial console** — Accept configuration commands over the USB-C serial connection during setup. No network needed.
- **Option D: Initial compile-time defaults + runtime override** — Ship with sensible compile-time defaults, allow runtime changes via MQTT or serial that persist to NVS.

**Recommendation:** Option D is the most practical. Keep compile-time defaults as a fallback, but allow runtime overrides stored in NVS. Use MQTT commands for configuration since the MQTT infrastructure already exists. A serial console could be added later for initial setup without network.

### 6.2 Auto-Discovery and Sensor Assignment (User Request #2)

**Problem:** Sensor ROM addresses must be known at compile time and manually assigned to zones.

**Proposed Solution: 1-Wire bus scan with persistent assignment**

**Startup Flow:**
```
Boot
  │
  ▼
Load saved sensor config from NVS
  │
  ├─ Config exists and all sensors found → Use saved config
  │
  ├─ Config exists but some sensors missing → Log warning, disable those zones
  │
  └─ No config exists → Run discovery mode
                            │
                            ▼
                     Scan 1-Wire bus
                     Find all DS18B20 sensors
                            │
                            ▼
                     Auto-assign: sensor[0] → zone[0], etc.
                     Save assignment to NVS
                            │
                            ▼
                     Start normal operation
```

**Key considerations:**
- DS18B20 ROM addresses are globally unique and stable — they're burned into each chip
- A scan discovers all sensors on the bus and returns their ROM addresses
- Auto-assignment is sequential: first sensor found → first zone, etc.
- The assignment persists to NVS so it survives reboots
- If a sensor disappears (wire breaks, sensor fails), that zone is disabled with a warning
- New sensors can be detected by triggering a re-scan (via MQTT command or button press)
- The user can rearrange assignments via MQTT or serial after auto-discovery

**Implementation steps:**
1. Implement `SharedOneWireBus::scan()` (currently a TODO stub)
2. Add NVS key range for sensor-zone mapping
3. Add startup logic to load-or-discover sensors
4. Add MQTT command to trigger re-scan
5. Add logging/MQTT notification of discovered sensors

**Constraint:** This requires the 1-Wire bus to be working first (see MVP gap 5.1).

### 6.3 Additional Future Features

These are features that would enhance the system beyond MVP but are not currently planned:

#### Scheduling & Programming
- Time-based setpoint schedules (e.g., lower temp at night, warm up before waking)
- Requires an RTC or NTP time sync
- Could use Matter Time Synchronization cluster

#### OTA Firmware Updates
- Over-the-air firmware updates via WiFi
- esp-hal supports this via the `esp-ota` crate
- Critical for deployed devices that are hard to physically access

#### Temperature History & Logging
- Store recent temperature history in flash or stream to MQTT
- Useful for debugging and optimizing heating schedules

#### Outdoor Temperature Compensation
- Additional outdoor sensor to adjust heating curves
- More aggressive heating when it's very cold outside

#### Multi-Sensor Averaging
- Support multiple sensors per zone (e.g., different rooms in same zone)
- Average or use min/max for control decisions

#### PID Control
- Replace simple hysteresis with PID controller for smoother temperature control
- Better for radiant/hydronic systems with slow response times
- Could be offered as an alternative control mode

#### Web Dashboard
- Simple web server on the ESP32 for status and configuration
- esp-hal supports basic HTTP serving
- Could replace MQTT-based configuration

#### Vacation / Away Mode
- Global mode to set all zones to a minimum safe temperature
- Could be triggered via Matter, MQTT, or physical button

#### Energy Usage Tracking
- Track relay on-time per zone per day
- Estimate energy usage based on boiler capacity
- Report via MQTT or Matter

#### Alerts & Notifications
- Sensor failure alerts
- Heating stuck on/off alerts
- Temperature out of expected range alerts
- Could use MQTT or Matter events

#### Physical Button / LED Interface
- Factory reset button (partially implemented)
- Status LED for WiFi/Matter/heating state
- Mode toggle button

#### Zone Grouping
- Group multiple zones with shared schedules or master/slave setpoints
- "Whole house" mode

---

## 7. Technical Debt & Minor Issues

| Issue | Location | Severity |
|-------|----------|----------|
| Unused `alloc::boxed::Box` import | `emberheat-core/src/sensor.rs:14` | Low — cosmetic |
| Hardcoded DHCP random seed (1234) | `emberheat-esp32/src/wifi.rs` | Low — should use hardware RNG |
| `USE_MOCK_SENSORS = true` hardcoded | `emberheat-esp32/src/config.rs` | Medium — needs feature flag or runtime config |
| Test vendor/product IDs in production code | `emberheat-esp32/src/config.rs` | Medium — fine for development, blocks production |
| Embassy zone tasks hardcoded to 7 | `emberheat-esp32/src/main.rs` | Low — architectural constraint of Embassy |
| Temperature valid range (3-37°C) may be too narrow | `emberheat-core/src/types.rs` | Low — depends on deployment climate |
| No integration tests for ESP32 | `emberheat-esp32/` | Medium — difficult for embedded |

---

## 8. Recommendations

### Immediate Priority (MVP Completion)
1. Resolve 1-Wire sensor dependency and get real hardware sensors working
2. Implement SharedOneWireBus scan and read
3. Re-enable multi-zone configuration with real sensor addresses
4. Hardware test the full control loop with real relays and sensors

### Short-Term (Post-MVP)
5. Add runtime zone configuration via NVS with MQTT commands
6. Add sensor auto-discovery and persistent assignment
7. Add OTA update support
8. Replace test DAC/vendor credentials

### Medium-Term (Enhancement)
9. Add scheduling/programming support
10. Add temperature history logging
11. Add alerts for sensor failures and anomalies
12. Add a web dashboard for configuration

### Long-Term (Nice to Have)
13. PID control mode
14. Outdoor temperature compensation
15. Energy usage tracking
16. Zone grouping
