//! # Thermos Core
//!
//! Platform-agnostic thermostat control library.
//!
//! This library provides the core control logic, trait definitions, and types
//! for building a thermostat system. It can be used on any platform (Linux,
//! ESP32, etc.) by implementing the platform-specific traits.
//!
//! ## Architecture
//!
//! The library is organized into several modules:
//!
//! - `types`: Core type definitions (HeatingState, TemperatureReading, etc.)
//! - `sensor`: Temperature sensor trait and utilities
//! - `gpio`: GPIO abstraction traits for relay control
//! - `persistence`: State storage trait for persisting configuration
//! - `control`: Hysteresis-based control logic
//! - `config`: Configuration types
//!
//! ## Example
//!
//! ```rust
//! use thermos_core::control::apply_hysteresis_control;
//! use thermos_core::config::HysteresisConfig;
//! use thermos_core::types::{HeatingState, RelayState};
//!
//! let hysteresis = HysteresisConfig::default();
//! let current_temp = 20.0; // °C
//! let target_temp = 21.1;  // °C
//!
//! let new_state = apply_hysteresis_control(
//!     current_temp,
//!     target_temp,
//!     HeatingState::Heat,
//!     RelayState::Off,
//!     &hysteresis,
//! );
//!
//! assert_eq!(new_state, RelayState::On); // Too cold, turn heating on
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(not(feature = "std"))]
extern crate alloc;

pub mod config;
pub mod control;
pub mod gpio;
pub mod persistence;
pub mod sensor;
pub mod types;

// Re-export commonly used types for convenience
pub use config::{HysteresisConfig, ZoneConfig};
pub use control::{apply_hysteresis_control, validate_temperature, ControlError};
pub use gpio::{GpioError, GpioManager, OutputPin};
pub use persistence::{PersistenceError, StateStorage};
pub use sensor::{SensorError, TemperatureSensor, SENSOR_READ_TIMEOUT_SECS};
pub use types::{
    HeatingState, RelayState, TemperatureReading, ThermostatState, DEFAULT_TEMP_C,
    MAX_VALID_TEMP_C, MIN_VALID_TEMP_C,
};

/// Library version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_re_exports() {
        // Test that re-exports are accessible
        let _state = HeatingState::Off;
        let _relay = RelayState::Off;
        let _reading = TemperatureReading::from_celsius(20.0);
        let _config = HysteresisConfig::default();
    }
}
