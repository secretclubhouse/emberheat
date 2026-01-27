# Matter Protocol Integration Guide

This document describes how to add Matter/HomeKit support to thermos-rust using the modern **rs-matter** pure Rust implementation.

## Overview

Matter (formerly Project CHIP) is an open-source connectivity standard for smart home devices. It enables integration with Apple HomeKit, Google Home, Amazon Alexa, and other ecosystems.

## Current Status

> ⚠️ **Matter is NOT functional.** The current codebase contains only a stub implementation.

**Raspberry Pi (`thermos-linux` / legacy `src/`):** The `src/matter/mod.rs` contains a **stub implementation** that logs warnings and waits indefinitely. It does not provide any actual Matter functionality.

**ESP32 (`thermos-esp32`):** Matter integration is **planned** using the `rs-matter` pure Rust library, which provides native support for Embassy async executor and bare-metal ESP32 hardware. This is not yet implemented.

## Recommended Approach: rs-matter

### Why rs-matter Instead of Connected Home IP?

The previous Matter integration attempt used C++ FFI bindings to the Connected Home IP SDK, which proved complex and difficult to maintain. As of 2025, **rs-matter** provides a superior alternative:

| Feature | rs-matter (Modern) | Connected Home IP (Legacy) |
|---------|-------------------|---------------------------|
| **Language** | Pure Rust | C++ with FFI bindings |
| **Build complexity** | Cargo dependency | ~300MB SDK, GLib, C++ compiler |
| **Embedded support** | `no_std` compatible | Linux-focused |
| **Async runtime** | Executor agnostic (Embassy ✅) | Complex integration |
| **ESP32 support** | First-class with examples | Requires ESP-IDF wrapper |
| **Maintenance** | Active development, on crates.io | Complex build scripts |
| **Dependencies** | Minimal | GLib, pkg-config, pthread, stdc++ |

**Conclusion:** Use `rs-matter` for new implementations. The C++ FFI approach documented in previous versions of this file is **deprecated**.

## Integration Timeline

Matter integration should be implemented **after** the ESP32 MVP is functional:

```
✅ Week 1: Platform-agnostic core extraction (COMPLETE)
→ Week 2: ESP32 platform layer (GPIO, sensors, NVS storage, WiFi)
→ Week 3: MQTT integration for temperature history
→ Week 4: Testing and refinement on ESP32 hardware
→ Week 5-6: Add rs-matter integration ← Matter implementation
```

## ESP32 Matter Integration (Weeks 5-6)

### Prerequisites

- ESP32-C6 hardware (recommended - has Thread radio for Matter-over-Thread)
- Working `thermos-esp32` binary with:
  - Embassy executor running
  - WiFi connectivity established
  - Temperature sensor reading
  - GPIO relay control
  - NVS state persistence
  - Basic MQTT logging (optional, can coexist)

### Step 1: Add rs-matter Dependency

Update `thermos-esp32/Cargo.toml`:

```toml
[dependencies]
thermos-core = { path = "../thermos-core" }
embassy-executor = { workspace = true }
embassy-net = "0.5"
embassy-time = { workspace = true }
esp-hal = "1.0"
esp-wifi = "0.11"
esp-backtrace = "0.14"

# Matter support (optional feature)
rs-matter = { version = "0.1", optional = true }

# MQTT support (can coexist with Matter)
rust-mqtt = { version = "0.3", optional = true }

[features]
default = ["mqtt"]
matter = ["rs-matter"]
full = ["mqtt", "matter"]  # Both protocols
```

### Step 2: Matter Device Configuration

Create `thermos-esp32/src/matter_device.rs`:

```rust
use rs_matter::data_model::cluster_basic_information::BasicInfoConfig;
use rs_matter::data_model::cluster_on_off;
use rs_matter::data_model::device_types::DEV_TYPE_ON_OFF_LIGHT;
use rs_matter::data_model::objects::{Endpoint, Node};
use rs_matter::data_model::system_model::descriptor;
use rs_matter::Matter;
use rs_matter::MATTER_PORT;

use thermos_core::{HeatingState, TemperatureReading, ThermostatState};

/// Matter Thermostat Cluster ID
const CLUSTER_THERMOSTAT: u32 = 0x0201;

/// Thermostat attributes
const ATTR_LOCAL_TEMPERATURE: u16 = 0x0000;        // int16 (centi-°C)
const ATTR_OCCUPIED_HEATING_SETPOINT: u16 = 0x0012; // int16 (centi-°C)
const ATTR_SYSTEM_MODE: u16 = 0x001C;               // enum8
const ATTR_THERMOSTAT_RUNNING_STATE: u16 = 0x0029;  // bitmap8

/// System mode values
const MODE_OFF: u8 = 0x00;
const MODE_HEAT: u8 = 0x04;

/// Running state bitmap
const RUNNING_STATE_IDLE: u8 = 0x00;
const RUNNING_STATE_HEATING: u8 = 0x01;

pub struct MatterThermostat {
    endpoint_id: u16,
    state: ThermostatState,
}

impl MatterThermostat {
    pub fn new(endpoint_id: u16, zone_name: String) -> Self {
        Self {
            endpoint_id,
            state: ThermostatState::default(),
        }
    }

    /// Update local temperature reading
    pub fn update_temperature(&mut self, reading: TemperatureReading) {
        self.state.current_temp = reading.celsius;
        // Convert to centi-degrees for Matter
        let centi_c = (reading.celsius * 100.0) as i16;
        // Update Matter attribute (implementation depends on rs-matter API)
    }

    /// Update target setpoint (called from HomeKit/Google Home)
    pub fn set_target_temperature(&mut self, centi_c: i16) {
        let temp_c = (centi_c as f32) / 100.0;
        self.state.target_temp = temp_c;
    }

    /// Update heating state
    pub fn update_heating_state(&mut self, heating: HeatingState) {
        self.state.target_state = heating;
        let running_state = match heating {
            HeatingState::Heat => RUNNING_STATE_HEATING,
            HeatingState::Off => RUNNING_STATE_IDLE,
        };
        // Update Matter attribute
    }

    /// Get current state for control loop
    pub fn get_state(&self) -> &ThermostatState {
        &self.state
    }
}

pub struct MatterConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_name: String,
    pub serial_number: String,
}

impl Default for MatterConfig {
    fn default() -> Self {
        Self {
            vendor_id: 0xFFF1,  // Test vendor ID
            product_id: 0x8001, // Thermostat
            device_name: "Thermos Thermostat".to_string(),
            serial_number: "THERMO-001".to_string(),
        }
    }
}
```

### Step 3: Matter Server Task

Create `thermos-esp32/src/matter_server.rs`:

```rust
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use rs_matter::Matter;

use crate::matter_device::{MatterConfig, MatterThermostat};
use thermos_core::TemperatureReading;

/// Messages from control loop to Matter server
pub enum MatterMessage {
    TemperatureUpdate(TemperatureReading),
    HeatingStateChanged(bool),
}

/// Channel for communication between control loop and Matter
pub static MATTER_CHANNEL: Channel<CriticalSectionRawMutex, MatterMessage, 8> = 
    Channel::new();

#[embassy_executor::task]
pub async fn matter_server_task(
    stack: &'static Stack<esp_wifi::wifi::WifiDevice<'static>>,
    config: MatterConfig,
) {
    log::info!("Starting Matter server");

    // Initialize Matter stack
    let matter = Matter::new(
        config.vendor_id,
        config.product_id,
        &config.device_name,
    );

    // Create thermostat endpoint
    let mut thermostat = MatterThermostat::new(1, "Zone1".to_string());

    // Print commissioning QR code
    let qr_code = matter.get_qr_code();
    log::info!("Matter QR Code: {}", qr_code);
    log::info!("Pairing code: {}", matter.get_pairing_code());

    // Main Matter event loop
    loop {
        embassy_futures::select::select(
            // Handle Matter protocol events
            matter.run(),
            // Handle updates from control loop
            async {
                match MATTER_CHANNEL.receive().await {
                    MatterMessage::TemperatureUpdate(reading) => {
                        thermostat.update_temperature(reading);
                    }
                    MatterMessage::HeatingStateChanged(heating) => {
                        let state = if heating {
                            thermos_core::HeatingState::Heat
                        } else {
                            thermos_core::HeatingState::Off
                        };
                        thermostat.update_heating_state(state);
                    }
                }
            }
        ).await;
    }
}
```

### Step 4: Integrate with Main Application

Update `thermos-esp32/src/main.rs`:

```rust
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Ticker};

use thermos_core::{apply_hysteresis_control, HysteresisConfig, ThermostatState};

#[cfg(feature = "matter")]
use crate::matter_server::{matter_server_task, MatterConfig, MatterMessage, MATTER_CHANNEL};

#[cfg(feature = "mqtt")]
use crate::mqtt::{mqtt_logger_task, MqttConfig};

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    // Initialize hardware (WiFi, sensors, GPIO, NVS storage)
    // ... hardware setup code ...

    // Spawn Matter server (optional feature)
    #[cfg(feature = "matter")]
    {
        let matter_config = MatterConfig::default();
        spawner.spawn(matter_server_task(stack, matter_config)).unwrap();
        log::info!("Matter server started");
    }

    // Spawn MQTT logger (optional feature)
    #[cfg(feature = "mqtt")]
    {
        let mqtt_config = MqttConfig::default();
        spawner.spawn(mqtt_logger_task(mqtt_client, mqtt_config)).unwrap();
        log::info!("MQTT logger started");
    }

    // Main control loop (uses thermos-core logic)
    spawner.spawn(thermostat_control_task(
        sensor,
        gpio_pin,
        storage,
    )).unwrap();

    // Keep main task alive
    loop {
        embassy_time::Timer::after(Duration::from_secs(60)).await;
    }
}

#[embassy_executor::task]
async fn thermostat_control_task(
    mut sensor: impl TemperatureSensor,
    mut gpio: impl OutputPin,
    mut storage: impl StateStorage,
) {
    let hysteresis = HysteresisConfig::default();
    let mut state = storage.load_state("Zone1").await
        .unwrap_or_default();

    let mut ticker = embassy_time::Ticker::every(Duration::from_secs(6));

    loop {
        ticker.next().await;

        // Read temperature
        match sensor.read_temperature().await {
            Ok(reading) => {
                let new_relay_state = apply_hysteresis_control(
                    reading.celsius,
                    state.target_temp,
                    state.target_state,
                    state.current_relay_state,
                    &hysteresis,
                );

                // Update GPIO
                match new_relay_state {
                    RelayState::On => gpio.set_low()?,
                    RelayState::Off => gpio.set_high()?,
                }

                state.current_relay_state = new_relay_state;
                state.current_temp = reading.celsius;

                // Notify Matter server
                #[cfg(feature = "matter")]
                {
                    MATTER_CHANNEL.send(MatterMessage::TemperatureUpdate(reading)).await;
                    if state.current_relay_state != previous_state {
                        let heating = matches!(state.current_relay_state, RelayState::On);
                        MATTER_CHANNEL.send(MatterMessage::HeatingStateChanged(heating)).await;
                    }
                }

                // Save state to NVS
                storage.save_state("Zone1", &state).await.ok();
            }
            Err(e) => {
                log::error!("Sensor error: {:?}", e);
            }
        }
    }
}
```

### Step 5: Build and Flash

```bash
# Build with Matter support
cd thermos-esp32
cargo build --release --features matter

# Flash to ESP32-C6
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/thermos-esp32

# Or with both MQTT and Matter
cargo build --release --features full
```

### Step 6: Commission the Device

1. **Watch the serial output** for the QR code and pairing code:
   ```
   Matter QR Code: MT:Y.K9042C00KA0648G00
   Pairing code: 34970112332
   ```

2. **Use a Matter controller** to commission:
   - **Apple Home App**: Add Accessory → Scan QR Code
   - **Google Home App**: Add Device → Matter → Scan QR Code
   - **chip-tool**: `chip-tool pairing qrcode 1 MT:Y.K9042C00KA0648G00`

3. **Control the thermostat**:
   - "Hey Siri, set the temperature to 72"
   - "Ok Google, what's the temperature in the living room?"
   - Adjust setpoint in Home app UI

## Matter Thermostat Cluster Details

### Cluster 0x0201: Thermostat

| Attribute ID | Name | Type | Access | Description |
|-------------|------|------|--------|-------------|
| 0x0000 | LocalTemperature | int16 | Read | Current temp (centi-°C) |
| 0x0012 | OccupiedHeatingSetpoint | int16 | Read/Write | Heat setpoint (centi-°C) |
| 0x001C | SystemMode | enum8 | Read/Write | Off(0x00) or Heat(0x04) |
| 0x0029 | ThermostatRunningState | bitmap8 | Read | Bit 0 = heating active |

### Temperature Conversion

Matter uses **centi-degrees Celsius** (hundredths of a degree):

```rust
// Celsius to centi-degrees
let centi_c = (temp_c * 100.0) as i16;
// Example: 21.5°C → 2150

// Centi-degrees to Celsius
let temp_c = (centi_c as f32) / 100.0;
// Example: 2150 → 21.5°C
```

### Commissioning Credentials

The QR code encodes:
- **Vendor ID**: 0xFFF1 (test vendor, change for production)
- **Product ID**: 0x8001 (custom thermostat)
- **Discriminator**: 12-bit value for device identification
- **Setup Passcode**: Numeric code for pairing

## Raspberry Pi Matter Support (Future)

**Status:** Not currently planned for immediate implementation.

**Rationale:** 
- `rs-matter` is optimized for embedded/no_std environments
- Linux support in `rs-matter` is still maturing
- Raspberry Pi deployment works well with existing MQTT/Prometheus architecture

**Future Path:**
- Monitor `rs-matter` development for improved Linux/std support
- Consider integration when `rs-matter` provides stable std platform layer
- Would use same trait-based architecture via `thermos-core`

The legacy C++ FFI approach (Connected Home IP SDK) is **not recommended** due to build complexity and maintenance burden.

## Architecture Benefits

### Shared Core Logic

Both MQTT and Matter use the same control algorithm from `thermos-core`:

```rust
// thermos-core/src/control.rs (platform-agnostic)
pub fn apply_hysteresis_control(
    current_temp_c: f32,
    target_temp_c: f32,
    target_state: HeatingState,
    current_relay_state: RelayState,
    hysteresis: &HysteresisConfig,
) -> RelayState {
    // Same logic for MQTT, Matter, standalone operation
}
```

### Optional Features

Use Cargo features to enable/disable protocols:

```bash
# MQTT only (minimal)
cargo build --features mqtt

# Matter only
cargo build --features matter

# Both protocols
cargo build --features full

# Neither (standalone operation)
cargo build --no-default-features
```

### Coexistence

MQTT and Matter can run simultaneously:
- **Matter**: Control interface (setpoint changes, mode)
- **MQTT**: Data logging (temperature history, state changes)
- **Control loop**: Runs independently, updates both protocols

## ESP32 Hardware Requirements

### Recommended: ESP32-C6

- **Thread radio**: Native Matter-over-Thread support
- **WiFi 6**: Matter-over-WiFi fallback
- **RISC-V**: Better Rust toolchain support than Xtensa
- **Low power**: Suitable for battery operation
- **Price**: ~$3-5 per module

### Alternative: ESP32-C3

- **WiFi only**: Matter-over-WiFi (no Thread)
- **RISC-V**: Good Rust support
- **Lower cost**: ~$2-3 per module
- **Limitation**: No Thread mesh networking

## Testing and Debugging

### Matter Development Tools

1. **chip-tool** - Command-line Matter controller
   ```bash
   chip-tool thermostat read local-temperature 1 1
   chip-tool thermostat write occupied-heating-setpoint 2100 1 1
   ```

2. **Apple Home App** - iOS/macOS HomeKit integration
3. **Google Home App** - Android/web Google Home integration
4. **Python Matter Controller** - Custom automation scripts

### Debugging Tips

**Enable verbose logging:**
```rust
// In main.rs
esp_println::logger::init_logger(log::LevelFilter::Debug);
```

**Monitor Matter commissioning:**
```bash
espflash flash --monitor
# Watch for:
# - "Matter QR Code: ..." 
# - "Device commissioned successfully"
# - "Received setpoint change: 2100"
```

**Check network connectivity:**
- Ensure ESP32 has valid IP address
- Matter uses port 5540 (UDP)
- HomeKit uses mDNS for discovery

## Resources

- **rs-matter GitHub**: [github.com/project-chip/rs-matter](https://github.com/project-chip/rs-matter)
- **Matter Specification**: [CSA IoT Downloads](https://csa-iot.org/developer-resource/specifications-download-request/)
- **ESP32 Rust**: [esp-rs.github.io/book](https://esp-rs.github.io/book/)
- **Embassy**: [embassy.dev](https://embassy.dev)
- **Matter Developer Portal**: [developers.home.google.com/matter](https://developers.home.google.com/matter)

## Summary

**Current approach (2025+):**
- ✅ Use `rs-matter` pure Rust library
- ✅ Target ESP32-C6 with Embassy executor
- ✅ Implement in `thermos-esp32` binary
- ✅ Share control logic via `thermos-core`
- ✅ Coexist with MQTT for data logging

**Deprecated approach (pre-2025):**
- ❌ C++ FFI to Connected Home IP SDK
- ❌ Complex build scripts and dependencies
- ❌ Linux-focused Raspberry Pi implementation

The modern pure Rust approach aligns with the Embassy migration, reduces complexity, and provides better embedded platform support.
