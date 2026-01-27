//! Zone configuration types for multi-zone thermostat support
//!
//! This module defines the configuration and state types for managing
//! multiple heating zones on the ESP32 thermostat.

use heapless::String as HeaplessString;

/// Maximum number of zones supported
pub const MAX_ZONES: usize = 7;

/// Default target temperature in Celsius (70°F)
const DEFAULT_TARGET_TEMP_C: f32 = 21.1;

/// Sensor type configuration for a zone
///
/// Each zone can use one of several sensor types for temperature readings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SensorType {
    /// DS18B20 1-Wire sensor with 64-bit ROM address
    Ds18b20([u8; 8]),

    /// BME280 I2C sensor with I2C address
    Bme280(u8),

    /// Mock sensor for testing (returns fixed temperature)
    Mock(f32),
}

/// Configuration for a single heating zone
///
/// Defines the hardware configuration for a zone including
/// relay pin, sensor type, and default temperature target.
#[derive(Debug, Clone)]
pub struct ZoneConfig {
    /// Human-readable zone name (max 32 characters)
    pub name: HeaplessString<32>,

    /// GPIO pin number for relay control
    pub relay_pin: u8,

    /// Temperature sensor configuration
    pub sensor: SensorType,

    /// Default target temperature in Celsius
    pub default_target_c: f32,
}

impl ZoneConfig {
    /// Create a new zone configuration
    ///
    /// # Arguments
    /// * `name` - Zone name (will be truncated if > 32 chars)
    /// * `relay_pin` - GPIO pin for relay
    /// * `sensor` - Sensor type and configuration
    /// * `default_target_c` - Default target temperature
    pub fn new(name: &str, relay_pin: u8, sensor: SensorType, default_target_c: f32) -> Self {
        let mut zone_name = HeaplessString::new();
        // Truncate name if too long
        for c in name.chars().take(32) {
            let _ = zone_name.push(c);
        }

        Self {
            name: zone_name,
            relay_pin,
            sensor,
            default_target_c,
        }
    }
}

/// Runtime state for a single heating zone
///
/// Tracks the current temperature, target, heating status,
/// and error count for a zone during operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneState {
    /// Current temperature reading in Celsius
    pub current_temp_c: f32,

    /// Target temperature in Celsius
    pub target_temp_c: f32,

    /// Whether heating is currently active
    pub heating_active: bool,

    /// Count of consecutive sensor errors
    pub error_count: u32,
}

impl Default for ZoneState {
    fn default() -> Self {
        Self {
            current_temp_c: DEFAULT_TARGET_TEMP_C,
            target_temp_c: DEFAULT_TARGET_TEMP_C,
            heating_active: false,
            error_count: 0,
        }
    }
}

impl ZoneState {
    /// Create a new zone state with a specific target temperature
    pub fn with_target(target_temp_c: f32) -> Self {
        Self {
            target_temp_c,
            ..Default::default()
        }
    }

    /// Reset error count (called after successful sensor read)
    pub fn clear_errors(&mut self) {
        self.error_count = 0;
    }

    /// Increment error count (called after sensor read failure)
    pub fn increment_errors(&mut self) {
        self.error_count = self.error_count.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_max_zones() {
        assert_eq!(MAX_ZONES, 7);
    }

    #[test]
    fn test_sensor_type_ds18b20() {
        let addr = [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        let sensor = SensorType::Ds18b20(addr);
        if let SensorType::Ds18b20(a) = sensor {
            assert_eq!(a[0], 0x28); // DS18B20 family code
        } else {
            panic!("Expected Ds18b20 variant");
        }
    }

    #[test]
    fn test_sensor_type_bme280() {
        let sensor = SensorType::Bme280(0x76);
        if let SensorType::Bme280(addr) = sensor {
            assert_eq!(addr, 0x76);
        } else {
            panic!("Expected Bme280 variant");
        }
    }

    #[test]
    fn test_sensor_type_mock() {
        let sensor = SensorType::Mock(25.0);
        if let SensorType::Mock(temp) = sensor {
            assert_eq!(temp, 25.0);
        } else {
            panic!("Expected Mock variant");
        }
    }

    #[test]
    fn test_zone_config_new() {
        let config = ZoneConfig::new("Living Room", 4, SensorType::Bme280(0x76), 21.1);
        assert_eq!(config.name.as_str(), "Living Room");
        assert_eq!(config.relay_pin, 4);
        assert_eq!(config.default_target_c, 21.1);
    }

    #[test]
    fn test_zone_config_long_name_truncated() {
        let long_name = "This is a very long zone name that exceeds 32 characters";
        let config = ZoneConfig::new(long_name, 4, SensorType::Mock(20.0), 21.1);
        assert_eq!(config.name.len(), 32);
    }

    #[test]
    fn test_zone_state_default() {
        let state = ZoneState::default();
        assert_eq!(state.current_temp_c, 21.1);
        assert_eq!(state.target_temp_c, 21.1);
        assert!(!state.heating_active);
        assert_eq!(state.error_count, 0);
    }

    #[test]
    fn test_zone_state_with_target() {
        let state = ZoneState::with_target(23.0);
        assert_eq!(state.target_temp_c, 23.0);
        assert_eq!(state.current_temp_c, 21.1); // Default
        assert!(!state.heating_active);
    }

    #[test]
    fn test_zone_state_error_handling() {
        let mut state = ZoneState::default();

        state.increment_errors();
        assert_eq!(state.error_count, 1);

        state.increment_errors();
        assert_eq!(state.error_count, 2);

        state.clear_errors();
        assert_eq!(state.error_count, 0);
    }

    #[test]
    fn test_zone_state_error_count_saturating() {
        let mut state = ZoneState::default();
        state.error_count = u32::MAX;
        state.increment_errors();
        assert_eq!(state.error_count, u32::MAX); // Should not overflow
    }
}
