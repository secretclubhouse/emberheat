//! Core type definitions for thermostat system
//!
//! This module contains platform-agnostic type definitions that are shared
//! across all thermostat implementations.

use serde::{Deserialize, Serialize};

/// Heating state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeatingState {
    /// Heating is off
    Off = 0,
    /// Heating is on
    Heat = 1,
}

impl From<u8> for HeatingState {
    fn from(value: u8) -> Self {
        match value {
            0 => HeatingState::Off,
            _ => HeatingState::Heat,
        }
    }
}

impl From<HeatingState> for u8 {
    fn from(state: HeatingState) -> Self {
        state as u8
    }
}

/// Relay state enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayState {
    /// Relay is off
    Off,
    /// Relay is on
    On,
}

impl From<HeatingState> for RelayState {
    fn from(state: HeatingState) -> Self {
        match state {
            HeatingState::Off => RelayState::Off,
            HeatingState::Heat => RelayState::On,
        }
    }
}

/// Temperature reading result
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TemperatureReading {
    /// Temperature in Celsius
    pub celsius: f32,
    /// Temperature in Fahrenheit
    pub fahrenheit: f32,
}

impl TemperatureReading {
    /// Create a new temperature reading from Celsius
    pub fn from_celsius(celsius: f32) -> Self {
        Self {
            celsius,
            fahrenheit: celsius * 9.0 / 5.0 + 32.0,
        }
    }

    /// Create a new temperature reading from Fahrenheit
    pub fn from_fahrenheit(fahrenheit: f32) -> Self {
        Self {
            celsius: (fahrenheit - 32.0) * 5.0 / 9.0,
            fahrenheit,
        }
    }

    /// Validate temperature is within normal range
    ///
    /// Valid range: 0°C to 37°C (32°F to 98.6°F)
    pub fn is_valid(&self) -> bool {
        self.celsius >= MIN_VALID_TEMP_C && self.celsius <= MAX_VALID_TEMP_C
    }
}

/// Minimum valid temperature in Celsius.
///
/// Below freezing the heating has already failed, so a reading under this
/// value is treated as a sensor fault rather than a real room temperature.
pub const MIN_VALID_TEMP_C: f32 = 0.0;

/// Maximum valid temperature in Celsius
pub const MAX_VALID_TEMP_C: f32 = 37.0;

/// Default temperature on error (21.1°C / 70°F)
pub const DEFAULT_TEMP_C: f32 = 21.1;

/// Thermostat state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermostatState {
    /// GPIO relay pin
    pub relay_pin: u8,

    /// GPIO temperature sensor pin
    pub temp_pin: u8,

    /// Temperature sensor ID
    #[cfg(feature = "std")]
    pub temp_id: String,
    #[cfg(not(feature = "std"))]
    pub temp_id: heapless::String<64>,

    /// Target temperature in Celsius
    pub target_temp: f32,

    /// Target heating state (0=off, 1=heat)
    pub target_state: u8,

    /// Current heating state (0=off, 1=heat)
    pub current_state: u8,

    /// Last update timestamp (Unix timestamp in seconds)
    #[cfg(feature = "std")]
    #[serde(with = "chrono::serde::ts_seconds")]
    pub last_updated: chrono::DateTime<chrono::Utc>,

    /// Last update timestamp (Unix timestamp in seconds) - no_std version
    #[cfg(not(feature = "std"))]
    pub last_updated: u64,
}

impl Default for ThermostatState {
    fn default() -> Self {
        Self {
            relay_pin: 0,
            temp_pin: 0,
            #[cfg(feature = "std")]
            temp_id: String::new(),
            #[cfg(not(feature = "std"))]
            temp_id: heapless::String::new(),
            target_temp: DEFAULT_TEMP_C,
            target_state: HeatingState::Off.into(),
            current_state: HeatingState::Off.into(),
            #[cfg(feature = "std")]
            last_updated: chrono::Utc::now(),
            #[cfg(not(feature = "std"))]
            last_updated: 0,
        }
    }
}

impl ThermostatState {
    /// Update timestamp to current time
    #[cfg(feature = "std")]
    pub fn touch(&mut self) {
        self.last_updated = chrono::Utc::now();
    }

    /// Update timestamp to current time (no_std - requires manual timestamp)
    #[cfg(not(feature = "std"))]
    pub fn touch(&mut self) {
        // In no_std, timestamp must be set manually by platform code
        // that has access to a real-time clock
    }

    /// Set timestamp manually (for no_std platforms)
    #[cfg(not(feature = "std"))]
    pub fn set_timestamp(&mut self, timestamp: u64) {
        self.last_updated = timestamp;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heating_state_conversion() {
        assert_eq!(HeatingState::from(0), HeatingState::Off);
        assert_eq!(HeatingState::from(1), HeatingState::Heat);
        assert_eq!(u8::from(HeatingState::Off), 0);
        assert_eq!(u8::from(HeatingState::Heat), 1);
    }

    #[test]
    fn test_relay_state_conversion() {
        assert_eq!(RelayState::from(HeatingState::Off), RelayState::Off);
        assert_eq!(RelayState::from(HeatingState::Heat), RelayState::On);
    }

    #[test]
    fn test_temperature_reading_conversion() {
        let reading = TemperatureReading::from_celsius(20.0);
        assert_eq!(reading.celsius, 20.0);
        assert_eq!(reading.fahrenheit, 68.0);

        let reading = TemperatureReading::from_celsius(0.0);
        assert_eq!(reading.fahrenheit, 32.0);

        let reading = TemperatureReading::from_fahrenheit(212.0);
        assert_eq!(reading.celsius, 100.0);
    }

    #[test]
    fn test_temperature_validation() {
        let valid = TemperatureReading::from_celsius(20.0);
        assert!(valid.is_valid());

        assert!(TemperatureReading::from_celsius(0.0).is_valid());
        assert!(TemperatureReading::from_celsius(2.0).is_valid());
        assert!(!TemperatureReading::from_celsius(-0.1).is_valid());

        let too_hot = TemperatureReading::from_celsius(38.0);
        assert!(!too_hot.is_valid());
    }

    #[test]
    fn test_thermostat_state_default() {
        let state = ThermostatState::default();
        assert_eq!(state.target_temp, DEFAULT_TEMP_C);
        assert_eq!(state.target_state, 0);
        assert_eq!(state.current_state, 0);
    }

    #[test]
    fn test_thermostat_state_serialization() {
        let state = ThermostatState {
            relay_pin: 23,
            temp_pin: 4,
            temp_id: "00000ab12345".to_string(),
            target_temp: 22.0,
            target_state: 1,
            current_state: 1,
            last_updated: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: ThermostatState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.relay_pin, state.relay_pin);
        assert_eq!(deserialized.target_temp, state.target_temp);
    }
}
