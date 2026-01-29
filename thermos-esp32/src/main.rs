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
#[cfg(feature = "external-ble")]
mod dual_radio;
mod gpio_impl;
mod matter;
mod mqtt;
mod persistence_nvs;
mod sensor_i2c;
mod sensors;
#[cfg(not(feature = "matter"))]
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
use config::{MQTT_BROKER_HOST, MQTT_BROKER_PORT, MQTT_DEVICE_ID, MQTT_PASSWORD, MQTT_USERNAME};
#[cfg(not(feature = "matter"))]
use config::{WIFI_PASSWORD, WIFI_SSID};
use core::fmt::Write as FmtWrite;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use gpio_impl::Esp32GpioPin;
use mqtt::{MqttConfig, MqttMessage};
use sensors::MockSensor;
use thermos_core::{
    apply_hysteresis_control, HeatingState, HysteresisConfig, OutputPin, RelayState,
    TemperatureSensor,
};
#[cfg(not(feature = "matter"))]
use wifi::WifiConfig;
use zone::ZoneState;

#[cfg(feature = "matter")]
use esp_radio::Controller as EspRadioController;
#[cfg(feature = "matter")]
use matter::stack::matter_task;

#[cfg(feature = "external-ble")]
use config::{
    HCI_UART_BAUDRATE, HCI_UART_CTS_PIN, HCI_UART_RTS_PIN, HCI_UART_RX_PIN, HCI_UART_TX_PIN,
};
#[cfg(feature = "external-ble")]
use dual_radio::DualRadioDriver;
#[cfg(feature = "external-ble")]
use esp_hal::uart::{Config as UartConfig, RxConfig, Uart};
#[cfg(feature = "external-ble")]
use matter::stack::matter_task_dual_radio;

/// Control loop interval (6 seconds, same as Raspberry Pi)
const CONTROL_INTERVAL_SECS: u64 = 6;

/// MQTT message queue size
const MQTT_QUEUE_SIZE: usize = 16;

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

// MQTT message channel for async communication (used in both MQTT-only and Matter modes)
static MQTT_CHANNEL: Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE> =
    Channel::new();

// Radio controller for Matter WiFi+BLE coexistence
#[cfg(feature = "matter")]
static RADIO_CONTROLLER: StaticCell<EspRadioController<'static>> = StaticCell::new();

// =============================================================================
// MQTT Message Helpers
// =============================================================================

/// Build an MQTT message for temperature reading
fn build_temperature_message(
    zone_name: &str,
    temp_c: f32,
    target_c: f32,
    heating: bool,
) -> Option<mqtt::MqttMessage> {
    // Build topic: thermostat/{device_id}/temp/{zone}
    let mut topic: heapless::String<64> = heapless::String::new();
    if write!(topic, "thermostat/{}/temp/{}", MQTT_DEVICE_ID, zone_name).is_err() {
        return None;
    }

    // Build JSON payload
    let mut payload: heapless::String<256> = heapless::String::new();
    let temp_f = temp_c * 9.0 / 5.0 + 32.0;
    if write!(
        payload,
        r#"{{"zone":"{}","temp_c":{:.1},"temp_f":{:.1},"target_c":{:.1},"heating":{}}}"#,
        zone_name, temp_c, temp_f, target_c, heating
    )
    .is_err()
    {
        return None;
    }

    Some(mqtt::MqttMessage { topic, payload })
}

// =============================================================================
// Zone Control Loop Implementation
// =============================================================================

/// Run the control loop for a single zone (MQTT mode)
///
/// This function runs indefinitely, reading temperature from the sensor,
/// applying hysteresis control, controlling the relay, and publishing to MQTT.
#[cfg(not(feature = "matter"))]
async fn zone_control_loop(
    zone_name: &'static str,
    sensor: &'static MockSensor,
    relay_pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
    mqtt_channel: &'static Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE>,
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

                // Publish temperature to MQTT
                let mqtt_msg = build_temperature_message(
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    state.heating_active,
                );
                if let Some(msg) = mqtt_msg {
                    mqtt_channel.send(msg).await;
                }

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

/// Run the control loop for a single zone (Matter mode with MQTT publishing)
///
/// This function runs indefinitely, reading temperature from the sensor,
/// applying hysteresis control, controlling the relay, and publishing to MQTT.
/// State updates are visible to Matter handlers via ZoneThermostatState atomics.
#[cfg(feature = "matter")]
async fn zone_control_loop(
    zone_name: &'static str,
    sensor: &'static MockSensor,
    relay_pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
    mqtt_channel: &'static Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE>,
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

                // Publish temperature to MQTT (messages queue until MQTT connects)
                let mqtt_msg = build_temperature_message(
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    state.heating_active,
                );
                if let Some(msg) = mqtt_msg {
                    mqtt_channel.send(msg).await;
                }

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
// MQTT Task (only compiled without Matter feature)
// =============================================================================

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn mqtt_task(stack: &'static embassy_net::Stack<'static>, config: mqtt::MqttConfig) {
    mqtt::mqtt_task(stack, config, &MQTT_CHANNEL).await
}

// =============================================================================
// Matter Task (only compiled with Matter feature)
// =============================================================================

#[cfg(all(feature = "matter", not(feature = "external-ble")))]
#[embassy_executor::task]
async fn matter_main_task(
    controller: &'static EspRadioController<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    bt: esp_hal::peripherals::BT<'static>,
    user_task: (), // MQTT disabled due to socket limit in rs-matter-embassy
) {
    matter_task(controller, wifi, bt, user_task).await
}

#[cfg(feature = "external-ble")]
#[embassy_executor::task]
async fn matter_main_task_dual_radio(
    controller: &'static EspRadioController<'static>,
    wifi: esp_hal::peripherals::WIFI<'static>,
    uart_rx: esp_hal::uart::UartRx<'static, esp_hal::Async>,
    uart_tx: esp_hal::uart::UartTx<'static, esp_hal::Async>,
    user_task: (), // MQTT disabled due to socket limit in rs-matter-embassy
) {
    let driver = DualRadioDriver::new(controller, wifi, uart_rx, uart_tx);
    matter_task_dual_radio(driver, user_task).await
}

// =============================================================================
// Zone Control Tasks
// =============================================================================
//
// Embassy requires concrete task functions - we define one per zone.
// We need separate versions for MQTT mode (with channel) and Matter mode (without).

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone1_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[0].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone1_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[0].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone2_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[1].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone2_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[1].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone3_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[2].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone3_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[2].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone4_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[3].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone4_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[3].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone5_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[4].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone5_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[4].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone6_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[5].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone6_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[5].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(not(feature = "matter"))]
#[embassy_executor::task]
async fn zone7_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[6].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
}

#[cfg(feature = "matter")]
#[embassy_executor::task]
async fn zone7_task(
    sensor: &'static MockSensor,
    pin: &'static mut Esp32GpioPin,
    state: &'static mut ZoneState,
    hysteresis: &'static HysteresisConfig,
) {
    zone_control_loop(
        ZONE_CONFIGS[6].name,
        sensor,
        pin,
        state,
        hysteresis,
        &MQTT_CHANNEL,
    )
    .await
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
    // Note: rs-matter-embassy example uses 100KB, but ESP32-C6 has limited RAM
    // and our 7 zone tasks + MQTT need memory too
    esp_alloc::heap_allocator!(size: 73728);

    // Initialize random number generator early (required before Matter crypto)
    // Must be called before esp_rtos::start per rs-matter-embassy example
    #[cfg(feature = "matter")]
    rs_matter_embassy::rand::esp::esp_init_rand(esp_hal::rng::Rng::new());

    // Initialize esp-rtos scheduler with timer and software interrupt
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_int.software_interrupt0);

    println!("====================================");
    info!("====================================");
    info!("Thermos ESP32-C6 Thermostat v0.3.0");
    info!("Multi-Zone Controller");
    info!("====================================");
    info!("Zones: {}", NUM_ZONES);
    info!(
        "Sensors: {}",
        if USE_MOCK_SENSORS { "Mock" } else { "DS18B20" }
    );
    info!("Control interval: {} seconds", CONTROL_INTERVAL_SECS);

    // ==========================================================================
    // Network Initialization (MQTT mode vs Matter mode)
    // ==========================================================================

    #[cfg(not(feature = "matter"))]
    {
        info!("------------------------------------");
        info!("WiFi Configuration:");
        info!("  SSID: {}", WIFI_SSID);
        info!("Initializing WiFi...");

        let wifi_config =
            WifiConfig::new(WIFI_SSID, WIFI_PASSWORD).expect("Failed to create WiFi config");

        let stack = wifi::init_wifi(spawner, wifi_config, peripherals.WIFI)
            .await
            .expect("Failed to initialize WiFi");

        // Wait for IP address assignment
        info!("Waiting for IP address...");
        loop {
            if let Some(config) = stack.config_v4() {
                info!("WiFi connected!");
                info!("  IP Address: {}", config.address.address());
                info!("  Gateway: {:?}", config.gateway);
                break;
            }
            embassy_time::Timer::after(Duration::from_millis(500)).await;
        }

        // Initialize MQTT
        info!("------------------------------------");
        info!("MQTT Configuration:");
        info!("  Broker: {}:{}", MQTT_BROKER_HOST, MQTT_BROKER_PORT);
        info!("  Device ID: {}", MQTT_DEVICE_ID);
        info!(
            "  Auth: {}",
            if MQTT_USERNAME.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );

        let mqtt_config = MqttConfig::with_auth(
            MQTT_BROKER_HOST,
            MQTT_BROKER_PORT,
            MQTT_DEVICE_ID,
            MQTT_USERNAME,
            MQTT_PASSWORD,
        )
        .expect("Failed to create MQTT config");

        // Spawn MQTT task
        spawner
            .spawn(mqtt_task(stack, mqtt_config))
            .expect("Failed to spawn MQTT task");
        info!("MQTT task spawned");
    }

    // Single-radio Matter mode (C6 WiFi + C6 BLE, time-shared)
    #[cfg(all(feature = "matter", not(feature = "external-ble")))]
    {
        info!("------------------------------------");
        info!("Matter Configuration:");
        info!("  Mode: Single-radio (C6 WiFi + C6 BLE, time-shared)");
        info!("  Zones: 1 (single zone test)");

        // Initialize esp-radio for WiFi+BLE coexistence
        let controller =
            RADIO_CONTROLLER.init(esp_radio::init().expect("Failed to initialize esp-radio"));

        // Configure MQTT for Matter mode (runs inside Matter's UserTask)
        info!("------------------------------------");
        info!("MQTT Configuration (Matter mode):");
        info!("  Broker: {}:{}", MQTT_BROKER_HOST, MQTT_BROKER_PORT);
        info!("  Device ID: {}", MQTT_DEVICE_ID);
        info!(
            "  Auth: {}",
            if MQTT_USERNAME.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );

        let mqtt_config = MqttConfig::with_auth(
            MQTT_BROKER_HOST,
            MQTT_BROKER_PORT,
            MQTT_DEVICE_ID,
            MQTT_USERNAME,
            MQTT_PASSWORD,
        )
        .expect("Failed to create MQTT config");

        // Create MQTT bridge that runs inside Matter's UserTask
        // NOTE: MQTT is temporarily disabled - socket set in rs-matter-embassy is too small
        // (4 sockets: 2 UDP for Matter, 1 DHCP, 1 DNS - no room for TCP)
        // TODO: Fork rs-matter-embassy to increase ENET_MIN_SOCKET_SET
        let _mqtt_bridge = matter::MqttBridge::new(mqtt_config, &MQTT_CHANNEL);

        // Spawn Matter task (handles WiFi, MQTT disabled due to socket limit)
        spawner
            .spawn(matter_main_task(
                controller,
                peripherals.WIFI,
                peripherals.BT,
                (), // MQTT disabled - socket set full
            ))
            .expect("Failed to spawn Matter task");
        info!("Matter task spawned (MQTT disabled - socket limit)");
    }

    // Dual-radio Matter mode (C6 WiFi + H2 BLE over UART)
    #[cfg(feature = "external-ble")]
    {
        info!("------------------------------------");
        info!("Matter Configuration:");
        info!("  Mode: Dual-radio (C6 WiFi + H2 BLE over UART)");
        info!("  Zones: 1 (single zone test)");
        info!(
            "  HCI UART: TX=GPIO{}, RX=GPIO{}, RTS=GPIO{}, CTS=GPIO{}, {}baud",
            HCI_UART_TX_PIN, HCI_UART_RX_PIN, HCI_UART_RTS_PIN, HCI_UART_CTS_PIN, HCI_UART_BAUDRATE
        );

        // Initialize esp-radio for WiFi (BLE comes from H2 over UART)
        let controller =
            RADIO_CONTROLLER.init(esp_radio::init().expect("Failed to initialize esp-radio"));

        // Initialize UART for HCI transport to ESP32-H2 with hardware flow control
        // Use a low FIFO threshold to trigger reads before the buffer overflows
        let uart_config = UartConfig::default()
            .with_baudrate(HCI_UART_BAUDRATE)
            .with_rx(RxConfig::default().with_fifo_full_threshold(32));
        let uart = Uart::new(peripherals.UART1, uart_config)
            .expect("Failed to initialize HCI UART")
            .with_rx(peripherals.GPIO19) // RX (H2 TX -> C6 RX)
            .with_tx(peripherals.GPIO18) // TX (C6 TX -> H2 RX)
            .with_rts(peripherals.GPIO6) // RTS (C6 RTS -> H2 CTS)
            .with_cts(peripherals.GPIO7) // CTS (C6 CTS <- H2 RTS)
            .into_async();
        let (uart_rx, uart_tx) = uart.split();
        info!("HCI UART initialized");

        // Configure MQTT for Matter mode (runs inside Matter's UserTask)
        info!("------------------------------------");
        info!("MQTT Configuration (Matter mode):");
        info!("  Broker: {}:{}", MQTT_BROKER_HOST, MQTT_BROKER_PORT);
        info!("  Device ID: {}", MQTT_DEVICE_ID);
        info!(
            "  Auth: {}",
            if MQTT_USERNAME.is_some() {
                "enabled"
            } else {
                "disabled"
            }
        );

        let mqtt_config = MqttConfig::with_auth(
            MQTT_BROKER_HOST,
            MQTT_BROKER_PORT,
            MQTT_DEVICE_ID,
            MQTT_USERNAME,
            MQTT_PASSWORD,
        )
        .expect("Failed to create MQTT config");

        // Create MQTT bridge that runs inside Matter's UserTask
        // NOTE: MQTT is temporarily disabled - socket set in rs-matter-embassy is too small
        let _mqtt_bridge = matter::MqttBridge::new(mqtt_config, &MQTT_CHANNEL);

        // Spawn dual-radio Matter task (C6 WiFi + H2 BLE)
        spawner
            .spawn(matter_main_task_dual_radio(
                controller,
                peripherals.WIFI,
                uart_rx,
                uart_tx,
                (), // MQTT disabled - socket set full
            ))
            .expect("Failed to spawn Matter dual-radio task");
        info!("Matter dual-radio task spawned (C6 WiFi + H2 BLE)");
    }

    info!("------------------------------------");

    // Initialize shared hysteresis config
    let hysteresis = HYSTERESIS.init(HysteresisConfig::default());
    info!(
        "Hysteresis config: +{:.1}C / -{:.1}C",
        hysteresis.upper_threshold_c, hysteresis.lower_threshold_c
    );

    // Initialize GPIO pins for relays (all start in OFF state = HIGH for active-low)
    // Only initialize zones where NUM_ZONES >= zone_number
    info!("Initializing {} zones...", NUM_ZONES);

    // Zone 1: GPIO0 (always initialized if NUM_ZONES >= 1)
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

    // Zone 2: GPIO1
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

    // Zone 3: GPIO2
    let (sensor3, pin3, state3) = if NUM_ZONES >= 3 {
        info!("  Initializing Zone3 GPIO{}...", ZONE_CONFIGS[2].relay_pin);
        let pin = PIN_3.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO2,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_3.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[2].name));
        let state = STATE_3.init(ZoneState::with_target(ZONE_CONFIGS[2].default_target_c));
        info!("  Zone3 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 4: GPIO3
    let (sensor4, pin4, state4) = if NUM_ZONES >= 4 {
        info!("  Initializing Zone4 GPIO{}...", ZONE_CONFIGS[3].relay_pin);
        let pin = PIN_4.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO3,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_4.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[3].name));
        let state = STATE_4.init(ZoneState::with_target(ZONE_CONFIGS[3].default_target_c));
        info!("  Zone4 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 5: GPIO10
    let (sensor5, pin5, state5) = if NUM_ZONES >= 5 {
        info!("  Initializing Zone5 GPIO{}...", ZONE_CONFIGS[4].relay_pin);
        let pin = PIN_5.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO10,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_5.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[4].name));
        let state = STATE_5.init(ZoneState::with_target(ZONE_CONFIGS[4].default_target_c));
        info!("  Zone5 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 6: GPIO11
    let (sensor6, pin6, state6) = if NUM_ZONES >= 6 {
        info!("  Initializing Zone6 GPIO{}...", ZONE_CONFIGS[5].relay_pin);
        let pin = PIN_6.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO11,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_6.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[5].name));
        let state = STATE_6.init(ZoneState::with_target(ZONE_CONFIGS[5].default_target_c));
        info!("  Zone6 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 7: GPIO22
    let (sensor7, pin7, state7) = if NUM_ZONES >= 7 {
        info!("  Initializing Zone7 GPIO{}...", ZONE_CONFIGS[6].relay_pin);
        let pin = PIN_7.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO22,
            Level::High,
            OutputConfig::default(),
        )));
        let sensor = SENSOR_7.init(MockSensor::new(MOCK_TEMPERATURE_C, ZONE_CONFIGS[6].name));
        let state = STATE_7.init(ZoneState::with_target(ZONE_CONFIGS[6].default_target_c));
        info!("  Zone7 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    info!("All relay GPIO pins initialized (OFF state)");
    info!("Mock sensors initialized at {:.1}C", MOCK_TEMPERATURE_C);

    // Log zone configuration
    for config in ZONE_CONFIGS.iter() {
        info!(
            "  {}: relay=GPIO{}, target={:.1}C",
            config.name, config.relay_pin, config.default_target_c
        );
    }

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

    if let (Some(sensor), Some(pin), Some(state)) = (sensor3, pin3, state3) {
        spawner
            .spawn(zone3_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone3 task");
        info!("  Zone3 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor4, pin4, state4) {
        spawner
            .spawn(zone4_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone4 task");
        info!("  Zone4 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor5, pin5, state5) {
        spawner
            .spawn(zone5_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone5 task");
        info!("  Zone5 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor6, pin6, state6) {
        spawner
            .spawn(zone6_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone6 task");
        info!("  Zone6 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor7, pin7, state7) {
        spawner
            .spawn(zone7_task(sensor, pin, state, hysteresis))
            .expect("Failed to spawn Zone7 task");
        info!("  Zone7 task spawned");
    }

    info!("All {} zone control tasks running", NUM_ZONES);
    info!("====================================");

    // Keep main task alive (zones run independently)
    loop {
        embassy_time::Timer::after(Duration::from_secs(60)).await;
    }
}
