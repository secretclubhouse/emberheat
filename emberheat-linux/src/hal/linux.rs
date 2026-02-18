//! Linux platform HAL implementation
//!
//! Uses `rppal` crate for Raspberry Pi GPIO access.

use super::{HalError, OutputPin, Platform};
use rppal::gpio::{Gpio, OutputPin as RppalOutputPin};

/// Linux/Raspberry Pi platform implementation
pub struct LinuxPlatform {
    gpio: Gpio,
}

impl LinuxPlatform {
    /// Create a new Linux platform instance
    pub fn new() -> Result<Self, HalError> {
        let gpio = Gpio::new().map_err(|e| HalError::GpioInitError(e.to_string()))?;
        Ok(Self { gpio })
    }
}

impl Default for LinuxPlatform {
    fn default() -> Self {
        Self::new().expect("Failed to initialize GPIO")
    }
}

/// Wrapper around rppal OutputPin that implements our OutputPin trait
pub struct LinuxOutputPin {
    pin: RppalOutputPin,
}

impl OutputPin for LinuxOutputPin {
    fn set_high(&mut self) {
        self.pin.set_high();
    }

    fn set_low(&mut self) {
        self.pin.set_low();
    }

    fn is_set_low(&self) -> bool {
        self.pin.is_set_low()
    }
}

impl Platform for LinuxPlatform {
    type Pin = LinuxOutputPin;

    fn get_output_pin(&self, pin_number: u8) -> Result<Self::Pin, HalError> {
        let pin = self
            .gpio
            .get(pin_number)
            .map_err(|_| HalError::PinUnavailable(pin_number))?
            .into_output();

        Ok(LinuxOutputPin { pin })
    }

    fn name(&self) -> &'static str {
        "Linux (Raspberry Pi)"
    }
}
