#![no_std]
#![no_main]

//! Thermos ESP32-C6 Thermostat with WiFi and MQTT
//!
//! Embedded thermostat controller using Embassy async runtime on ESP32-C6.
//! Implements the same control logic as the Raspberry Pi version but optimized
//! for bare-metal embedded systems.
//!
//! Week 3: WiFi and MQTT integration for temperature history logging

extern crate alloc;

mod gpio_impl;
mod mqtt;
mod persistence_nvs;
mod sensor_i2c;
mod wifi;

use core::str::FromStr;

use embassy_executor::Spawner;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Ticker};
use esp_backtrace as _;
use esp_hal::{
    gpio::{Level, Output},
    i2c::master::I2c,
    rng::Rng,
    time::Rate,
};
use log::{error, info, warn};
use static_cell::make_static;

use gpio_impl::Esp32GpioPin;
use mqtt::{MqttConfig, MqttMessage, MqttPublisher};
use persistence_nvs::NvsStorage;
use sensor_i2c::{Bme280Sensor, MockSensor};
use thermos_core::{
    apply_hysteresis_control, HeatingState, HysteresisConfig, OutputPin, RelayState, StateStorage,
    TemperatureSensor, ThermostatState, DEFAULT_TEMP_C,
};
use wifi::WifiConfig;

/// Zone configuration
const ZONE_NAME: &str = "Zone1";
const RELAY_PIN_NUM: u8 = 4; // GPIO4 for relay control

/// Control loop interval (6 seconds, same as Raspberry Pi)
const CONTROL_INTERVAL_SECS: u64 = 6;

/// WiFi credentials (TODO: move to config file or NVS)
const WIFI_SSID: &str = "YourSSID";
const WIFI_PASSWORD: &str = "YourPassword";

/// MQTT broker configuration
const MQTT_BROKER: &str = "192.168.1.100"; // Change to your broker IP
const MQTT_PORT: u16 = 1883;
const DEVICE_ID: &str = "esp32-zone1";

/// Use mock sensor for testing without hardware
const USE_MOCK_SENSOR: bool = true;

/// Enable WiFi and MQTT (set to false for offline testing)
const ENABLE_WIFI: bool = true;

#[esp_hal::main]
async fn main(spawner: Spawner) {
    // Initialize ESP32 peripherals with default configuration
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Initialize heap allocator (72KB)
    esp_alloc::heap_allocator!(size: 73728);

    // Initialize logging
    log::set_max_level(log::LevelFilter::Info);
    info!("====================================");
    info!("Thermos ESP32-C6 Thermostat v0.2.0");
    info!("Week 3: WiFi + MQTT Integration");
    info!("====================================");
    info!("WiFi: {}", if ENABLE_WIFI { "enabled" } else { "disabled" });
    info!("MQTT: {}", if ENABLE_WIFI { "enabled" } else { "disabled" });
    info!("Control interval: {} seconds", CONTROL_INTERVAL_SECS);

    // Create relay control pin (GPIO4)
    let relay_pin = Output::new(peripherals.GPIO4, Level::High);
    let gpio = Esp32GpioPin::new(relay_pin);

    info!("Relay initialized to OFF state");

    // Initialize storage
    let storage = NvsStorage::new();
    info!("NVS storage initialized");

    // Create MQTT channel for message queueing
    let mqtt_channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, 16> =
        make_static!(Channel::new());

    // Initialize WiFi and MQTT if enabled
    let mqtt_publisher = if ENABLE_WIFI {
        info!("Initializing WiFi...");

        // Create WiFi configuration
        let wifi_config =
            WifiConfig::new(WIFI_SSID, WIFI_PASSWORD).expect("Failed to create WiFi config");

        // Initialize WiFi and get network stack (esp-radio doesn't need RNG/RADIO_CLK)
        let stack = wifi::init_wifi(spawner, wifi_config, peripherals.WIFI)
            .await
            .expect("Failed to initialize WiFi");

        info!("Waiting for network link...");
        loop {
            if stack.is_link_up() {
                break;
            }
            embassy_time::Timer::after(Duration::from_millis(500)).await;
        }

        info!("Waiting for DHCP...");
        loop {
            if let Some(config) = stack.config_v4() {
                info!("IP address: {}", config.address);
                break;
            }
            embassy_time::Timer::after(Duration::from_millis(500)).await;
        }

        // Create MQTT configuration
        let mqtt_config = MqttConfig::new(MQTT_BROKER, MQTT_PORT, DEVICE_ID)
            .expect("Failed to create MQTT config");

        // Spawn MQTT background task
        spawner
            .spawn(mqtt::mqtt_task(stack, mqtt_config.clone(), mqtt_channel))
            .expect("Failed to spawn MQTT task");

        // Create MQTT publisher
        let publisher = MqttPublisher::new(mqtt_channel.sender(), mqtt_config);

        // Publish startup event
        publisher
            .publish_status("online")
            .await
            .expect("Failed to publish startup");

        Some(publisher)
    } else {
        info!("WiFi disabled, running offline");
        None
    };

    // Initialize I2C for temperature sensor
    if USE_MOCK_SENSOR {
        info!("Using mock temperature sensor ({}°C)", DEFAULT_TEMP_C);
        let sensor =
            MockSensor::new("BME280-Mock", DEFAULT_TEMP_C).expect("Failed to create mock sensor");

        // Spawn control loop with mock sensor
        spawner
            .spawn(control_loop_mock(sensor, gpio, storage, mqtt_publisher))
            .expect("Failed to spawn control loop");
    } else {
        info!("Initializing BME280 sensor on I2C");

        use esp_hal::i2c::master::Config;

        let config = Config::default().with_frequency(Rate::from_hz(100_000));

        let i2c = I2c::new(peripherals.I2C0, config)
            .expect("Failed to create I2C")
            .with_sda(peripherals.GPIO6)
            .with_scl(peripherals.GPIO7);

        let mut sensor = Bme280Sensor::new(i2c, Bme280Sensor::DEFAULT_ADDRESS, "BME280-Zone1")
            .expect("Failed to create BME280 sensor");

        // Initialize sensor
        match sensor.init().await {
            Ok(_) => info!("BME280 sensor initialized successfully"),
            Err(e) => {
                error!("Failed to initialize BME280: {:?}", e);
                error!("Continuing with potentially invalid readings...");
            }
        }

        // Spawn control loop with BME280 sensor
        spawner
            .spawn(control_loop_bme280(sensor, gpio, storage, mqtt_publisher))
            .expect("Failed to spawn control loop");
    }

    info!("Control loop spawned, entering idle mode");

    // Keep main task alive
    loop {
        embassy_time::Timer::after(Duration::from_secs(60)).await;
    }
}

/// Main thermostat control loop task with mock sensor
#[embassy_executor::task]
async fn control_loop_mock(
    sensor: MockSensor,
    mut gpio: Esp32GpioPin,
    mut storage: NvsStorage,
    mqtt: Option<MqttPublisher>,
) {
    info!("Control loop starting for zone: {}", ZONE_NAME);

    // Load saved state or use defaults
    let mut target_temp = DEFAULT_TEMP_C;

    match storage.load_state(ZONE_NAME).await {
        Ok(Some(saved_state)) => {
            info!("Loaded saved state: target={}°C", saved_state.target_temp);
            target_temp = saved_state.target_temp;
        }
        Ok(None) => {
            info!("No saved state found, using defaults");
        }
        Err(e) => {
            error!("Failed to load state: {:?}, using defaults", e);
        }
    }

    // Runtime control state (not persisted every cycle)
    let target_state = HeatingState::Heat;
    let mut current_relay_state = RelayState::Off;

    // Hysteresis configuration (±0.5°C, same as Raspberry Pi)
    let hysteresis = HysteresisConfig::default();

    // Control loop ticker (6 second interval)
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));

    info!(
        "Control loop running - target: {}°C, hysteresis: ±{}°C",
        target_temp, hysteresis.upper_threshold_c
    );

    loop {
        ticker.next().await;

        // Read temperature from sensor
        match sensor.read_temperature().await {
            Ok(reading) => {
                let temp_c = reading.celsius;

                // Apply hysteresis control algorithm from thermos-core
                let new_relay_state = apply_hysteresis_control(
                    temp_c,
                    target_temp,
                    target_state,
                    current_relay_state,
                    &hysteresis,
                );

                // Update relay if state changed
                if new_relay_state != current_relay_state {
                    let heating = matches!(new_relay_state, RelayState::On);
                    let heating_str = if heating { "ON" } else { "OFF" };

                    info!(
                        "Temp: {:.1}°C, Target: {:.1}°C → Heating: {}",
                        temp_c, target_temp, heating_str
                    );

                    // Control GPIO (active-low: LOW = ON, HIGH = OFF)
                    let gpio_result = match new_relay_state {
                        RelayState::On => gpio.set_low(),
                        RelayState::Off => gpio.set_high(),
                    };

                    if let Err(e) = gpio_result {
                        error!("Failed to control relay: {:?}", e);
                    } else {
                        current_relay_state = new_relay_state;

                        // Publish state change event to MQTT
                        if let Some(ref mqtt) = mqtt {
                            let msg = if heating { "Heating ON" } else { "Heating OFF" };
                            if let Err(e) = mqtt.publish_event(ZONE_NAME, "state_change", msg).await
                            {
                                warn!("Failed to publish MQTT event: {}", e);
                            }
                        }
                    }

                    // Save state to NVS when relay state changes
                    let state = ThermostatState {
                        relay_pin: RELAY_PIN_NUM,
                        temp_pin: 0,
                        temp_id: heapless::String::from_str(sensor.id()).unwrap_or_default(),
                        target_temp,
                        target_state: target_state as u8,
                        current_state: current_relay_state as u8,
                        #[cfg(not(feature = "std"))]
                        last_updated: 0,
                        #[cfg(feature = "std")]
                        last_updated: chrono::Utc::now(),
                    };

                    if let Err(e) = storage.save_state(ZONE_NAME, &state).await {
                        warn!("Failed to save state: {:?}", e);
                    }
                }

                // Publish temperature reading to MQTT every cycle
                if let Some(ref mqtt) = mqtt {
                    let heating = matches!(current_relay_state, RelayState::On);
                    if let Err(e) = mqtt
                        .publish_temperature(ZONE_NAME, temp_c, target_temp, heating)
                        .await
                    {
                        warn!("Failed to publish MQTT temperature: {}", e);
                    }
                }

                // Log current state every minute (every 10th iteration)
                static mut LOG_COUNTER: u32 = 0;
                unsafe {
                    LOG_COUNTER += 1;
                    if LOG_COUNTER >= 10 {
                        LOG_COUNTER = 0;
                        let heating_str = match current_relay_state {
                            RelayState::On => "ON",
                            RelayState::Off => "OFF",
                        };
                        info!(
                            "Temp: {:.1}°C, Target: {:.1}°C, Heating: {}",
                            temp_c, target_temp, heating_str
                        );
                    }
                }
            }
            Err(e) => {
                error!("Sensor read error: {:?}", e);

                // On sensor error, turn heating OFF for safety
                if current_relay_state != RelayState::Off {
                    warn!("Sensor error: turning heating OFF for safety");
                    if let Err(gpio_err) = gpio.set_high() {
                        error!("CRITICAL: Failed to turn off relay: {:?}", gpio_err);
                    } else {
                        current_relay_state = RelayState::Off;

                        // Publish error event
                        if let Some(ref mqtt) = mqtt {
                            let _ = mqtt
                                .publish_event(ZONE_NAME, "error", "Sensor read failed")
                                .await;
                        }
                    }
                }
            }
        }
    }
}

/// Main thermostat control loop task with BME280 sensor
#[embassy_executor::task]
async fn control_loop_bme280(
    sensor: Bme280Sensor,
    mut gpio: Esp32GpioPin,
    mut storage: NvsStorage,
    mqtt: Option<MqttPublisher>,
) {
    info!("Control loop starting for zone: {}", ZONE_NAME);

    // Load saved state or use defaults
    let mut target_temp = DEFAULT_TEMP_C;

    match storage.load_state(ZONE_NAME).await {
        Ok(Some(saved_state)) => {
            info!("Loaded saved state: target={}°C", saved_state.target_temp);
            target_temp = saved_state.target_temp;
        }
        Ok(None) => {
            info!("No saved state found, using defaults");
        }
        Err(e) => {
            error!("Failed to load state: {:?}, using defaults", e);
        }
    }

    // Runtime control state (not persisted every cycle)
    let target_state = HeatingState::Heat;
    let mut current_relay_state = RelayState::Off;

    // Hysteresis configuration (±0.5°C, same as Raspberry Pi)
    let hysteresis = HysteresisConfig::default();

    // Control loop ticker (6 second interval)
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));

    info!(
        "Control loop running - target: {}°C, hysteresis: ±{}°C",
        target_temp, hysteresis.upper_threshold_c
    );

    loop {
        ticker.next().await;

        // Read temperature from sensor
        match sensor.read_temperature().await {
            Ok(reading) => {
                let temp_c = reading.celsius;

                // Apply hysteresis control algorithm from thermos-core
                let new_relay_state = apply_hysteresis_control(
                    temp_c,
                    target_temp,
                    target_state,
                    current_relay_state,
                    &hysteresis,
                );

                // Update relay if state changed
                if new_relay_state != current_relay_state {
                    let heating = matches!(new_relay_state, RelayState::On);
                    let heating_str = if heating { "ON" } else { "OFF" };

                    info!(
                        "Temp: {:.1}°C, Target: {:.1}°C → Heating: {}",
                        temp_c, target_temp, heating_str
                    );

                    // Control GPIO (active-low: LOW = ON, HIGH = OFF)
                    let gpio_result = match new_relay_state {
                        RelayState::On => gpio.set_low(),
                        RelayState::Off => gpio.set_high(),
                    };

                    if let Err(e) = gpio_result {
                        error!("Failed to control relay: {:?}", e);
                    } else {
                        current_relay_state = new_relay_state;

                        // Publish state change event to MQTT
                        if let Some(ref mqtt) = mqtt {
                            let msg = if heating { "Heating ON" } else { "Heating OFF" };
                            if let Err(e) = mqtt.publish_event(ZONE_NAME, "state_change", msg).await
                            {
                                warn!("Failed to publish MQTT event: {}", e);
                            }
                        }
                    }

                    // Save state to NVS when relay state changes
                    let state = ThermostatState {
                        relay_pin: RELAY_PIN_NUM,
                        temp_pin: 0,
                        temp_id: heapless::String::from_str(sensor.id()).unwrap_or_default(),
                        target_temp,
                        target_state: target_state as u8,
                        current_state: current_relay_state as u8,
                        #[cfg(not(feature = "std"))]
                        last_updated: 0,
                        #[cfg(feature = "std")]
                        last_updated: chrono::Utc::now(),
                    };

                    if let Err(e) = storage.save_state(ZONE_NAME, &state).await {
                        warn!("Failed to save state: {:?}", e);
                    }
                }

                // Publish temperature reading to MQTT every cycle
                if let Some(ref mqtt) = mqtt {
                    let heating = matches!(current_relay_state, RelayState::On);
                    if let Err(e) = mqtt
                        .publish_temperature(ZONE_NAME, temp_c, target_temp, heating)
                        .await
                    {
                        warn!("Failed to publish MQTT temperature: {}", e);
                    }
                }

                // Log current state every minute (every 10th iteration)
                static mut LOG_COUNTER: u32 = 0;
                unsafe {
                    LOG_COUNTER += 1;
                    if LOG_COUNTER >= 10 {
                        LOG_COUNTER = 0;
                        let heating_str = match current_relay_state {
                            RelayState::On => "ON",
                            RelayState::Off => "OFF",
                        };
                        info!(
                            "Temp: {:.1}°C, Target: {:.1}°C, Heating: {}",
                            temp_c, target_temp, heating_str
                        );
                    }
                }
            }
            Err(e) => {
                error!("Sensor read error: {:?}", e);

                // On sensor error, turn heating OFF for safety
                if current_relay_state != RelayState::Off {
                    warn!("Sensor error: turning heating OFF for safety");
                    if let Err(gpio_err) = gpio.set_high() {
                        error!("CRITICAL: Failed to turn off relay: {:?}", gpio_err);
                    } else {
                        current_relay_state = RelayState::Off;

                        // Publish error event
                        if let Some(ref mqtt) = mqtt {
                            let _ = mqtt
                                .publish_event(ZONE_NAME, "error", "Sensor read failed")
                                .await;
                        }
                    }
                }
            }
        }
    }
}
