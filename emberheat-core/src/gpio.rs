//! GPIO abstraction traits
//!
//! This module provides platform-agnostic GPIO traits that can be implemented
//! by different hardware backends (rppal for Raspberry Pi, esp-hal for ESP32, etc.).

// Allow large error types - we use heapless::String<128> for descriptive error messages
// in a no_std context where boxing isn't always available
#![allow(clippy::result_large_err)]

use crate::types::RelayState;
use core::fmt;

/// GPIO errors
#[derive(Debug)]
pub enum GpioError {
    InitError(heapless::String<128>),
    AccessError(heapless::String<128>),
    InvalidPin(u8),
    PinInUse(u8),
    PinNotFound(u8),
    HardwareError(heapless::String<128>),
}

impl fmt::Display for GpioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GpioError::InitError(msg) => write!(f, "Pin initialization failed: {}", msg),
            GpioError::AccessError(msg) => write!(f, "Pin access error: {}", msg),
            GpioError::InvalidPin(pin) => write!(f, "Invalid pin number: {}", pin),
            GpioError::PinInUse(pin) => write!(f, "GPIO pin {} is already in use", pin),
            GpioError::PinNotFound(pin) => write!(f, "GPIO pin {} not found", pin),
            GpioError::HardwareError(msg) => write!(f, "Hardware error: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for GpioError {}

/// Output pin trait
///
/// Represents a single GPIO output pin that can be set high or low.
/// This abstraction allows different hardware backends (rppal, esp-hal, etc.)
/// to provide compatible implementations.
pub trait OutputPin: Send + Sync {
    /// Set pin to high level
    fn set_high(&mut self) -> Result<(), GpioError>;

    /// Set pin to low level
    fn set_low(&mut self) -> Result<(), GpioError>;

    /// Check if pin is set to low level
    fn is_set_low(&self) -> Result<bool, GpioError>;

    /// Check if pin is set to high level
    fn is_set_high(&self) -> Result<bool, GpioError> {
        Ok(!self.is_set_low()?)
    }

    /// Set pin to a specific relay state
    ///
    /// Note: Relays are typically active-LOW (On = Low, Off = High)
    fn set_state(&mut self, state: RelayState) -> Result<(), GpioError> {
        match state {
            RelayState::On => self.set_low(), // Active LOW
            RelayState::Off => self.set_high(),
        }
    }

    /// Get current relay state
    fn get_state(&self) -> Result<RelayState, GpioError> {
        Ok(if self.is_set_low()? {
            RelayState::On // Active LOW
        } else {
            RelayState::Off
        })
    }
}

/// GPIO manager trait
///
/// Manages initialization and access to GPIO pins.
/// Platform-specific implementations handle pin allocation and cleanup.
pub trait GpioManager: Send + Sync {
    /// Output pin type for this platform
    type Pin: OutputPin;

    /// Initialize a relay output pin
    ///
    /// # Arguments
    ///
    /// * `pin_number` - GPIO pin number (platform-specific numbering)
    ///
    /// # Returns
    ///
    /// * `Result<Self::Pin>` - Initialized output pin or error
    fn init_relay(&mut self, pin_number: u8) -> Result<Self::Pin, GpioError>;

    /// Initialize VCC power control pin
    ///
    /// # Arguments
    ///
    /// * `pin_number` - GPIO pin number for VCC control
    ///
    /// # Returns
    ///
    /// * `Result<Self::Pin>` - Initialized output pin or error
    fn init_vcc(&mut self, pin_number: u8) -> Result<Self::Pin, GpioError>;

    /// Cleanup GPIO resources
    ///
    /// Should be called before program exit to properly release GPIO resources.
    fn cleanup(&mut self);
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock output pin for testing
    struct MockPin {
        state: bool, // true = high, false = low
    }

    impl OutputPin for MockPin {
        fn set_high(&mut self) -> Result<(), GpioError> {
            self.state = true;
            Ok(())
        }

        fn set_low(&mut self) -> Result<(), GpioError> {
            self.state = false;
            Ok(())
        }

        fn is_set_low(&self) -> Result<bool, GpioError> {
            Ok(!self.state)
        }
    }

    #[test]
    fn test_mock_pin_basic_operations() {
        let mut pin = MockPin { state: true };

        assert!(pin.is_set_high().unwrap());
        assert!(!pin.is_set_low().unwrap());

        pin.set_low().unwrap();
        assert!(pin.is_set_low().unwrap());
        assert!(!pin.is_set_high().unwrap());

        pin.set_high().unwrap();
        assert!(pin.is_set_high().unwrap());
    }

    #[test]
    fn test_relay_state_active_low() {
        let mut pin = MockPin { state: true };

        // Relay ON = LOW (active low logic)
        pin.set_state(RelayState::On).unwrap();
        assert!(pin.is_set_low().unwrap());
        assert_eq!(pin.get_state().unwrap(), RelayState::On);

        // Relay OFF = HIGH
        pin.set_state(RelayState::Off).unwrap();
        assert!(pin.is_set_high().unwrap());
        assert_eq!(pin.get_state().unwrap(), RelayState::Off);
    }
}
