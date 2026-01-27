//! Zone controller with hysteresis control loop
//!
//! This module provides the core control loop for managing a single heating zone.
//! It reads temperature from a sensor, applies hysteresis-based control logic,
//! and controls a relay to maintain the target temperature.
//!
//! # Control Logic
//!
//! The controller uses hysteresis to prevent relay chatter:
//! - Turn ON heating when: temp <= target + lower_threshold (e.g., target - 0.5C)
//! - Turn OFF heating when: temp >= target + upper_threshold (e.g., target + 0.5C)
//! - Maintain current state when within the dead band
//!
//! # Relay Control
//!
//! Uses active-low logic for relay control:
//! - `set_low()` = relay ON (heating active)
//! - `set_high()` = relay OFF (heating inactive)
//!
//! # Example
//!
//! ```no_run
//! use thermos_esp32::controller::run_zone_control_loop;
//! use thermos_esp32::sensors::MockSensor;
//! use thermos_esp32::zone::ZoneState;
//! use thermos_core::HysteresisConfig;
//!
//! // Create sensor and relay pin
//! let sensor = MockSensor::new(20.0, "zone1");
//! let mut relay_pin = /* ESP32 GPIO pin */;
//! let mut state = ZoneState::with_target(21.1);
//! let hysteresis = HysteresisConfig::default();
//!
//! // Run the control loop (never returns)
//! run_zone_control_loop(
//!     "Zone1",
//!     sensor,
//!     &mut relay_pin,
//!     &mut state,
//!     &hysteresis,
//! ).await;
//! ```

use embassy_time::{Duration, Ticker};
use log::{debug, error, info, warn};
use thermos_core::{
    apply_hysteresis_control, HeatingState, HysteresisConfig, OutputPin, RelayState,
    TemperatureSensor,
};

use crate::zone::ZoneState;

/// Control loop interval in seconds
///
/// The control loop runs every 6 seconds, which provides a good balance between
/// responsiveness and avoiding excessive relay switching.
pub const CONTROL_INTERVAL_SECS: u64 = 6;

/// Maximum consecutive sensor errors before logging at error level
const MAX_ERROR_LOG_COUNT: u32 = 10;

/// Run the zone control loop
///
/// This is the main control loop for a single heating zone. It runs indefinitely,
/// reading temperature from the sensor, applying hysteresis control, and controlling
/// the relay to maintain the target temperature.
///
/// # Arguments
///
/// * `zone_name` - Human-readable name for logging
/// * `sensor` - Temperature sensor implementing `TemperatureSensor` trait
/// * `relay_pin` - GPIO pin for relay control implementing `OutputPin` trait
/// * `state` - Mutable reference to zone state (updated each iteration)
/// * `hysteresis` - Hysteresis configuration for control logic
///
/// # Returns
///
/// This function never returns (returns `!`). It runs the control loop indefinitely.
///
/// # Panics
///
/// Does not panic. All errors are logged and handled gracefully.
/// On sensor failure, heating is turned OFF as a safety measure.
///
/// # Safety
///
/// - Relay is initialized to OFF state on startup
/// - On any sensor failure, heating is immediately turned OFF
/// - All GPIO errors are logged but do not cause panics
pub async fn run_zone_control_loop<S, P>(
    zone_name: &str,
    sensor: S,
    relay_pin: &mut P,
    state: &mut ZoneState,
    hysteresis: &HysteresisConfig,
) -> !
where
    S: TemperatureSensor,
    P: OutputPin,
{
    info!(
        "[{}] Starting control loop (interval: {}s, target: {:.1}C)",
        zone_name, CONTROL_INTERVAL_SECS, state.target_temp_c
    );

    // Initialize relay to OFF state (HIGH = OFF for active-low)
    if let Err(e) = relay_pin.set_high() {
        error!("[{}] Failed to initialize relay to OFF: {:?}", zone_name, e);
    }

    // Track previous relay state to detect changes
    let mut current_relay_state = RelayState::Off;

    // Create ticker for control loop timing
    let mut ticker = Ticker::every(Duration::from_secs(CONTROL_INTERVAL_SECS));

    loop {
        ticker.next().await;

        // Read temperature from sensor
        let temp_result = sensor.read_temperature().await;

        match temp_result {
            Ok(reading) => {
                // Successful read - clear error count and update state
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

                // Determine target heating state (Heat if zone is enabled)
                // For now, we assume zone is always in Heat mode when enabled
                let target_state = HeatingState::Heat;

                // Apply hysteresis control logic
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
                // Sensor error - increment error count
                state.increment_errors();

                // Log at appropriate level based on error count
                if state.error_count <= MAX_ERROR_LOG_COUNT {
                    warn!(
                        "[{}] Sensor read failed (error #{}): {}",
                        zone_name, state.error_count, e
                    );
                } else if state.error_count == MAX_ERROR_LOG_COUNT + 1 {
                    error!(
                        "[{}] Sensor errors exceeded threshold, suppressing further warnings",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_control_interval() {
        assert_eq!(CONTROL_INTERVAL_SECS, 6);
    }

    #[test]
    fn test_max_error_log_count() {
        assert_eq!(MAX_ERROR_LOG_COUNT, 10);
    }
}
