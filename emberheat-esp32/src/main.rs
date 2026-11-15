#![no_std]
#![no_main]

//! EmberHeat ESP32-C6 Multi-Zone Thermostat
//!
//! Embedded multi-zone (up to 7) thermostat controller using Embassy async runtime on ESP32-C6.
//! Implements the same control logic as the Raspberry Pi version but optimized
//! for bare-metal embedded systems.
//!
//! # Architecture
//!
//! - Each zone runs an independent control loop as an Embassy task
//! - Zone temperatures come from mock sensors or remote MQTT sensors
//!   (`SENSOR_SOURCE`), with a bounded duty-cycle fallback when remote
//!   readings stop
//! - Real DS18B20 sensors share a single 1-Wire bus (future hardware support)
//! - Each zone has its own relay GPIO pin
//!
//! # Configuration
//!
//! Zone configuration is defined in `config.rs`:
//! - `NUM_ZONES` / `ZONE_CONFIGS`: number of zones and their configuration
//! - `SENSOR_SOURCE`: Mock or MQTT remote sensors
//! - `MOCK_TEMPERATURE_C`: Temperature returned by mock sensors

extern crate alloc;

// ESP-IDF bootloader app descriptor - required for espflash
esp_bootloader_esp_idf::esp_app_desc!();

mod config;
mod controller;
#[cfg(feature = "external-ble")]
mod dual_radio;
mod failsafe;
mod gpio_impl;
mod matter;
mod mqtt;
mod persistence;
mod persistence_nvs;
mod sensor_i2c;
mod sensor_remote;
mod sensors;
#[cfg(not(feature = "matter"))]
mod wifi;
mod zone;

use embassy_executor::Spawner;
use embassy_time::{Duration, Ticker};
use esp_backtrace as _;
use esp_hal::gpio::{Level, Output, OutputConfig};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::timg::TimerGroup;
use esp_println::println;
use log::{debug, error, info, warn};
use static_cell::StaticCell;

#[cfg(feature = "matter")]
use config::MOCK_TEMPERATURE_C;
#[cfg(feature = "matter")]
use config::PERSIST_FLASH_RANGE;
use config::{SensorSource, NUM_ZONES, SENSOR_SOURCE, ZONE_CONFIGS};
use config::{MQTT_BROKER_HOST, MQTT_BROKER_PORT, MQTT_DEVICE_ID, MQTT_PASSWORD, MQTT_USERNAME};
use config::{REMOTE_FALLBACK, REMOTE_SENSOR_MAX_AGE_SECS, SENSOR_STATUS_REPUBLISH_SECS};
#[cfg(not(feature = "matter"))]
use config::{WIFI_PASSWORD, WIFI_SSID};
use core::fmt::Write as FmtWrite;
#[cfg(feature = "matter")]
use embassy_embedded_hal::adapter::BlockingAsync;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::Instant;
use emberheat_core::fallback::{
    fallback_on_secs, FallbackController, FallbackDecision, SensorStatus,
};
use emberheat_core::{
    apply_hysteresis_control, HeatingState, HysteresisConfig, OutputPin, RelayState, SensorError,
    TemperatureSensor,
};
#[cfg(feature = "matter")]
use esp_storage::FlashStorage;
use gpio_impl::Esp32GpioPin;
use mqtt::MqttConfig;
#[cfg(feature = "matter")]
use persistence::{load_all_zones, FlashKvBlobStore};
#[cfg(feature = "matter")]
use sensor_remote::zone_index_by_name;
use sensor_remote::ZoneSensor;
#[cfg(not(feature = "matter"))]
use wifi::WifiConfig;
use zone::ZoneState;

#[cfg(feature = "matter")]
#[cfg(feature = "matter")]
use matter::stack::matter_task;

#[cfg(feature = "external-ble")]
use config::{
    HCI_UART_BAUDRATE, HCI_UART_CTS_PIN, HCI_UART_RTS_PIN, HCI_UART_RTS_THRESHOLD, HCI_UART_RX_PIN,
    HCI_UART_TX_PIN,
};
#[cfg(feature = "external-ble")]
use dual_radio::DualRadioDriver;
#[cfg(feature = "external-ble")]
use esp_hal::uart::{Config as UartConfig, RxConfig, Uart};
#[cfg(feature = "external-ble")]
use matter::stack::matter_task_dual_radio;

/// Control loop interval (6 seconds, same as Raspberry Pi)
const CONTROL_INTERVAL_SECS: u64 = 6;

/// GPIO each zone's relay is wired to in `main` (the `peripherals.GPIOn` used
/// for `relayN`). Embassy needs concrete pin types, so these are hardcoded.
const HARDWIRED_RELAY_GPIOS: [u8; 7] = [0, 1, 2, 3, 10, 11, 22];

// The panic path turns relays OFF by pin number from ZONE_CONFIGS; if that
// ever disagreed with the pins actually driven here, a crash would leave a
// relay ON. Catch the mismatch at build time.
const _: () = {
    let mut i = 0;
    while i < NUM_ZONES {
        assert!(
            ZONE_CONFIGS[i].relay_pin == HARDWIRED_RELAY_GPIOS[i],
            "ZONE_CONFIGS relay_pin does not match the GPIO wired up in main.rs"
        );
        i += 1;
    }
};

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
static SENSOR_1: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_1: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_1: StaticCell<ZoneState> = StaticCell::new();

// Zone 2
static SENSOR_2: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_2: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_2: StaticCell<ZoneState> = StaticCell::new();

// Zone 3
static SENSOR_3: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_3: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_3: StaticCell<ZoneState> = StaticCell::new();

// Zone 4
static SENSOR_4: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_4: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_4: StaticCell<ZoneState> = StaticCell::new();

// Zone 5
static SENSOR_5: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_5: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_5: StaticCell<ZoneState> = StaticCell::new();

// Zone 6
static SENSOR_6: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_6: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_6: StaticCell<ZoneState> = StaticCell::new();

// Zone 7
static SENSOR_7: StaticCell<ZoneSensor> = StaticCell::new();
static PIN_7: StaticCell<Esp32GpioPin> = StaticCell::new();
static STATE_7: StaticCell<ZoneState> = StaticCell::new();

// Shared hysteresis config
static HYSTERESIS: StaticCell<HysteresisConfig> = StaticCell::new();

// MQTT message channel for async communication (used in both MQTT-only and Matter modes)
static MQTT_CHANNEL: Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE> =
    Channel::new();

// =============================================================================
// MQTT Message Helpers
// =============================================================================

/// Build an MQTT message for temperature reading
fn build_temperature_message(
    zone_name: &str,
    temp_c: f32,
    target_c: f32,
    heating: bool,
    sensors: u8,
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
        r#"{{"zone":"{}","temp_c":{:.1},"temp_f":{:.1},"target_c":{:.1},"heating":{},"sensors":{}}}"#,
        zone_name, temp_c, temp_f, target_c, heating, sensors
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

/// Queue a message for MQTT, dropping it when the queue is full so control
/// never waits on the network. Returns `true` if the message was queued.
fn queue_mqtt(
    channel: &'static Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE>,
    msg: mqtt::MqttMessage,
) -> bool {
    // Full queue (normal during a broker outage): drop the message. Callers
    // that must not lose it retry on a later pass.
    channel.try_send(msg).is_ok()
}

/// Build an MQTT message reporting the health of a zone's temperature input.
///
/// Topic `thermostat/<device_id>/zone/<zone>/sensor_status`, payload
/// `ok`, `fallback` or `lost`.
fn build_sensor_status_message(zone_name: &str, status: SensorStatus) -> Option<mqtt::MqttMessage> {
    let mut topic: heapless::String<64> = heapless::String::new();
    if write!(
        topic,
        "thermostat/{}/zone/{}/sensor_status",
        MQTT_DEVICE_ID, zone_name
    )
    .is_err()
    {
        return None;
    }

    let mut payload: heapless::String<256> = heapless::String::new();
    payload.push_str(status.as_str()).ok()?;

    Some(mqtt::MqttMessage { topic, payload })
}

/// What the fallback cycle is doing with the relay, for the status log line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FallbackActivity {
    /// Cycling the relay at the recent duty.
    Cycling,
    /// Relay held OFF because the thermostat mode is not Heat.
    HeldOffNotHeat,
    /// Relay held OFF because the recent duty is too low to be worth cycling.
    HeldOffLowDuty,
}

impl FallbackActivity {
    /// Classify a fallback pass. `heat_mode` is whether the thermostat mode
    /// allows heating (always true without Matter).
    fn classify(duty: f32, heat_mode: bool) -> Self {
        if !heat_mode {
            FallbackActivity::HeldOffNotHeat
        } else if fallback_on_secs(duty, &REMOTE_FALLBACK) == 0 {
            FallbackActivity::HeldOffLowDuty
        } else {
            FallbackActivity::Cycling
        }
    }
}

/// Per-zone sensor status reporting: logs each transition once and publishes
/// the status until it has actually been queued, then again every
/// `SENSOR_STATUS_REPUBLISH_SECS` so a restarted broker or Home Assistant
/// picks it up.
struct SensorStatusReporter {
    /// Last status (and fallback activity) written to the log.
    logged: Option<(SensorStatus, Option<FallbackActivity>)>,
    /// Last status successfully queued for MQTT, and when.
    queued: Option<(SensorStatus, u64)>,
    /// Whether the "waiting for first reading" line has been logged.
    waiting_logged: bool,
    /// Whether the status topic overflowed and that was logged.
    topic_error_logged: bool,
}

impl SensorStatusReporter {
    const fn new() -> Self {
        Self {
            logged: None,
            queued: None,
            waiting_logged: false,
            topic_error_logged: false,
        }
    }

    /// Record this pass's status. `activity` is `Some` only for `Fallback`.
    fn report(
        &mut self,
        zone_name: &str,
        status: SensorStatus,
        activity: Option<FallbackActivity>,
        duty: f32,
        channel: &'static Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE>,
    ) {
        if self.logged != Some((status, activity)) {
            match (status, activity) {
                (SensorStatus::Ok, _) => info!("[{}] Sensor status: ok", zone_name),
                (SensorStatus::Fallback, Some(FallbackActivity::HeldOffNotHeat)) => warn!(
                    "[{}] Sensor status: fallback (readings lost, heating held OFF: mode is not Heat)",
                    zone_name
                ),
                (SensorStatus::Fallback, Some(FallbackActivity::HeldOffLowDuty)) => warn!(
                    "[{}] Sensor status: fallback (readings lost, heating held OFF: recent duty {:.0}% too low to cycle)",
                    zone_name,
                    duty * 100.0
                ),
                (SensorStatus::Fallback, _) => warn!(
                    "[{}] Sensor status: fallback (readings lost, cycling relay at {:.0}% duty)",
                    zone_name,
                    duty * 100.0
                ),
                (SensorStatus::Lost, _) => {
                    warn!("[{}] Sensor status: lost (heating OFF)", zone_name)
                }
            }
            self.logged = Some((status, activity));
        }

        let now = Instant::now().as_secs();
        let due = match self.queued {
            Some((s, at)) => s != status || now.saturating_sub(at) >= SENSOR_STATUS_REPUBLISH_SECS,
            None => true,
        };
        if !due {
            return;
        }
        match build_sensor_status_message(zone_name, status) {
            // Not queued (queue full during an outage): try again next pass.
            Some(msg) => {
                if queue_mqtt(channel, msg) {
                    self.queued = Some((status, now));
                }
            }
            None => {
                if !self.topic_error_logged {
                    error!(
                        "[{}] Sensor status topic too long; status will not be published",
                        zone_name
                    );
                    self.topic_error_logged = true;
                }
            }
        }
    }

    /// Log, once, that the zone is waiting for its first reading after boot.
    fn note_waiting_for_first_reading(&mut self, zone_name: &str) {
        if !self.waiting_logged {
            info!(
                "[{}] Waiting for first reading (heating OFF, no status for up to {}s after boot)",
                zone_name, REMOTE_SENSOR_MAX_AGE_SECS
            );
            self.waiting_logged = true;
        }
    }
}

/// Whether a pass with no reading and no history is still inside the
/// post-boot window in which sensors are expected to report for the first
/// time. Inside it the zone stays OFF quietly instead of reporting `lost`.
fn in_boot_grace(decision: FallbackDecision) -> bool {
    decision == FallbackDecision::NoHistory && Instant::now().as_secs() < REMOTE_SENSOR_MAX_AGE_SECS
}

/// Drive an active-low relay to `new_state` (ON = LOW, OFF = HIGH).
///
/// `current` and `state.heating_active` change only when the GPIO write
/// succeeds, so they always reflect what the pin was last set to.
fn write_relay(
    zone_name: &str,
    relay_pin: &mut Esp32GpioPin,
    current: &mut RelayState,
    state: &mut ZoneState,
    new_state: RelayState,
) {
    if new_state == *current {
        return;
    }
    let relay_result = match new_state {
        RelayState::On => relay_pin.set_low(), // Active-low: LOW = ON
        RelayState::Off => relay_pin.set_high(), // Active-low: HIGH = OFF
    };
    match relay_result {
        Ok(()) => {
            *current = new_state;
            state.heating_active = new_state == RelayState::On;
        }
        Err(e) => {
            error!("[{}] Failed to set relay state: {:?}", zone_name, e);
        }
    }
}

/// Run the control loop for a single zone (MQTT mode)
///
/// This function runs indefinitely, reading temperature from the sensor,
/// applying hysteresis control, controlling the relay, and publishing to MQTT.
#[cfg(not(feature = "matter"))]
async fn zone_control_loop(
    zone_name: &'static str,
    sensor: &'static ZoneSensor,
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

    // Duty history and outage clock for the fallback cycle (RAM only)
    let mut fallback = FallbackController::new();
    let mut status = SensorStatusReporter::new();

    // Heartbeat slot for the watchdog supervisor. An unknown zone name never
    // beats, so the supervisor restarts the controller instead of trusting it.
    let beat_index = sensor_remote::zone_index_by_name(zone_name);

    // Control loop ticker
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));

    loop {
        ticker.next().await;

        // Tell the watchdog supervisor this loop is still turning over
        if let Some(index) = beat_index {
            failsafe::zone_beat(index);
        }

        // Read temperature from sensor
        let reading = sensor.read_temperature().await;

        // Readings unavailable (none fresh): keep cycling at the recent duty
        // for a bounded time. Any other outcome falls through to the match.
        if let Err(SensorError::Unavailable(_)) = reading {
            let decision = fallback.on_unavailable(Instant::now().as_secs(), &REMOTE_FALLBACK);
            if let FallbackDecision::Relay(on) = decision {
                let new_relay_state = if on { RelayState::On } else { RelayState::Off };
                if new_relay_state != current_relay_state {
                    info!(
                        "[{}] Fallback: turning heating {}",
                        zone_name,
                        if on { "ON" } else { "OFF" }
                    );
                }
                write_relay(
                    zone_name,
                    relay_pin,
                    &mut current_relay_state,
                    state,
                    new_relay_state,
                );
                status.report(
                    zone_name,
                    SensorStatus::Fallback,
                    Some(FallbackActivity::classify(fallback.duty(), true)),
                    fallback.duty(),
                    mqtt_channel,
                );
                continue;
            }
            if in_boot_grace(decision) {
                // Just booted and no sensor has reported yet: relay OFF, but
                // no `lost` status and no sensor error until the window ends.
                write_relay(
                    zone_name,
                    relay_pin,
                    &mut current_relay_state,
                    state,
                    RelayState::Off,
                );
                status.note_waiting_for_first_reading(zone_name);
                continue;
            }
        }

        match reading {
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
                    info!(
                        "[{}] Turning heating {} (temp: {:.1}C, target: {:.1}C)",
                        zone_name,
                        if new_relay_state == RelayState::On {
                            "ON"
                        } else {
                            "OFF"
                        },
                        reading.celsius,
                        state.target_temp_c
                    );
                    write_relay(
                        zone_name,
                        relay_pin,
                        &mut current_relay_state,
                        state,
                        new_relay_state,
                    );
                }

                // Fold this pass into the duty history the fallback runs on
                fallback.on_reading(
                    current_relay_state == RelayState::On,
                    CONTROL_INTERVAL_SECS as u32,
                );
                status.report(
                    zone_name,
                    SensorStatus::Ok,
                    None,
                    fallback.duty(),
                    mqtt_channel,
                );

                // Publish temperature to MQTT after the relay is updated; never
                // waits, so a down broker cannot stall control.
                if let Some(msg) = build_temperature_message(
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    state.heating_active,
                    sensor.sensors_used(),
                ) {
                    queue_mqtt(mqtt_channel, msg);
                }
            }
            Err(e) => {
                // Not covered by fallback: other sensor errors, no history
                // since boot, or the fallback time limit has passed.
                status.report(
                    zone_name,
                    SensorStatus::Lost,
                    None,
                    fallback.duty(),
                    mqtt_channel,
                );

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
    sensor: &'static ZoneSensor,
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

    // Index of this zone's Matter state (its position in ZONE_CONFIGS)
    let zone_index = zone_index_by_name(zone_name).unwrap_or(0);

    // Duty history and outage clock for the fallback cycle (RAM only)
    let mut fallback = FallbackController::new();
    let mut status = SensorStatusReporter::new();

    // Heartbeat slot for the watchdog supervisor. An unknown zone name never
    // beats, so the supervisor restarts the controller instead of trusting it.
    let beat_index = sensor_remote::zone_index_by_name(zone_name);

    // Control loop ticker
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));

    loop {
        ticker.next().await;

        // Tell the watchdog supervisor this loop is still turning over
        if let Some(index) = beat_index {
            failsafe::zone_beat(index);
        }

        // Read temperature from sensor
        let reading = sensor.read_temperature().await;

        // Readings unavailable (none fresh): keep cycling at the recent duty
        // for a bounded time. Any other outcome falls through to the match.
        if let Err(SensorError::Unavailable(_)) = reading {
            let decision = fallback.on_unavailable(Instant::now().as_secs(), &REMOTE_FALLBACK);
            if let FallbackDecision::Relay(on) = decision {
                // The Matter mode still rules: only heat while it is Heat.
                let (mode, _) = matter::get_control_target(zone_index);
                let heat_mode = mode != HeatingState::Off;
                let heat = on && heat_mode;
                let new_relay_state = if heat {
                    RelayState::On
                } else {
                    RelayState::Off
                };
                if new_relay_state != current_relay_state {
                    info!(
                        "[{}] Fallback: turning heating {}",
                        zone_name,
                        if heat { "ON" } else { "OFF" }
                    );
                }
                write_relay(
                    zone_name,
                    relay_pin,
                    &mut current_relay_state,
                    state,
                    new_relay_state,
                );

                // No reading: controllers show no temperature, and the
                // running state follows the relay.
                matter::update_matter_without_reading(zone_index, state.heating_active);
                status.report(
                    zone_name,
                    SensorStatus::Fallback,
                    Some(FallbackActivity::classify(fallback.duty(), heat_mode)),
                    fallback.duty(),
                    mqtt_channel,
                );
                continue;
            }
            if in_boot_grace(decision) {
                // Just booted and no sensor has reported yet: relay OFF, but
                // no `lost` status and no sensor error until the window ends.
                write_relay(
                    zone_name,
                    relay_pin,
                    &mut current_relay_state,
                    state,
                    RelayState::Off,
                );
                matter::update_matter_without_reading(zone_index, state.heating_active);
                status.note_waiting_for_first_reading(zone_name);
                continue;
            }
        }

        match reading {
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

                // Take the mode and target from Matter state, so a setpoint or
                // mode change made in a Matter controller drives the relay.
                // Any mode other than Heat yields HeatingState::Off.
                let (target_state, target_c) = matter::get_control_target(zone_index);
                if target_c != state.target_temp_c {
                    info!(
                        "[{}] Target changed: {:.1}C -> {:.1}C",
                        zone_name, state.target_temp_c, target_c
                    );
                    state.target_temp_c = target_c;
                }

                debug!(
                    "[{}] Temp: {:.1}C, Target: {:.1}C, Heating: {}",
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    if state.heating_active { "ON" } else { "OFF" }
                );

                // Apply hysteresis control
                let new_relay_state = apply_hysteresis_control(
                    reading.celsius,
                    state.target_temp_c,
                    target_state,
                    current_relay_state,
                    hysteresis,
                );

                // Update relay if state changed
                if new_relay_state != current_relay_state {
                    info!(
                        "[{}] Turning heating {} (temp: {:.1}C, target: {:.1}C)",
                        zone_name,
                        if new_relay_state == RelayState::On {
                            "ON"
                        } else {
                            "OFF"
                        },
                        reading.celsius,
                        state.target_temp_c
                    );
                    write_relay(
                        zone_name,
                        relay_pin,
                        &mut current_relay_state,
                        state,
                        new_relay_state,
                    );
                }

                // Fold this pass into the duty history the fallback runs on
                fallback.on_reading(
                    current_relay_state == RelayState::On,
                    CONTROL_INTERVAL_SECS as u32,
                );
                status.report(
                    zone_name,
                    SensorStatus::Ok,
                    None,
                    fallback.duty(),
                    mqtt_channel,
                );

                // Sync temperature and running state to Matter (so
                // HomeKit/Google/Alexa can see them). Done after the relay
                // update so the running state is current.
                matter::update_matter_from_control_loop(
                    zone_index,
                    reading.celsius,
                    state.heating_active,
                );

                // Publish temperature to MQTT after the relay is updated; never
                // waits, so a down broker cannot stall control.
                if let Some(msg) = build_temperature_message(
                    zone_name,
                    reading.celsius,
                    state.target_temp_c,
                    state.heating_active,
                    sensor.sensors_used(),
                ) {
                    queue_mqtt(mqtt_channel, msg);
                }
            }
            Err(e) => {
                // Not covered by fallback: other sensor errors, no history
                // since boot, or the fallback time limit has passed.
                status.report(
                    zone_name,
                    SensorStatus::Lost,
                    None,
                    fallback.duty(),
                    mqtt_channel,
                );

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

                // Tell Matter there is no reading, so controllers show no
                // temperature and the running state follows the relay.
                matter::update_matter_without_reading(zone_index, state.heating_active);
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
    wifi: esp_hal::peripherals::WIFI<'static>,
    bt: esp_hal::peripherals::BT<'static>,
    rng: esp_hal::peripherals::RNG<'static>,
    adc1: esp_hal::peripherals::ADC1<'static>,
    flash: esp_hal::peripherals::FLASH<'static>,
    user_task: matter::MqttBridge,
) {
    // Initialize flash storage for persistence
    // BlockingAsync wraps sync flash traits to async (required by sequential-storage)
    let flash = BlockingAsync::new(FlashStorage::new(flash));
    let mut store = FlashKvBlobStore::new(flash, PERSIST_FLASH_RANGE);

    // Load persisted zone states before Matter starts
    load_all_zones(&mut store).await;

    // Run Matter with flash-backed persistence
    matter_task(wifi, bt, rng, adc1, store, user_task).await
}

#[cfg(feature = "external-ble")]
#[embassy_executor::task]
async fn matter_main_task_dual_radio(
    wifi: esp_hal::peripherals::WIFI<'static>,
    uart_rx: esp_hal::uart::UartRx<'static, esp_hal::Async>,
    uart_tx: esp_hal::uart::UartTx<'static, esp_hal::Async>,
    rng: esp_hal::peripherals::RNG<'static>,
    adc1: esp_hal::peripherals::ADC1<'static>,
    flash: esp_hal::peripherals::FLASH<'static>,
    user_task: matter::MqttBridge,
) {
    // Initialize flash storage for persistence
    // BlockingAsync wraps sync flash traits to async (required by sequential-storage)
    let flash = BlockingAsync::new(FlashStorage::new(flash));
    let mut store = FlashKvBlobStore::new(flash, PERSIST_FLASH_RANGE);

    // Load persisted zone states before Matter starts
    load_all_zones(&mut store).await;

    // Run Matter with flash-backed persistence
    let driver = DualRadioDriver::new(wifi, uart_rx, uart_tx);
    matter_task_dual_radio(driver, rng, adc1, store, user_task).await
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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
    sensor: &'static ZoneSensor,
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

    // Drive every relay pin HIGH (OFF, active-low) before anything else can
    // go wrong or block: network bring-up below can wait indefinitely.
    let relay1 = if NUM_ZONES >= 1 {
        Some(PIN_1.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO0,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };
    let relay2 = if NUM_ZONES >= 2 {
        Some(PIN_2.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO1,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };
    let relay3 = if NUM_ZONES >= 3 {
        Some(PIN_3.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO2,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };
    let relay4 = if NUM_ZONES >= 4 {
        Some(PIN_4.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO3,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };
    let relay5 = if NUM_ZONES >= 5 {
        Some(PIN_5.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO10,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };
    let relay6 = if NUM_ZONES >= 6 {
        Some(PIN_6.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO11,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };
    let relay7 = if NUM_ZONES >= 7 {
        Some(PIN_7.init(Esp32GpioPin::new(Output::new(
            peripherals.GPIO22,
            Level::High,
            OutputConfig::default(),
        ))))
    } else {
        None
    };

    // RTC watchdog handle; armed by the watchdog task once the zones run
    let rtc = Rtc::new(peripherals.RTC_TIMER);

    // Initialize heap allocator (72KB)
    // Note: rs-matter-embassy example uses 100KB, but ESP32-C6 has limited RAM
    // and our 7 zone tasks + MQTT need memory too
    esp_alloc::heap_allocator!(size: 73728);

    // Initialize esp-rtos scheduler with timer and software interrupt
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    println!("====================================");
    info!("====================================");
    info!("EmberHeat ESP32-C6 Thermostat v0.3.0");
    info!("Multi-Zone Controller");
    info!("====================================");
    info!("Zones: {}", NUM_ZONES);
    info!(
        "Sensors: {}",
        match SENSOR_SOURCE {
            SensorSource::Mock => "Mock",
            SensorSource::Mqtt => "MQTT",
        }
    );
    info!("Control interval: {} seconds", CONTROL_INTERVAL_SECS);
    // Shows whether the last restart was a panic (software reset), the
    // watchdog, or a normal power-up
    info!("Reset reason: {:?}", esp_hal::system::reset_reason());

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
        spawner.spawn(mqtt_task(stack, mqtt_config).expect("Failed to spawn MQTT task"));
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
        let mqtt_bridge = matter::MqttBridge::new(mqtt_config, &MQTT_CHANNEL);

        // Mock source: sync the placeholder temperature to Matter state BEFORE
        // spawning the Matter task, so HomeKit sees a valid LocalTemperature
        // during commissioning (otherwise it may cache "not supported" status
        // from NULL temperature).
        // Mqtt source: a zone shows no temperature (NULL) and heating OFF
        // until its first remote reading, so no placeholder is seeded.
        match SENSOR_SOURCE {
            SensorSource::Mock => {
                for zone_index in 0..NUM_ZONES {
                    matter::update_matter_from_control_loop(zone_index, MOCK_TEMPERATURE_C, false);
                }
                info!(
                    "Initial temperature ({:.1}C) synced to Matter state for {} zones",
                    MOCK_TEMPERATURE_C, NUM_ZONES
                );
            }
            SensorSource::Mqtt => {
                for zone_index in 0..NUM_ZONES {
                    matter::update_matter_without_reading(zone_index, false);
                }
                info!(
                    "No initial temperature for {} zones until remote readings arrive",
                    NUM_ZONES
                );
            }
        }

        // Spawn Matter task (handles WiFi + MQTT)
        spawner.spawn(
            matter_main_task(
                peripherals.WIFI,
                peripherals.BT,
                peripherals.RNG,
                peripherals.ADC1,
                peripherals.FLASH,
                mqtt_bridge,
            )
            .expect("Failed to spawn Matter task"),
        );
        info!("Matter task spawned");
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

        // Initialize UART for HCI transport to ESP32-H2 with hardware flow control
        // Use a low FIFO threshold to trigger reads before the buffer overflows
        let uart_config = UartConfig::default()
            .with_baudrate(HCI_UART_BAUDRATE)
            .with_rx(RxConfig::default().with_fifo_full_threshold(32))
            // Assigning the RTS/CTS pins is not enough: without this the C6 holds
            // RTS high ("not ready") and the H2 never sends its HCI replies.
            .with_hw_flow_ctrl(esp_hal::uart::HwFlowControl {
                cts: esp_hal::uart::CtsConfig::Enabled,
                rts: esp_hal::uart::RtsConfig::Enabled(HCI_UART_RTS_THRESHOLD),
            });
        let uart = Uart::new(peripherals.UART1, uart_config)
            .expect("Failed to initialize HCI UART")
            .with_rx(peripherals.GPIO19) // RX (H2 TX -> C6 RX)
            .with_tx(peripherals.GPIO18) // TX (C6 TX -> H2 RX)
            .with_rts(peripherals.GPIO6) // RTS (C6 RTS -> H2 CTS)
            .with_cts(peripherals.GPIO7) // CTS (C6 CTS <- H2 RTS)
            .into_async();
        let (uart_rx, uart_tx) = uart.split();
        info!("HCI UART initialized");

        // Restart the H2 now that the UART lines are stable (see HCI_H2_RESET_PIN).
        // Open-drain: only ever pulls the line low; the H2 board's own pull-up
        // releases it, so the C6 never fights the H2's reset circuit.
        let mut h2_reset = Output::new(
            peripherals.GPIO23,
            Level::Low,
            OutputConfig::default().with_drive_mode(esp_hal::gpio::DriveMode::OpenDrain),
        );
        embassy_time::Timer::after(Duration::from_millis(config::HCI_H2_RESET_PULSE_MS)).await;
        h2_reset.set_high();
        embassy_time::Timer::after(Duration::from_millis(config::HCI_H2_BOOT_MS)).await;
        // Keep the pin configured (released) for the life of the firmware
        core::mem::forget(h2_reset);
        info!("ESP32-H2 reset via GPIO{}", config::HCI_H2_RESET_PIN);

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
        let mqtt_bridge = matter::MqttBridge::new(mqtt_config, &MQTT_CHANNEL);

        // Mock source: sync the placeholder temperature to Matter state BEFORE
        // spawning the Matter task, so HomeKit sees a valid LocalTemperature
        // during commissioning (otherwise it may cache "not supported" status
        // from NULL temperature).
        // Mqtt source: a zone shows no temperature (NULL) and heating OFF
        // until its first remote reading, so no placeholder is seeded.
        match SENSOR_SOURCE {
            SensorSource::Mock => {
                for zone_index in 0..NUM_ZONES {
                    matter::update_matter_from_control_loop(zone_index, MOCK_TEMPERATURE_C, false);
                }
                info!(
                    "Initial temperature ({:.1}C) synced to Matter state for {} zones",
                    MOCK_TEMPERATURE_C, NUM_ZONES
                );
            }
            SensorSource::Mqtt => {
                for zone_index in 0..NUM_ZONES {
                    matter::update_matter_without_reading(zone_index, false);
                }
                info!(
                    "No initial temperature for {} zones until remote readings arrive",
                    NUM_ZONES
                );
            }
        }

        // Spawn dual-radio Matter task (C6 WiFi + H2 BLE)
        spawner.spawn(
            matter_main_task_dual_radio(
                peripherals.WIFI,
                uart_rx,
                uart_tx,
                peripherals.RNG,
                peripherals.ADC1,
                peripherals.FLASH,
                mqtt_bridge,
            )
            .expect("Failed to spawn Matter dual-radio task"),
        );
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
    let (sensor1, pin1, state1) = if let Some(pin) = relay1 {
        info!("  Initializing Zone1 GPIO{}...", ZONE_CONFIGS[0].relay_pin);
        let sensor = SENSOR_1.init(ZoneSensor::for_zone(0));
        let state = STATE_1.init(ZoneState::with_target(ZONE_CONFIGS[0].default_target_c));
        info!("  Zone1 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 2: GPIO1
    let (sensor2, pin2, state2) = if let Some(pin) = relay2 {
        info!("  Initializing Zone2 GPIO{}...", ZONE_CONFIGS[1].relay_pin);
        let sensor = SENSOR_2.init(ZoneSensor::for_zone(1));
        let state = STATE_2.init(ZoneState::with_target(ZONE_CONFIGS[1].default_target_c));
        info!("  Zone2 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 3: GPIO2
    let (sensor3, pin3, state3) = if let Some(pin) = relay3 {
        info!("  Initializing Zone3 GPIO{}...", ZONE_CONFIGS[2].relay_pin);
        let sensor = SENSOR_3.init(ZoneSensor::for_zone(2));
        let state = STATE_3.init(ZoneState::with_target(ZONE_CONFIGS[2].default_target_c));
        info!("  Zone3 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 4: GPIO3
    let (sensor4, pin4, state4) = if let Some(pin) = relay4 {
        info!("  Initializing Zone4 GPIO{}...", ZONE_CONFIGS[3].relay_pin);
        let sensor = SENSOR_4.init(ZoneSensor::for_zone(3));
        let state = STATE_4.init(ZoneState::with_target(ZONE_CONFIGS[3].default_target_c));
        info!("  Zone4 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 5: GPIO10
    let (sensor5, pin5, state5) = if let Some(pin) = relay5 {
        info!("  Initializing Zone5 GPIO{}...", ZONE_CONFIGS[4].relay_pin);
        let sensor = SENSOR_5.init(ZoneSensor::for_zone(4));
        let state = STATE_5.init(ZoneState::with_target(ZONE_CONFIGS[4].default_target_c));
        info!("  Zone5 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 6: GPIO11
    let (sensor6, pin6, state6) = if let Some(pin) = relay6 {
        info!("  Initializing Zone6 GPIO{}...", ZONE_CONFIGS[5].relay_pin);
        let sensor = SENSOR_6.init(ZoneSensor::for_zone(5));
        let state = STATE_6.init(ZoneState::with_target(ZONE_CONFIGS[5].default_target_c));
        info!("  Zone6 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    // Zone 7: GPIO22
    let (sensor7, pin7, state7) = if let Some(pin) = relay7 {
        info!("  Initializing Zone7 GPIO{}...", ZONE_CONFIGS[6].relay_pin);
        let sensor = SENSOR_7.init(ZoneSensor::for_zone(6));
        let state = STATE_7.init(ZoneState::with_target(ZONE_CONFIGS[6].default_target_c));
        info!("  Zone7 initialized");
        (Some(sensor), Some(pin), Some(state))
    } else {
        (None, None, None)
    };

    info!("All relay GPIO pins initialized (OFF state)");
    info!("Zone sensors initialized");

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
            .spawn(zone1_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone1 task"));
        info!("  Zone1 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor2, pin2, state2) {
        spawner
            .spawn(zone2_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone2 task"));
        info!("  Zone2 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor3, pin3, state3) {
        spawner
            .spawn(zone3_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone3 task"));
        info!("  Zone3 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor4, pin4, state4) {
        spawner
            .spawn(zone4_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone4 task"));
        info!("  Zone4 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor5, pin5, state5) {
        spawner
            .spawn(zone5_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone5 task"));
        info!("  Zone5 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor6, pin6, state6) {
        spawner
            .spawn(zone6_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone6 task"));
        info!("  Zone6 task spawned");
    }

    if let (Some(sensor), Some(pin), Some(state)) = (sensor7, pin7, state7) {
        spawner
            .spawn(zone7_task(sensor, pin, state, hysteresis).expect("Failed to spawn Zone7 task"));
        info!("  Zone7 task spawned");
    }

    // From here on a hung zone loop or a stuck executor restarts the chip
    spawner.spawn(failsafe::watchdog_task(rtc).expect("Failed to spawn watchdog task"));

    info!("All {} zone control tasks running", NUM_ZONES);
    info!("====================================");

    // Keep main task alive (zones run independently)
    loop {
        embassy_time::Timer::after(Duration::from_secs(60)).await;
    }
}
