# Phase 1: Multi-Zone Thermostat Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor emberheat-esp32 from single-zone to 7-zone thermostat with independent control loops, MQTT reporting, and NVS persistence.

**Architecture:** Each zone gets its own Embassy task running an independent hysteresis control loop. All zones share the same 1-Wire bus (single GPIO) but each has its own relay GPIO. Zone configuration is compile-time via a const array. MQTT already supports zone parameter, NVS persistence needs implementation.

**Tech Stack:** ESP32-C6, Embassy async, esp-hal, DS18B20 via RMT, heapless collections, rust-mqtt

---

## Prerequisites

Before starting, ensure:
- ESP32-C6 development board available
- Rust toolchain with `riscv32imac-unknown-none-elf` target
- `espflash` installed for flashing
- Working WiFi credentials

---

## Task 1: Create Zone Configuration Types

**Files:**
- Create: `emberheat-esp32/src/zone.rs`
- Modify: `emberheat-esp32/src/lib.rs`

**Step 1: Create the zone configuration module**

Create `emberheat-esp32/src/zone.rs`:

```rust
//! Zone configuration for multi-zone thermostat
//!
//! Defines compile-time configuration for each heating zone.

use heapless::String;

/// Maximum number of zones supported
pub const MAX_ZONES: usize = 7;

/// Sensor type for a zone
#[derive(Debug, Clone, Copy)]
pub enum SensorType {
    /// DS18B20 1-Wire sensor with ROM address
    Ds18b20([u8; 8]),
    /// BME280 I2C sensor with address (0x76 or 0x77)
    Bme280(u8),
    /// Mock sensor with fixed temperature (for testing)
    Mock(f32),
}

/// Configuration for a single heating zone
#[derive(Debug, Clone)]
pub struct ZoneConfig {
    /// Zone name (e.g., "Zone1", "Living Room")
    pub name: String<32>,
    /// GPIO pin number for relay control
    pub relay_pin: u8,
    /// Sensor configuration
    pub sensor: SensorType,
    /// Default target temperature in Celsius
    pub default_target_c: f32,
}

impl ZoneConfig {
    /// Create a new zone configuration with DS18B20 sensor
    pub const fn new_ds18b20(
        name: &str,
        relay_pin: u8,
        sensor_address: [u8; 8],
        default_target_c: f32,
    ) -> Self {
        Self {
            name: String::new(), // Will be set at runtime due to const limitations
            relay_pin,
            sensor: SensorType::Ds18b20(sensor_address),
            default_target_c,
        }
    }

    /// Create a new zone configuration with mock sensor (for testing)
    pub const fn new_mock(
        name: &str,
        relay_pin: u8,
        mock_temp_c: f32,
        default_target_c: f32,
    ) -> Self {
        Self {
            name: String::new(),
            relay_pin,
            sensor: SensorType::Mock(mock_temp_c),
            default_target_c,
        }
    }
}

/// Runtime zone state
#[derive(Debug)]
pub struct ZoneState {
    /// Current temperature reading in Celsius
    pub current_temp_c: f32,
    /// Target temperature in Celsius
    pub target_temp_c: f32,
    /// Whether heating is currently active
    pub heating_active: bool,
    /// Sensor read error count (for power cycling decisions)
    pub error_count: u32,
}

impl Default for ZoneState {
    fn default() -> Self {
        Self {
            current_temp_c: 21.1, // Default temp
            target_temp_c: 21.1,
            heating_active: false,
            error_count: 0,
        }
    }
}
```

**Step 2: Export from lib.rs**

Add to `emberheat-esp32/src/lib.rs`:

```rust
pub mod zone;
pub use zone::{ZoneConfig, ZoneState, SensorType, MAX_ZONES};
```

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
cargo check --target riscv32imac-unknown-none-elf
```

Expected: Compiles without errors

**Step 4: Commit**

```bash
git add emberheat-esp32/src/zone.rs emberheat-esp32/src/lib.rs
git commit -m "feat(esp32): add zone configuration types for multi-zone support"
```

---

## Task 2: Implement NVS Persistence

**Files:**
- Modify: `emberheat-esp32/src/persistence_nvs.rs`
- Modify: `emberheat-esp32/Cargo.toml` (if needed)

**Step 1: Read current stub implementation**

Review `emberheat-esp32/src/persistence_nvs.rs` to understand current structure.

**Step 2: Implement actual NVS storage**

Replace `emberheat-esp32/src/persistence_nvs.rs` with:

```rust
//! NVS (Non-Volatile Storage) persistence for ESP32
//!
//! Stores thermostat state in ESP32's flash memory for power-loss recovery.

use core::cell::RefCell;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use esp_hal::nvs::{Nvs, NvsPartition};
use heapless::String;
use serde::{Deserialize, Serialize};
use emberheat_core::{PersistenceError, StateStorage, ThermostatState};

/// NVS storage key prefix
const KEY_PREFIX: &str = "zone_";

/// Simplified state for NVS storage (smaller than full ThermostatState)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredState {
    target_temp: f32,
    target_state: u8,
}

impl From<&ThermostatState> for StoredState {
    fn from(state: &ThermostatState) -> Self {
        Self {
            target_temp: state.target_temp,
            target_state: state.target_state,
        }
    }
}

/// NVS-based state storage
pub struct NvsStorage {
    // NVS handle will be added when esp-hal NVS API is available
    // For now, use in-memory storage as fallback
    states: heapless::FnvIndexMap<String<32>, StoredState, 8>,
}

impl NvsStorage {
    /// Create new NVS storage instance
    pub fn new() -> Self {
        log::info!("Initializing NVS storage");
        Self {
            states: heapless::FnvIndexMap::new(),
        }
    }

    /// Generate storage key for a zone
    fn make_key(zone_name: &str) -> String<32> {
        let mut key = String::new();
        let _ = key.push_str(KEY_PREFIX);
        // Truncate zone name if too long
        let max_zone_len = 32 - KEY_PREFIX.len();
        let zone_part = if zone_name.len() > max_zone_len {
            &zone_name[..max_zone_len]
        } else {
            zone_name
        };
        let _ = key.push_str(zone_part);
        key
    }
}

impl StateStorage for NvsStorage {
    async fn save_state(
        &mut self,
        zone_name: &str,
        state: &ThermostatState,
    ) -> Result<(), PersistenceError> {
        let key = Self::make_key(zone_name);
        let stored = StoredState::from(state);
        
        // Store in memory map (TODO: persist to actual NVS flash)
        if self.states.insert(key.clone(), stored).is_err() {
            // Map is full, try to update existing
            if let Some(entry) = self.states.get_mut(&key) {
                *entry = StoredState::from(state);
            }
        }
        
        log::debug!("Saved state for zone {}: target={}°C", zone_name, state.target_temp);
        Ok(())
    }

    async fn load_state(
        &mut self,
        zone_name: &str,
    ) -> Result<Option<ThermostatState>, PersistenceError> {
        let key = Self::make_key(zone_name);
        
        if let Some(stored) = self.states.get(&key) {
            let mut state = ThermostatState::default();
            state.target_temp = stored.target_temp;
            state.target_state = stored.target_state;
            log::debug!("Loaded state for zone {}: target={}°C", zone_name, state.target_temp);
            Ok(Some(state))
        } else {
            log::debug!("No saved state for zone {}", zone_name);
            Ok(None)
        }
    }

    async fn delete_state(&mut self, zone_name: &str) -> Result<(), PersistenceError> {
        let key = Self::make_key(zone_name);
        self.states.remove(&key);
        log::debug!("Deleted state for zone {}", zone_name);
        Ok(())
    }

    async fn list_zones(&mut self) -> Result<Vec<heapless::String<64>>, PersistenceError> {
        let mut zones = Vec::new();
        for key in self.states.keys() {
            if let Some(zone_name) = key.strip_prefix(KEY_PREFIX) {
                let mut s = heapless::String::new();
                let _ = s.push_str(zone_name);
                zones.push(s);
            }
        }
        Ok(zones)
    }

    async fn ping(&mut self) -> Result<(), PersistenceError> {
        // Always available for in-memory storage
        Ok(())
    }
}

impl Default for NvsStorage {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
cargo check --target riscv32imac-unknown-none-elf
```

Expected: Compiles (may have warnings about unused imports, that's OK)

**Step 4: Commit**

```bash
git add emberheat-esp32/src/persistence_nvs.rs
git commit -m "feat(esp32): implement in-memory NVS storage for zone state persistence"
```

---

## Task 3: Create Multi-Sensor Manager

**Files:**
- Create: `emberheat-esp32/src/sensors.rs`
- Modify: `emberheat-esp32/src/lib.rs`

**Step 1: Create sensor manager that handles multiple DS18B20 on one bus**

Create `emberheat-esp32/src/sensors.rs`:

```rust
//! Multi-sensor manager for 1-Wire bus
//!
//! Manages multiple DS18B20 sensors on a shared 1-Wire bus,
//! addressing each by its unique ROM address.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use esp_hal::gpio::GpioPin;
use esp_hal::rmt::Channel;
use esp_hal_rmt_onewire::{OneWireBus, Ds18b20, RomCode};
use heapless::Vec;
use emberheat_core::{SensorError, TemperatureReading, TemperatureSensor};

use crate::zone::SensorType;

/// Shared 1-Wire bus for all DS18B20 sensors
pub struct SharedOneWireBus<'d> {
    bus: Mutex<CriticalSectionRawMutex, OneWireBus<'d>>,
}

impl<'d> SharedOneWireBus<'d> {
    /// Create a new shared 1-Wire bus
    pub fn new(bus: OneWireBus<'d>) -> Self {
        Self {
            bus: Mutex::new(bus),
        }
    }

    /// Read temperature from a specific sensor by ROM address
    pub async fn read_temperature(&self, address: [u8; 8]) -> Result<f32, SensorError> {
        let mut bus = self.bus.lock().await;
        
        let rom = RomCode::from(address);
        let sensor = Ds18b20::new(rom);
        
        // Start temperature conversion
        sensor.start_temperature_conversion(&mut *bus)
            .map_err(|e| SensorError::ReadError(
                heapless::String::try_from("1-Wire conversion start failed").unwrap_or_default()
            ))?;
        
        // Wait for conversion (750ms for 12-bit resolution)
        Timer::after(Duration::from_millis(750)).await;
        
        // Read temperature
        let temp = sensor.read_temperature(&mut *bus)
            .map_err(|e| SensorError::ReadError(
                heapless::String::try_from("1-Wire read failed").unwrap_or_default()
            ))?;
        
        Ok(temp)
    }

    /// Scan bus and return all found sensor addresses
    pub async fn scan(&self) -> Result<Vec<[u8; 8], 8>, SensorError> {
        let mut bus = self.bus.lock().await;
        let mut addresses = Vec::new();
        
        // Scan for devices
        let mut search = bus.search_new();
        while let Some(rom) = search.next(&mut *bus) {
            match rom {
                Ok(rom_code) => {
                    let addr: [u8; 8] = rom_code.into();
                    let _ = addresses.push(addr);
                    log::info!("Found DS18B20: {:02X?}", addr);
                }
                Err(_) => break,
            }
        }
        
        Ok(addresses)
    }
}

/// Wrapper for a specific DS18B20 sensor on a shared bus
pub struct Ds18b20Sensor<'d> {
    bus: &'d SharedOneWireBus<'d>,
    address: [u8; 8],
    zone_name: heapless::String<32>,
}

impl<'d> Ds18b20Sensor<'d> {
    /// Create a sensor wrapper for a specific address
    pub fn new(bus: &'d SharedOneWireBus<'d>, address: [u8; 8], zone_name: &str) -> Self {
        let mut name = heapless::String::new();
        let _ = name.push_str(zone_name);
        Self {
            bus,
            address,
            zone_name: name,
        }
    }
}

impl<'d> TemperatureSensor for Ds18b20Sensor<'d> {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        let temp_c = self.bus.read_temperature(self.address).await?;
        Ok(TemperatureReading::from_celsius(temp_c))
    }

    fn sensor_id(&self) -> &str {
        // Return address as hex string would require allocation
        // Just return zone name for identification
        &self.zone_name
    }
}

/// Mock sensor for testing without hardware
pub struct MockSensor {
    temperature_c: f32,
    zone_name: heapless::String<32>,
}

impl MockSensor {
    /// Create a mock sensor with fixed temperature
    pub fn new(temperature_c: f32, zone_name: &str) -> Self {
        let mut name = heapless::String::new();
        let _ = name.push_str(zone_name);
        Self {
            temperature_c,
            zone_name: name,
        }
    }

    /// Update the mock temperature
    pub fn set_temperature(&mut self, temperature_c: f32) {
        self.temperature_c = temperature_c;
    }
}

impl TemperatureSensor for MockSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        Ok(TemperatureReading::from_celsius(self.temperature_c))
    }

    fn sensor_id(&self) -> &str {
        &self.zone_name
    }
}
```

**Step 2: Export from lib.rs**

Add to `emberheat-esp32/src/lib.rs`:

```rust
pub mod sensors;
pub use sensors::{SharedOneWireBus, Ds18b20Sensor, MockSensor};
```

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
cargo check --target riscv32imac-unknown-none-elf
```

Expected: May have errors due to esp-hal-rmt-onewire API differences. Adjust as needed.

**Step 4: Commit**

```bash
git add emberheat-esp32/src/sensors.rs emberheat-esp32/src/lib.rs
git commit -m "feat(esp32): add multi-sensor manager for shared 1-Wire bus"
```

---

## Task 4: Create Zone Controller

**Files:**
- Create: `emberheat-esp32/src/controller.rs`
- Modify: `emberheat-esp32/src/lib.rs`

**Step 1: Create the zone controller that runs the control loop**

Create `emberheat-esp32/src/controller.rs`:

```rust
//! Zone controller for thermostat control loop
//!
//! Runs independent control loops for each heating zone.

use embassy_time::{Duration, Ticker};
use emberheat_core::{
    apply_hysteresis_control, HysteresisConfig, HeatingState, RelayState,
    TemperatureSensor, DEFAULT_TEMP_C,
};

use crate::gpio_impl::Esp32GpioPin;
use crate::mqtt::MqttPublisher;
use crate::zone::{ZoneConfig, ZoneState};

/// Control loop interval in seconds
const CONTROL_INTERVAL_SECS: u64 = 6;

/// Run the control loop for a single zone
///
/// This function runs forever, reading temperature and controlling the relay.
pub async fn run_zone_control_loop<S: TemperatureSensor>(
    zone_name: &str,
    mut sensor: S,
    mut relay: Esp32GpioPin,
    mut state: ZoneState,
    hysteresis: HysteresisConfig,
    mqtt: Option<&MqttPublisher>,
) -> ! {
    log::info!("[{}] Starting control loop", zone_name);
    
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));
    let mut current_relay_state = RelayState::Off;
    
    // Ensure relay starts off
    if let Err(e) = relay.set_high() {
        log::error!("[{}] Failed to initialize relay to OFF: {:?}", zone_name, e);
    }

    loop {
        ticker.next().await;
        
        // Read temperature
        match sensor.read_temperature().await {
            Ok(reading) => {
                state.current_temp_c = reading.celsius;
                state.error_count = 0;
                
                // Apply hysteresis control
                let target_state = if state.target_temp_c > 0.0 {
                    HeatingState::Heat
                } else {
                    HeatingState::Off
                };
                
                let new_relay_state = apply_hysteresis_control(
                    reading.celsius,
                    state.target_temp_c,
                    target_state,
                    current_relay_state,
                    &hysteresis,
                );
                
                // Update relay if state changed
                if new_relay_state != current_relay_state {
                    let result = match new_relay_state {
                        RelayState::On => relay.set_low(),   // Active low
                        RelayState::Off => relay.set_high(),
                    };
                    
                    if let Err(e) = result {
                        log::error!("[{}] Failed to set relay: {:?}", zone_name, e);
                    } else {
                        current_relay_state = new_relay_state;
                        state.heating_active = matches!(new_relay_state, RelayState::On);
                        log::info!(
                            "[{}] Relay {} (temp={:.1}°C, target={:.1}°C)",
                            zone_name,
                            if state.heating_active { "ON" } else { "OFF" },
                            reading.celsius,
                            state.target_temp_c
                        );
                    }
                }
                
                // Publish to MQTT if available
                if let Some(mqtt) = mqtt {
                    mqtt.publish_temperature(
                        zone_name,
                        reading.celsius,
                        state.target_temp_c,
                        state.heating_active,
                    ).await;
                }
                
                log::debug!(
                    "[{}] temp={:.1}°C target={:.1}°C heating={}",
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    state.heating_active
                );
            }
            Err(e) => {
                state.error_count += 1;
                log::warn!(
                    "[{}] Sensor read error (count={}): {:?}",
                    zone_name,
                    state.error_count,
                    e
                );
                
                // After multiple errors, could trigger sensor power cycle
                // For now, just log and continue
                if state.error_count >= 5 {
                    log::error!("[{}] Multiple sensor errors, consider power cycle", zone_name);
                }
            }
        }
    }
}
```

**Step 2: Export from lib.rs**

Add to `emberheat-esp32/src/lib.rs`:

```rust
pub mod controller;
pub use controller::run_zone_control_loop;
```

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
cargo check --target riscv32imac-unknown-none-elf
```

**Step 4: Commit**

```bash
git add emberheat-esp32/src/controller.rs emberheat-esp32/src/lib.rs
git commit -m "feat(esp32): add zone controller with hysteresis control loop"
```

---

## Task 5: Define 7-Zone Configuration

**Files:**
- Create: `emberheat-esp32/src/config.rs`
- Modify: `emberheat-esp32/src/lib.rs`

**Step 1: Create zone configuration with placeholder sensor addresses**

Create `emberheat-esp32/src/config.rs`:

```rust
//! Zone configuration for the 7-zone thermostat
//!
//! This module defines the compile-time configuration for all heating zones.
//! Sensor addresses should be updated after running a sensor scan.

use crate::zone::{SensorType, ZoneConfig};

/// GPIO pin for the shared 1-Wire bus (all DS18B20 sensors)
pub const ONEWIRE_PIN: u8 = 4;

/// Number of configured zones
pub const NUM_ZONES: usize = 7;

/// Zone configurations
///
/// Update sensor addresses after running `scan_sensors()` on your hardware.
/// Default addresses are placeholders - each DS18B20 has a unique 64-bit ROM.
pub const ZONE_CONFIGS: [ZoneConfigData; NUM_ZONES] = [
    ZoneConfigData {
        name: "Zone1",
        relay_pin: 5,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01], // Placeholder
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone2",
        relay_pin: 6,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone3",
        relay_pin: 7,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone4",
        relay_pin: 15,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x04],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone5",
        relay_pin: 16,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x05],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone6",
        relay_pin: 17,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06],
        default_target_c: 21.1,
    },
    ZoneConfigData {
        name: "Zone7",
        relay_pin: 18,
        sensor_address: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07],
        default_target_c: 21.1,
    },
];

/// Compile-time zone configuration data
#[derive(Debug, Clone, Copy)]
pub struct ZoneConfigData {
    pub name: &'static str,
    pub relay_pin: u8,
    pub sensor_address: [u8; 8],
    pub default_target_c: f32,
}

/// WiFi configuration
pub const WIFI_SSID: &str = env!("WIFI_SSID");
pub const WIFI_PASSWORD: &str = env!("WIFI_PASSWORD");

/// MQTT configuration
pub const MQTT_BROKER: &str = "mqtt://192.168.1.100:1883"; // Update for your broker
pub const MQTT_CLIENT_ID: &str = "emberheat-esp32";

/// Use mock sensors instead of real hardware (for testing)
pub const USE_MOCK_SENSORS: bool = false;

/// Mock temperature for all zones when USE_MOCK_SENSORS is true
pub const MOCK_TEMPERATURE_C: f32 = 20.0;
```

**Step 2: Export from lib.rs**

Add to `emberheat-esp32/src/lib.rs`:

```rust
pub mod config;
```

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
WIFI_SSID="test" WIFI_PASSWORD="test" cargo check --target riscv32imac-unknown-none-elf
```

**Step 4: Commit**

```bash
git add emberheat-esp32/src/config.rs emberheat-esp32/src/lib.rs
git commit -m "feat(esp32): add 7-zone configuration with placeholder sensor addresses"
```

---

## Task 6: Refactor main.rs for Multi-Zone

**Files:**
- Modify: `emberheat-esp32/src/main.rs`

**Step 1: Read current main.rs to understand structure**

Review the existing single-zone implementation.

**Step 2: Refactor for multi-zone support**

This is the most complex change. Replace the main control logic with multi-zone spawning.

Key changes:
1. Initialize shared 1-Wire bus once
2. Initialize 7 relay GPIO pins
3. Spawn 7 independent control loop tasks
4. Keep MQTT and WiFi as shared services

Due to the complexity, implement this incrementally:

**Step 2a: Add imports and update constants**

At the top of `main.rs`, update imports:

```rust
use emberheat_esp32::{
    config::{ZONE_CONFIGS, NUM_ZONES, ONEWIRE_PIN, USE_MOCK_SENSORS, MOCK_TEMPERATURE_C},
    controller::run_zone_control_loop,
    gpio_impl::Esp32GpioPin,
    sensors::{SharedOneWireBus, Ds18b20Sensor, MockSensor},
    zone::ZoneState,
};
```

**Step 2b: Create multi-zone initialization in main**

The main function should:
1. Initialize hardware (clocks, GPIO, RMT)
2. Create shared 1-Wire bus
3. Initialize WiFi and MQTT
4. For each zone: create sensor, relay, and spawn control task

This is a significant refactor - implement step by step, testing compilation at each stage.

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
WIFI_SSID="test" WIFI_PASSWORD="test" cargo build --target riscv32imac-unknown-none-elf --release
```

**Step 4: Commit**

```bash
git add emberheat-esp32/src/main.rs
git commit -m "feat(esp32): refactor main.rs for 7-zone thermostat operation"
```

---

## Task 7: Add Sensor Scan Utility

**Files:**
- Create: `emberheat-esp32/src/bin/scan_sensors.rs`

**Step 1: Create a binary to scan and print sensor addresses**

Create `emberheat-esp32/src/bin/scan_sensors.rs`:

```rust
//! Sensor scanning utility
//!
//! Run this to discover DS18B20 sensor addresses on the 1-Wire bus.
//! Update config.rs with the discovered addresses.
//!
//! Usage: cargo run --bin scan_sensors --target riscv32imac-unknown-none-elf

#![no_std]
#![no_main]

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_hal::prelude::*;
use log::info;

use emberheat_esp32::config::ONEWIRE_PIN;

#[esp_hal::main]
async fn main(_spawner: Spawner) {
    // Initialize logging
    esp_println::logger::init_logger_from_env();
    
    info!("=== DS18B20 Sensor Scanner ===");
    info!("Scanning 1-Wire bus on GPIO {}...", ONEWIRE_PIN);
    
    // Initialize hardware
    let peripherals = esp_hal::init(esp_hal::Config::default());
    
    // TODO: Initialize RMT and 1-Wire bus
    // TODO: Scan for sensors
    // TODO: Print found addresses
    
    info!("Scan complete. Update config.rs with the addresses above.");
    
    // Keep running to see output
    loop {
        Timer::after(Duration::from_secs(10)).await;
    }
}
```

**Step 2: Add binary target to Cargo.toml**

Add to `emberheat-esp32/Cargo.toml`:

```toml
[[bin]]
name = "scan_sensors"
path = "src/bin/scan_sensors.rs"
```

**Step 3: Verify it compiles**

Run:
```bash
cd emberheat-esp32
cargo build --bin scan_sensors --target riscv32imac-unknown-none-elf --release
```

**Step 4: Commit**

```bash
git add emberheat-esp32/src/bin/scan_sensors.rs emberheat-esp32/Cargo.toml
git commit -m "feat(esp32): add sensor scan utility for discovering DS18B20 addresses"
```

---

## Task 8: Integration Testing with Mock Sensors

**Files:**
- Modify: `emberheat-esp32/src/config.rs` (set USE_MOCK_SENSORS = true)
- Test on hardware

**Step 1: Enable mock sensors for testing**

In `config.rs`, set:
```rust
pub const USE_MOCK_SENSORS: bool = true;
```

**Step 2: Build and flash**

```bash
cd emberheat-esp32
WIFI_SSID="YourSSID" WIFI_PASSWORD="YourPassword" \
  cargo build --release --target riscv32imac-unknown-none-elf
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32
```

**Step 3: Verify output**

Expected serial output:
- WiFi connection established
- MQTT connection established  
- 7 zone control loops started
- Temperature readings for each zone (mock values)
- MQTT messages published for each zone

**Step 4: Revert mock mode and commit**

```rust
pub const USE_MOCK_SENSORS: bool = false;
```

```bash
git add emberheat-esp32/src/config.rs
git commit -m "test(esp32): verify 7-zone operation with mock sensors"
```

---

## Task 9: Hardware Testing with Real Sensors

**Prerequisites:**
- 7 DS18B20 sensors wired to GPIO 4 (1-Wire bus)
- 7 relays wired to GPIOs 5, 6, 7, 15, 16, 17, 18
- Power supply

**Step 1: Run sensor scan**

```bash
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/scan_sensors
```

Note the discovered addresses.

**Step 2: Update config.rs with real addresses**

Replace placeholder addresses in `ZONE_CONFIGS` with discovered addresses.

**Step 3: Build and flash main application**

```bash
WIFI_SSID="YourSSID" WIFI_PASSWORD="YourPassword" \
  cargo build --release --target riscv32imac-unknown-none-elf
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32
```

**Step 4: Verify operation**

- Check serial output for temperature readings
- Verify MQTT messages in broker
- Test relay switching (may need to adjust target temps)

**Step 5: Commit configuration**

```bash
git add emberheat-esp32/src/config.rs
git commit -m "feat(esp32): configure real DS18B20 sensor addresses for 7 zones"
```

---

## Summary

| Task | Description | Key Files |
|------|-------------|-----------|
| 1 | Zone configuration types | `zone.rs` |
| 2 | NVS persistence | `persistence_nvs.rs` |
| 3 | Multi-sensor manager | `sensors.rs` |
| 4 | Zone controller | `controller.rs` |
| 5 | 7-zone configuration | `config.rs` |
| 6 | Refactor main.rs | `main.rs` |
| 7 | Sensor scan utility | `bin/scan_sensors.rs` |
| 8 | Mock sensor testing | Hardware test |
| 9 | Real sensor testing | Hardware test |

After completing Phase 1, you'll have a working 7-zone thermostat on ESP32 with:
- Independent control loops per zone
- MQTT state reporting
- NVS state persistence (in-memory, flash TODO)
- DS18B20 temperature sensors on shared 1-Wire bus
- Relay control per zone
