//! Stub platform HAL implementation
//!
//! Provides a mock implementation for development and testing
//! on non-Linux platforms (macOS, Windows).

use super::{HalError, OutputPin, Platform};
use std::cell::Cell;
use tracing::debug;

/// Stub platform for development and testing
pub struct StubPlatform;

impl StubPlatform {
    /// Create a new stub platform instance
    pub fn new() -> Result<Self, HalError> {
        tracing::warn!("Using stub HAL platform - GPIO operations will be simulated");
        Ok(Self)
    }
}

impl Default for StubPlatform {
    fn default() -> Self {
        Self::new().expect("Stub platform should never fail to initialize")
    }
}

/// Stub output pin that tracks state in memory
pub struct StubOutputPin {
    pin_number: u8,
    is_low: Cell<bool>,
}

impl StubOutputPin {
    fn new(pin_number: u8) -> Self {
        Self {
            pin_number,
            is_low: Cell::new(false), // Default to high (off for active-low relays)
        }
    }
}

impl OutputPin for StubOutputPin {
    fn set_high(&mut self) {
        debug!("Stub GPIO {}: set HIGH", self.pin_number);
        self.is_low.set(false);
    }

    fn set_low(&mut self) {
        debug!("Stub GPIO {}: set LOW", self.pin_number);
        self.is_low.set(true);
    }

    fn is_set_low(&self) -> bool {
        self.is_low.get()
    }
}

impl Platform for StubPlatform {
    type Pin = StubOutputPin;

    fn get_output_pin(&self, pin_number: u8) -> Result<Self::Pin, HalError> {
        debug!("Stub platform: creating output pin {}", pin_number);
        Ok(StubOutputPin::new(pin_number))
    }

    fn name(&self) -> &'static str {
        "Stub (Development/Testing)"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_pin_operations() {
        let platform = StubPlatform::new().unwrap();
        let mut pin = platform.get_output_pin(23).unwrap();

        // Default is high
        assert!(!pin.is_set_low());
        assert!(pin.is_set_high());

        // Set low
        pin.set_low();
        assert!(pin.is_set_low());
        assert!(!pin.is_set_high());

        // Set high
        pin.set_high();
        assert!(!pin.is_set_low());
        assert!(pin.is_set_high());
    }

    #[test]
    fn test_stub_platform_name() {
        let platform = StubPlatform::new().unwrap();
        assert!(platform.name().contains("Stub"));
    }
}
