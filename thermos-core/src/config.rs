//! Configuration types for thermostat system
//!
//! This module contains platform-agnostic configuration types.
//! Platform-specific implementations handle configuration loading.

use serde::{Deserialize, Serialize};

/// Configuration for a single heating zone
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneConfig {
    /// GPIO pin number for relay control
    pub relay_pin: u8,

    /// GPIO pin number for temperature sensor power
    pub temp_pin: u8,

    /// Temperature sensor ID (e.g., "00000ab12345" for 1-Wire, "0x48" for I2C)
    #[cfg(feature = "std")]
    pub temp_id: String,
    #[cfg(not(feature = "std"))]
    pub temp_id: heapless::String<64>,

    /// Optional fallback sensor URL or identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg(feature = "std")]
    pub extra_sensor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[cfg(not(feature = "std"))]
    pub extra_sensor: Option<heapless::String<128>>,
}

/// Hysteresis configuration
#[derive(Debug, Clone, Copy)]
pub struct HysteresisConfig {
    /// Upper threshold in Celsius (turn OFF when temp >= target + this value)
    pub upper_threshold_c: f32,

    /// Lower threshold in Celsius (turn ON when temp <= target + this value)
    pub lower_threshold_c: f32,
}

impl Default for HysteresisConfig {
    fn default() -> Self {
        Self {
            upper_threshold_c: 0.5,  // Turn OFF at target + 0.5°C
            lower_threshold_c: -0.5, // Turn ON at target - 0.5°C
        }
    }
}

impl HysteresisConfig {
    /// Create a new hysteresis configuration
    ///
    /// # Arguments
    ///
    /// * `upper_threshold_c` - Upper threshold in Celsius
    /// * `lower_threshold_c` - Lower threshold in Celsius
    pub fn new(upper_threshold_c: f32, lower_threshold_c: f32) -> Self {
        Self {
            upper_threshold_c,
            lower_threshold_c,
        }
    }

    /// Create configuration with symmetric dead band
    ///
    /// # Arguments
    ///
    /// * `dead_band_c` - Dead band width in Celsius (e.g., 0.5 creates ±0.5°C band)
    pub fn with_dead_band(dead_band_c: f32) -> Self {
        Self {
            upper_threshold_c: dead_band_c,
            lower_threshold_c: -dead_band_c,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hysteresis_default() {
        let config = HysteresisConfig::default();
        assert_eq!(config.upper_threshold_c, 0.5);
        assert_eq!(config.lower_threshold_c, -0.5);
    }

    #[test]
    fn test_hysteresis_with_dead_band() {
        let config = HysteresisConfig::with_dead_band(1.0);
        assert_eq!(config.upper_threshold_c, 1.0);
        assert_eq!(config.lower_threshold_c, -1.0);
    }

    #[test]
    fn test_zone_config_serialization() {
        let zone = ZoneConfig {
            relay_pin: 23,
            temp_pin: 4,
            temp_id: "test123".to_string(),
            extra_sensor: Some("http://example.com".to_string()),
        };

        let json = serde_json::to_string(&zone).unwrap();
        let deserialized: ZoneConfig = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.relay_pin, zone.relay_pin);
        assert_eq!(deserialized.temp_id, zone.temp_id);
    }
}
