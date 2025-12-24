//! Hardware Abstraction Layer (HAL)
//!
//! This module provides platform-agnostic abstractions for hardware operations.
//! It enables the thermostat code to run on multiple platforms:
//!
//! - **Linux** (Raspberry Pi): Uses `rppal` for GPIO
//! - **ESP32** (future): Will use `embassy-esp` for GPIO
//!
//! # Architecture
//!
//! The HAL defines traits that abstract hardware operations:
//!
//! - [`OutputPin`]: Digital output control (relays, LEDs)
//! - [`Platform`]: Platform-specific initialization and resource management
//!
//! Platform-specific implementations are selected at compile time using
//! feature flags and conditional compilation.
//!
//! **Note**: This HAL is currently prepared for future ESP32 bare-metal support.
//! The gpio module currently uses its own platform abstraction but will migrate
//! to use these traits when ESP32 support is added.

#![allow(dead_code)] // HAL traits prepared for future ESP32 port

/// Digital output pin trait
///
/// Abstracts a GPIO pin configured as a digital output.
/// Used for controlling relays and other on/off devices.
pub trait OutputPin {
    /// Set the pin high (logic 1)
    fn set_high(&mut self);

    /// Set the pin low (logic 0)
    fn set_low(&mut self);

    /// Check if the pin is currently set low
    fn is_set_low(&self) -> bool;

    /// Check if the pin is currently set high
    fn is_set_high(&self) -> bool {
        !self.is_set_low()
    }
}

/// Temperature sensor trait for HAL
///
/// Abstracts temperature sensor hardware across platforms.
/// Different platforms may use different sensor interfaces:
/// - Linux: 1-Wire via sysfs
/// - ESP32: Direct 1-Wire bit-banging or I2C sensors
pub trait TemperatureSensorHal: Send + Sync {
    /// Read temperature in millidegrees Celsius
    ///
    /// Returns the temperature multiplied by 1000 (e.g., 21500 = 21.5°C)
    fn read_millidegrees(&self) -> Result<i32, HalError>;

    /// Get sensor identifier
    fn id(&self) -> &str;

    /// Check if sensor is available/connected
    fn is_available(&self) -> bool;
}

/// HAL error types
#[derive(Debug, Clone)]
pub enum HalError {
    /// GPIO initialization failed
    GpioInitError(String),

    /// Pin not available or already in use
    PinUnavailable(u8),

    /// Sensor read error
    SensorError(String),

    /// Timeout during operation
    Timeout,

    /// Platform not supported for this operation
    Unsupported(String),
}

impl std::fmt::Display for HalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HalError::GpioInitError(msg) => write!(f, "GPIO initialization error: {}", msg),
            HalError::PinUnavailable(pin) => write!(f, "Pin {} not available", pin),
            HalError::SensorError(msg) => write!(f, "Sensor error: {}", msg),
            HalError::Timeout => write!(f, "Operation timed out"),
            HalError::Unsupported(msg) => write!(f, "Unsupported: {}", msg),
        }
    }
}

impl std::error::Error for HalError {}

/// Platform capabilities and initialization
///
/// Each supported platform implements this trait to provide
/// platform-specific initialization and resource access.
pub trait Platform {
    /// Output pin type for this platform
    type Pin: OutputPin;

    /// Get a GPIO pin as an output
    ///
    /// # Arguments
    ///
    /// * `pin_number` - Platform-specific pin identifier
    ///
    /// # Returns
    ///
    /// An output pin instance or error if unavailable
    fn get_output_pin(&self, pin_number: u8) -> Result<Self::Pin, HalError>;

    /// Platform name for logging
    fn name(&self) -> &'static str;
}

// Re-export platform-specific implementations
#[cfg(target_os = "linux")]
mod linux;

#[cfg(target_os = "linux")]
pub use linux::LinuxPlatform;

// Stub implementation for non-Linux (development/testing)
#[cfg(not(target_os = "linux"))]
mod stub;

#[cfg(not(target_os = "linux"))]
pub use stub::StubPlatform;

/// Type alias for the current platform
#[cfg(target_os = "linux")]
pub type CurrentPlatform = LinuxPlatform;

#[cfg(not(target_os = "linux"))]
pub type CurrentPlatform = StubPlatform;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hal_error_display() {
        let err = HalError::GpioInitError("test".to_string());
        assert!(err.to_string().contains("GPIO initialization error"));

        let err = HalError::PinUnavailable(23);
        assert!(err.to_string().contains("23"));

        let err = HalError::Timeout;
        assert!(err.to_string().contains("timed out"));
    }
}
