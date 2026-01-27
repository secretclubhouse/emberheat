#![no_std]
#![no_main]

//! Thermos ESP32-C6 Multi-Zone Thermostat
//!
//! Embedded 7-zone thermostat controller using Embassy async runtime on ESP32-C6.
//! Implements the same control logic as the Raspberry Pi version but optimized
//! for bare-metal embedded systems.
//!
//! # Architecture
//!
//! - Each zone runs an independent control loop as an Embassy task
//! - Mock sensors are used for testing without hardware
//! - Real DS18B20 sensors share a single 1-Wire bus (future hardware support)
//! - Each zone has its own relay GPIO pin
//!
//! # Configuration
//!
//! Zone configuration is defined in `config.rs`:
//! - `ZONE_CONFIGS`: Array of 7 zone configurations
//! - `USE_MOCK_SENSORS`: Toggle between mock and real sensors
//! - `MOCK_TEMPERATURE_C`: Temperature returned by mock sensors

extern crate alloc;

// ESP-IDF bootloader app descriptor - required for espflash
esp_bootloader_esp_idf::esp_app_desc!();

mod config;
mod controller;
mod gpio_impl;
// TODO: Re-enable mqtt when WiFi support is added
// mod mqtt;
mod persistence_nvs;
mod sensor_i2c;
mod sensors;
mod wifi;
mod zone;

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use esp_backtrace as _;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use log::{debug, error, info, warn};
use static_cell::StaticCell;

use config::{MOCK_TEMPERATURE_C, NUM_ZONES, USE_MOCK_SENSORS, ZONE_CONFIGS};
use gpio_impl::Esp32GpioPin;
use sensors::MockSensor;
use thermos_core::{
    apply_hysteresis_control, HeatingState, HysteresisConfig, OutputPin, RelayState,
    TemperatureSensor,
};
use zone::ZoneState;

/// Control loop interval (6 seconds, same as Raspberry Pi)
const CONTROL_INTERVAL_SECS: u64 = 6;

/// Maximum consecutive sensor errors before logging at error level
const MAX_ERROR_LOG_COUNT: u32 = 10;

// =============================================================================
// Static Storage for Zone Components
// =============================================================================
//
// Embassy tasks require 'static lifetimes. We use StaticCell to create
// static storage for sensors, GPIO pins, and state that persists for the
// program lifetime.

// Individual static cells for each zone's components
// Zone 1
static SENSOR_1: StaticCell<MockSensor> = StaticCell::new();
static PIN_1: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_1: StaticCell<ZoneState> = StaticCell::new();

// Zone 2
static SENSOR_2: StaticCell<MockSensor> = StaticCell::new();
static PIN_2: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_2: StaticCell<ZoneState> = StaticCell::new();

// Zone 3
static SENSOR_3: StaticCell<MockSensor> = StaticCell::new();
static PIN_3: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_3: StaticCell<ZoneState> = StaticCell::new();

// Zone 4
static SENSOR_4: StaticCell<MockSensor> = StaticCell::new();
static PIN_4: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_4: StaticCell<ZoneState> = StaticCell::new();

// Zone 5
static SENSOR_5: StaticCell<MockSensor> = StaticCell::new();
static PIN_5: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_5: StaticCell<ZoneState> = StaticCell::new();

// Zone 6
static SENSOR_6: StaticCell<MockSensor> = StaticCell::new();
static PIN_6: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_6: StaticCell<ZoneState> = StaticCell::new();

// Zone 7
static SENSOR_7: StaticCell<MockSensor> = StaticCell::new();
static PIN_7: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_7: StaticCell<ZoneState> = StaticCell::new();

// Shared hysteresis config
static HYSTERESIS: StaticCell<HysteresisConfig> = StaticCell::new();

// =============================================================================
// Zone Control Loop Implementation
// =============================================================================

/// Run the control loop for a single zone
///
/// This function runs indefinitely, reading temperature from the sensor,
/// applying hysteresis control, and controlling the relay.
async fn zone_control_loop(
    zone_name: &'static str,
    sensor: &'static MockSensor,
    relay_pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) -> ! {
    info!(
        "[{}] Starting control loop (interval: {}s, target: {:.1}C)",
        zone_name, CONTROL_INTERVAL_SECS, state.target_temp_c
    );

    // Initialize relay to OFF state (HIGH = OFF for active-low)
    if let Err(e) = relay_pin.set_high() {
        error!("[{}] Failed to initialize relay to OFF: {:?}", zone_name, e);
    }

    // Track relay state
    let mut current_relay_state = RelayState::Off;

    // Control loop ticker
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));

    loop {
        ticker.next().await;

        // Read temperature from sensor
        match sensor.read_temperature().await {
            Ok(reading) => {
                // Clear errors on successful read
                if state.error_count > 0 {
                    info!(
                        "[{}] Sensor recovered after {} errors",
                        zone_name, state.error_count
                    );
                }
                state.clear_errors();
                state.current_temp_c = reading.celsius;

                debug!(
                    "[{}] Temp: {:.1}C, Target: {:.1}C, Heating: {}",
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    if state.heating_active { "ON" } else { "OFF" }
                );

                // Apply hysteresis control
                let target_state = HeatingState::Heat;
                let new_relay_state = apply_hysteresis_control(
                    reading.celsius,
                    state.target_temp_c,
                    target_state,
                    current_relay_state,
                    hysteresis,
                );

                // Update relay if state changed
                if new_relay_state != current_relay_state {
                    let relay_result = match new_relay_state {
                        RelayState::On => {
                            info!(
                                "[{}] Turning heating ON (temp: {:.1}C, target: {:.1}C)",
                                zone_name, reading.celsius, state.target_temp_c
                            );
                            relay_pin.set_low() // Active-low: LOW = ON
                        }
                        RelayState::Off => {
                            info!(
                                "[{}] Turning heating OFF (temp: {:.1}C, target: {:.1}C)",
                                zone_name, reading.celsius, state.target_temp_c
                            );
                            relay_pin.set_high() // Active-low: HIGH = OFF
                        }
                    };

                    match relay_result {
                        Ok(()) => {
                            current_relay_state = new_relay_state;
                            state.heating_active = new_relay_state == RelayState::On;
                        }
                        Err(e) => {
                            error!("[{}] Failed to set relay state: {:?}", zone_name, e);
                        }
                    }
                }
            }
            Err(e) => {
                // Sensor error handling
                state.increment_errors();

                if state.error_count <= MAX_ERROR_LOG_COUNT {
                    warn!(
                        "[{}] Sensor read failed (error #{}): {}",
                        zone_name, state.error_count, e
                    );
                } else if state.error_count == MAX_ERROR_LOG_COUNT + 1 {
                    error!(
                        "[{}] Sensor errors exceeded threshold, suppressing warnings",
                        zone_name
                    );
                }

                // Safety: Turn off heating on sensor failure
                if current_relay_state == RelayState::On {
                    warn!(
                        "[{}] Turning heating OFF due to sensor failure (safety)",
                        zone_name
                    );
                    if let Err(e) = relay_pin.set_high() {
                        error!("[{}] Failed to turn off relay (safety): {:?}", zone_name, e);
                    } else {
                        current_relay_state = RelayState::Off;
                        state.heating_active = false;
                    }
                }
            }
        }
    }
}

// =============================================================================
// Zone Control Tasks
// =============================================================================
//
// Embassy requires concrete task functions - we define one per zone.

#[embassy_executor::task]
async fn zone1_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[0].name, sensor, pin, state, hysteresis).await
}

#[embassy_executor::task]
async fn zone2_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[1].name, sensor, pin, state, hysteresis).await
}

#[embassy_executor::task]
async fn zone3_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[2].name, sensor, pin, state, hysteresis).await
}

#[embassy_executor::task]
async fn zone4_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[3].name, sensor, pin, state, hysteresis).await
}

#[embassy_executor::task]
async fn zone5_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[4].name, sensor, pin, state, hysteresis).await
}

#[embassy_executor::task]
async fn zone6_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[5].name, sensor, pin, state, hysteresis).await
}

#[embassy_executor::task]
async fn zone7_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(ZONE_CONFIGS[6].name, sensor, pin, state, hysteresis).await
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[esp_rtos::main]
async fn main(spawner: Spawner) {
    // Initialize logging first so we can see any errors
    esp_println::logger::init_logger_from_env();

    // Initialize ESP32 peripherals with default configuration
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize heap allocator (72KB)
    esp_alloc::heap_allocator!(size: 73728);

    // Initialize esp-rtos scheduler with timer and software interrupt
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("====================================");
    info!("====================================");
    info!("Thermos ESP32-C6 Thermostat v0.3.0");
    info!("7-Zone Multi-Zone Controller");
    info!("====================================");
    info!("Zones: {}", NUM_ZONES);
    info!(
        "Sensors: {}",
        if USE_MOCK_SENSORS { "Mock" } else { "DS18B20" }
    );
    info!("Control interval: {} seconds", CONTROL_INTERVAL_SECS);

    // Initialize shared hysteresis config
    let hysteresis = HYSTERESIS.init(HysteresisConfig::default());
    info!(
        "Hysteresis config: +{:.1}C / -{:.1}C",
        hysteresis.upper_threshold_c, hysteresis.lower_threshold_c
    );

    // Initialize GPIO pins for relays (all start in OFF state = HIGH for active-low)
    info!("Initializing relay GPIO pins...");

    // Zone 1: GPIO0
    info!("  Initializing Zone1 GPIO0...");
    let pin1 = PIN_1.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO0,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor1 = SENSOR_1.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[0].name));
    let state1 = STATE_1.init(ZoneState::with_target(ZONE_CONFIGS[0].default_target_c));
    info!("  Zone1 initialized");

    // Zone 2: GPIO1
    info!("  Initializing Zone2 GPIO1...");
    let pin2 = PIN_2.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO1,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor2 = SENSOR_2.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[1].name));
    let state2 = STATE_2.init(ZoneState::with_target(ZONE_CONFIGS[1].default_target_c));
    info!("  Zone2 initialized");

    // Zone 3: GPIO2
    info!("  Initializing Zone3 GPIO2...");
    let pin3 = PIN_3.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO2,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor3 = SENSOR_3.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[2].name));
    let state3 = STATE_3.init(ZoneState::with_target(ZONE_CONFIGS[2].default_target_c));
    info!("  Zone3 initialized");

    // Zone 4: GPIO3
    info!("  Initializing Zone4 GPIO3...");
    let pin4 = PIN_4.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO3,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor4 = SENSOR_4.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[3].name));
    let state4 = STATE_4.init(ZoneState::with_target(ZONE_CONFIGS[3].default_target_c));
    info!("  Zone4 initialized");

    // Zone 5: GPIO10
    info!("  Initializing Zone5 GPIO10...");
    let pin5 = PIN_5.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO10,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor5 = SENSOR_5.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[4].name));
    let state5 = STATE_5.init(ZoneState::with_target(ZONE_CONFIGS[4].default_target_c));
    info!("  Zone5 initialized");

    // Zone 6: GPIO11
    info!("  Initializing Zone6 GPIO11...");
    let pin6 = PIN_6.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO11,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor6 = SENSOR_6.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[5].name));
    let state6 = STATE_6.init(ZoneState::with_target(ZONE_CONFIGS[5].default_target_c));
    info!("  Zone6 initialized");

    // Zone 7: GPIO22
    info!("  Initializing Zone7 GPIO22...");
    let pin7 = PIN_7.init(Esp32GpioPin::new(Output::new(
        peripherals.GPIO22,
        Level::High,
        OutputConfig::default(),
    )));
    let sensor7 = SENSOR_7.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[6].name));
    let state7 = STATE_7.init(ZoneState::with_target(ZONE_CONFIGS[6].default_target_c));
    info!("  Zone7 initialized");

    info!("All relay GPIO pins initialized (OFF state)");
    info!("Mock sensors initialized at {:.1}C", MOCK_TEMPERATURE_C);

    // Log zone configuration
    for config in ZONE_CONFIGS.iter() {
        info!(
            "  {}: relay=GPIO{}, target={:.1}C",
            config.name, config.relay_pin, config.default_target_c
        );
    }

    // Spawn control tasks for all 7 zones
    info!("Spawning zone control tasks...");

    spawner
        .spawn(zone1_task(sensor1, pin1, state1, hysteresis))
        .expect("Failed to spawn Zone1 task");
    info!("  Zone1 task spawned");

    spawner
        .spawn(zone2_task(sensor2, pin2, state2, hysteresis))
        .expect("Failed to spawn Zone2 task");
    info!("  Zone2 task spawned");

    spawner
        .spawn(zone3_task(sensor3, pin3, state3, hysteresis))
        .expect("Failed to spawn Zone3 task");
    info!("  Zone3 task spawned");

    spawner
        .spawn(zone4_task(sensor4, pin4, state4, hysteresis))
        .expect("Failed to spawn Zone4 task");
    info!("  Zone4 task spawned");

    spawner
        .spawn(zone5_task(sensor5, pin5, state5, hysteresis))
        .expect("Failed to spawn Zone5 task");
    info!("  Zone5 task spawned");

    spawner
        .spawn(zone6_task(sensor6, pin6, state6, hysteresis))
        .expect("Failed to spawn Zone6 task");
    info!("  Zone6 task spawned");

    spawner
        .spawn(zone7_task(sensor7, pin7, state7, hysteresis))
        .expect("Failed to spawn Zone7 task");
    info!("  Zone7 task spawned");

    info!("All {} zone control tasks running", NUM_ZONES);
    info!("====================================");

    // Keep main task alive (zones run independently)
    loop {
        embassy_time::Timer::after(Duration::from_secs(60)).await;
    }
}
