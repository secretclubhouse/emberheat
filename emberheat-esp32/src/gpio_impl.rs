//! GPIO implementation for ESP32-C6 using esp-hal
//!
//! This module implements the emberheat-core OutputPin trait for ESP32 GPIO pins,
//! providing relay control for heating systems.

use emberheat_core::{GpioError, OutputPin};
use esp_hal::gpio::{Level, Output};

/// ESP32 GPIO pin wrapper that implements emberheat-core OutputPin trait
///
/// Uses active-low logic: LOW = relay ON (heating), HIGH = relay OFF
pub struct Esp32GpioPin {
    pin: Output<'static>,
}

impl Esp32GpioPin {
    /// Create a new GPIO pin for relay control
    ///
    /// # Arguments
    /// * `pin` - ESP32 output pin from esp-hal
    ///
    /// # Example
    /// ```no_run
    /// use esp_hal::gpio::Io;
    /// let io = Io::new(peripherals.GPIO, peripherals.IO_MUX);
    /// let relay_pin = Esp32GpioPin::new(io.pins.gpio4.into_push_pull_output());
    /// ```
    pub fn new(pin: Output<'static>) -> Self {
        Self { pin }
    }
}

impl OutputPin for Esp32GpioPin {
    fn set_high(&mut self) -> Result<(), GpioError> {
        self.pin.set_level(Level::High);
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), GpioError> {
        self.pin.set_level(Level::Low);
        Ok(())
    }

    fn is_set_low(&self) -> Result<bool, GpioError> {
        Ok(self.pin.is_set_low())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Hardware tests require actual ESP32-C6 hardware
    // These are compile-time tests to verify trait implementation

    #[test]
    fn test_gpio_trait_implementation() {
        // This test just ensures the trait is properly implemented
        // Actual hardware tests would require embedded test framework
    }
}
