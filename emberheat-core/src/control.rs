//! Platform-agnostic thermostat control logic
//!
//! This module contains the core hysteresis-based temperature control algorithm
//! that works on any platform (Linux, ESP32, etc.).

// Allow large error types - ControlError wraps other error types that use heapless::String<128>
// for descriptive error messages in a no_std context
#![allow(clippy::result_large_err)]

use crate::config::HysteresisConfig;
use crate::gpio::GpioError;
use crate::persistence::PersistenceError;
use crate::sensor::SensorError;
use crate::types::{HeatingState, RelayState, TemperatureReading};
use core::fmt;

/// Control loop errors
#[derive(Debug)]
pub enum ControlError {
    Sensor(SensorError),
    Gpio(GpioError),
    Persistence(PersistenceError),
    Logic(heapless::String<128>),
}

impl fmt::Display for ControlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ControlError::Sensor(e) => write!(f, "Sensor error: {}", e),
            ControlError::Gpio(e) => write!(f, "GPIO error: {}", e),
            ControlError::Persistence(e) => write!(f, "Persistence error: {}", e),
            ControlError::Logic(msg) => write!(f, "Control logic error: {}", msg),
        }
    }
}

impl From<SensorError> for ControlError {
    fn from(err: SensorError) -> Self {
        ControlError::Sensor(err)
    }
}

impl From<GpioError> for ControlError {
    fn from(err: GpioError) -> Self {
        ControlError::Gpio(err)
    }
}

impl From<PersistenceError> for ControlError {
    fn from(err: PersistenceError) -> Self {
        ControlError::Persistence(err)
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ControlError {}

/// Applies hysteresis control logic to determine relay state
///
/// # Arguments
///
/// * `current_temp_c` - Current temperature in Celsius
/// * `target_temp_c` - Target temperature in Celsius
/// * `target_state` - Target heating state (Heat or Off)
/// * `current_relay_state` - Current relay state
/// * `hysteresis` - Hysteresis configuration
///
/// # Returns
///
/// * `RelayState` - New relay state to apply
///
/// # Logic
///
/// When target_state is Heat:
/// - Turn ON when: temp <= target + lower_threshold (e.g., target - 0.5°C)
/// - Turn OFF when: temp >= target + upper_threshold (e.g., target + 0.5°C)
/// - Maintain current state when within dead band
///
/// When target_state is Off:
/// - Always turn OFF
pub fn apply_hysteresis_control(
    current_temp_c: f32,
    target_temp_c: f32,
    target_state: HeatingState,
    current_relay_state: RelayState,
    hysteresis: &HysteresisConfig,
) -> RelayState {
    if target_state == HeatingState::Heat {
        let lower_bound = target_temp_c + hysteresis.lower_threshold_c;
        let upper_bound = target_temp_c + hysteresis.upper_threshold_c;

        if current_temp_c <= lower_bound {
            RelayState::On
        } else if current_temp_c >= upper_bound {
            RelayState::Off
        } else {
            // Within dead band - maintain current state
            current_relay_state
        }
    } else {
        // Target is OFF - force relay off
        RelayState::Off
    }
}

/// Validates that a temperature reading is within reasonable bounds
///
/// # Arguments
///
/// * `reading` - Temperature reading to validate
///
/// # Returns
///
/// * `Result<(), ControlError>` - Ok if valid, error otherwise
pub fn validate_temperature(reading: &TemperatureReading) -> Result<(), ControlError> {
    if !reading.is_valid() {
        use heapless::String;
        let mut msg = String::<128>::new();
        use core::fmt::Write;
        let _ = write!(
            msg,
            "Temperature out of valid range: {:.1}°C",
            reading.celsius
        );
        Err(ControlError::Logic(msg))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hysteresis_turn_on() {
        let hysteresis = HysteresisConfig::default();

        // Current: 20.0°C, Target: 21.1°C
        // Lower bound: 21.1 - 0.5 = 20.6°C
        // 20.0 <= 20.6, should turn ON
        let new_state =
            apply_hysteresis_control(20.0, 21.1, HeatingState::Heat, RelayState::Off, &hysteresis);

        assert_eq!(new_state, RelayState::On);
    }

    #[test]
    fn test_hysteresis_turn_off() {
        let hysteresis = HysteresisConfig::default();

        // Current: 22.0°C, Target: 21.1°C
        // Upper bound: 21.1 + 0.5 = 21.6°C
        // 22.0 >= 21.6, should turn OFF
        let new_state =
            apply_hysteresis_control(22.0, 21.1, HeatingState::Heat, RelayState::On, &hysteresis);

        assert_eq!(new_state, RelayState::Off);
    }

    #[test]
    fn test_hysteresis_maintain_on() {
        let hysteresis = HysteresisConfig::default();

        // Current: 21.0°C, Target: 21.1°C
        // Lower: 20.6, Upper: 21.6
        // 21.0 is within dead band, should maintain current state
        let new_state =
            apply_hysteresis_control(21.0, 21.1, HeatingState::Heat, RelayState::On, &hysteresis);

        assert_eq!(new_state, RelayState::On);
    }

    #[test]
    fn test_hysteresis_maintain_off() {
        let hysteresis = HysteresisConfig::default();

        // Current: 21.0°C, Target: 21.1°C
        // Within dead band, should maintain current state
        let new_state =
            apply_hysteresis_control(21.0, 21.1, HeatingState::Heat, RelayState::Off, &hysteresis);

        assert_eq!(new_state, RelayState::Off);
    }

    #[test]
    fn test_target_off_forces_relay_off() {
        let hysteresis = HysteresisConfig::default();

        // Even if temp is low, target_state Off should force relay off
        let new_state =
            apply_hysteresis_control(15.0, 21.1, HeatingState::Off, RelayState::On, &hysteresis);

        assert_eq!(new_state, RelayState::Off);
    }

    #[test]
    fn test_edge_case_lower_boundary() {
        let hysteresis = HysteresisConfig::default();

        // Exactly at lower boundary: 21.1 - 0.5 = 20.6
        let new_state =
            apply_hysteresis_control(20.6, 21.1, HeatingState::Heat, RelayState::Off, &hysteresis);

        // Should turn ON (temp <= lower_bound)
        assert_eq!(new_state, RelayState::On);
    }

    #[test]
    fn test_edge_case_upper_boundary() {
        let hysteresis = HysteresisConfig::default();

        // Exactly at upper boundary: 21.1 + 0.5 = 21.6
        let new_state =
            apply_hysteresis_control(21.6, 21.1, HeatingState::Heat, RelayState::On, &hysteresis);

        // Should turn OFF (temp >= upper_bound)
        assert_eq!(new_state, RelayState::Off);
    }

    #[test]
    fn test_custom_hysteresis() {
        let hysteresis = HysteresisConfig::new(1.0, -1.0);

        // With wider dead band: ±1.0°C
        // Target: 20.0°C
        // Lower: 19.0°C, Upper: 21.0°C

        // Below lower bound
        let state =
            apply_hysteresis_control(18.5, 20.0, HeatingState::Heat, RelayState::Off, &hysteresis);
        assert_eq!(state, RelayState::On);

        // Within dead band
        let state =
            apply_hysteresis_control(20.0, 20.0, HeatingState::Heat, RelayState::On, &hysteresis);
        assert_eq!(state, RelayState::On);

        // Above upper bound
        let state =
            apply_hysteresis_control(21.5, 20.0, HeatingState::Heat, RelayState::On, &hysteresis);
        assert_eq!(state, RelayState::Off);
    }

    #[test]
    fn test_validate_temperature_valid() {
        let reading = TemperatureReading::from_celsius(20.0);
        assert!(validate_temperature(&reading).is_ok());
    }

    #[test]
    fn test_validate_temperature_too_cold() {
        let reading = TemperatureReading::from_celsius(-1.0);
        assert!(validate_temperature(&reading).is_err());
    }

    #[test]
    fn test_validate_temperature_too_hot() {
        let reading = TemperatureReading::from_celsius(40.0);
        assert!(validate_temperature(&reading).is_err());
    }
}
