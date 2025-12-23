//! GPIO abstraction layer for relay and sensor control
//!
//! This module provides a clean abstraction over Raspberry Pi GPIO operations,
//! specifically for controlling heating relays and managing sensor power.
//!
//! **Platform Requirements:** Linux with GPIO hardware (Raspberry Pi)
//!
//! On non-Linux platforms, a stub implementation is provided for compilation
//! and testing purposes.
//!
//! # Hardware Interface
//!
//! - **Relay Control**: Active-LOW logic (LOW = heat ON, HIGH = heat OFF)
//! - **Sensor Power**: Standard OUTPUT control for power cycling
//!
//! # Example
//!
//! ```no_run
//! use thermos_rust::gpio::{GpioController, RelayState};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let gpio = GpioController::new()?;
//!
//!     // Control relay
//!     gpio.set_relay(23, RelayState::On)?;
//!
//!     // Read relay state
//!     let state = gpio.get_relay_state(23)?;
//!
//!     Ok(())
//! }
//! ```

#![allow(dead_code)]

use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::info;

/// GPIO controller errors
#[derive(Error, Debug)]
pub enum GpioError {
    #[error("GPIO pin {0} is already in use")]
    PinInUse(u8),

    #[error("GPIO pin {0} not found")]
    PinNotFound(u8),

    #[error("GPIO hardware error: {0}")]
    HardwareError(String),
}

/// Relay state enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelayState {
    /// Relay ON (heating active) - GPIO LOW
    On,
    /// Relay OFF (heating inactive) - GPIO HIGH
    Off,
}

// ============================================================================
// Linux implementation using rppal
// ============================================================================
#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use anyhow::Context;
    use rppal::gpio::{Gpio, Level, OutputPin};
    use tracing::debug;

    impl From<RelayState> for Level {
        fn from(state: RelayState) -> Self {
            match state {
                RelayState::On => Level::Low, // Active-LOW
                RelayState::Off => Level::High,
            }
        }
    }

    impl From<Level> for RelayState {
        fn from(level: Level) -> Self {
            match level {
                Level::Low => RelayState::On, // Active-LOW
                Level::High => RelayState::Off,
            }
        }
    }

    /// VCC pin controller for sensor power cycling
    pub struct VccController {
        pin: OutputPin,
    }

    impl VccController {
        pub fn new(gpio: &Gpio, pin_number: u8) -> Result<Self> {
            let mut pin = gpio.get(pin_number)?.into_output();
            pin.set_high();
            info!("VCC controller initialized on GPIO {}", pin_number);
            Ok(Self { pin })
        }

        pub fn power_on(&mut self) {
            self.pin.set_high();
        }

        pub fn power_off(&mut self) {
            self.pin.set_low();
        }
    }

    /// Relay controller for heating control
    pub struct RelayController {
        output_pin: OutputPin,
        pin_number: u8,
    }

    impl RelayController {
        pub fn new(gpio: &Gpio, pin_number: u8) -> Result<Self> {
            let current_level = {
                let input_pin = gpio.get(pin_number)?.into_input();
                input_pin.read()
            };

            let mut output_pin = gpio.get(pin_number)?.into_output();
            output_pin.write(current_level);

            info!(
                "Relay controller initialized on GPIO {} (current state: {:?})",
                pin_number,
                RelayState::from(current_level)
            );

            Ok(Self {
                output_pin,
                pin_number,
            })
        }

        pub fn set_state(&mut self, state: RelayState) -> Result<()> {
            let level = Level::from(state);
            self.output_pin.write(level);
            debug!(
                "Relay GPIO {} set to {:?} (level: {:?})",
                self.pin_number, state, level
            );
            Ok(())
        }

        pub fn get_state(&self) -> RelayState {
            if self.output_pin.is_set_low() {
                RelayState::On
            } else {
                RelayState::Off
            }
        }
    }

    /// Main GPIO controller managing all GPIO operations
    pub struct GpioController {
        gpio: Gpio,
        relays: Arc<Mutex<HashMap<u8, RelayController>>>,
        vcc_controller: Arc<Mutex<Option<VccController>>>,
    }

    impl GpioController {
        pub fn new() -> Result<Self> {
            let gpio = Gpio::new().context("Failed to initialize GPIO")?;
            info!("GPIO controller initialized");
            Ok(Self {
                gpio,
                relays: Arc::new(Mutex::new(HashMap::new())),
                vcc_controller: Arc::new(Mutex::new(None)),
            })
        }

        pub fn init_relay(&self, pin_number: u8) -> Result<()> {
            let mut relays = self.relays.lock().unwrap();
            if relays.contains_key(&pin_number) {
                return Err(GpioError::PinInUse(pin_number).into());
            }
            let controller = RelayController::new(&self.gpio, pin_number)?;
            relays.insert(pin_number, controller);
            Ok(())
        }

        pub fn set_relay(&self, pin_number: u8, state: RelayState) -> Result<()> {
            let mut relays = self.relays.lock().unwrap();
            let controller = relays
                .get_mut(&pin_number)
                .ok_or(GpioError::PinNotFound(pin_number))?;
            controller.set_state(state)
        }

        pub fn get_relay_state(&self, pin_number: u8) -> Result<RelayState> {
            let relays = self.relays.lock().unwrap();
            let controller = relays
                .get(&pin_number)
                .ok_or(GpioError::PinNotFound(pin_number))?;
            Ok(controller.get_state())
        }

        pub fn init_vcc_controller(&self, pin_number: u8) -> Result<()> {
            let controller = VccController::new(&self.gpio, pin_number)?;
            let mut vcc = self.vcc_controller.lock().unwrap();
            *vcc = Some(controller);
            Ok(())
        }

        pub async fn power_cycle_vcc(&self, down_time: std::time::Duration) -> Result<()> {
            let has_vcc = {
                let vcc = self.vcc_controller.lock().unwrap();
                vcc.is_some()
            };

            if has_vcc {
                info!("Starting VCC power cycle (down for {:?})", down_time);
                {
                    let mut vcc = self.vcc_controller.lock().unwrap();
                    if let Some(controller) = vcc.as_mut() {
                        controller.power_off();
                    }
                }
                embassy_time::Timer::after(embassy_time::Duration::from_millis(
                    down_time.as_millis() as u64,
                ))
                .await;
                {
                    let mut vcc = self.vcc_controller.lock().unwrap();
                    if let Some(controller) = vcc.as_mut() {
                        controller.power_on();
                    }
                }
                info!("VCC power cycle complete");
            }
            Ok(())
        }

        pub fn cleanup(&self) {
            info!("Cleaning up GPIO resources");
            let mut relays = self.relays.lock().unwrap();
            relays.clear();
            let mut vcc = self.vcc_controller.lock().unwrap();
            *vcc = None;
        }
    }

    impl Drop for GpioController {
        fn drop(&mut self) {
            self.cleanup();
        }
    }
}

// ============================================================================
// Stub implementation for non-Linux platforms (macOS, Windows, etc.)
// ============================================================================
#[cfg(not(target_os = "linux"))]
mod stub_impl {
    use super::*;
    use tracing::{debug, warn};

    /// Stub GPIO controller for non-Linux platforms
    pub struct GpioController {
        relays: Arc<Mutex<HashMap<u8, RelayState>>>,
        vcc_on: Arc<Mutex<bool>>,
    }

    impl GpioController {
        pub fn new() -> Result<Self> {
            warn!("GPIO controller running in STUB mode (non-Linux platform)");
            info!("GPIO controller initialized (stub)");
            Ok(Self {
                relays: Arc::new(Mutex::new(HashMap::new())),
                vcc_on: Arc::new(Mutex::new(true)),
            })
        }

        pub fn init_relay(&self, pin_number: u8) -> Result<()> {
            let mut relays = self.relays.lock().unwrap();
            if relays.contains_key(&pin_number) {
                return Err(GpioError::PinInUse(pin_number).into());
            }
            relays.insert(pin_number, RelayState::Off);
            info!("Relay controller initialized on GPIO {} (stub)", pin_number);
            Ok(())
        }

        pub fn set_relay(&self, pin_number: u8, state: RelayState) -> Result<()> {
            let mut relays = self.relays.lock().unwrap();
            if !relays.contains_key(&pin_number) {
                return Err(GpioError::PinNotFound(pin_number).into());
            }
            relays.insert(pin_number, state);
            debug!("Relay GPIO {} set to {:?} (stub)", pin_number, state);
            Ok(())
        }

        pub fn get_relay_state(&self, pin_number: u8) -> Result<RelayState> {
            let relays = self.relays.lock().unwrap();
            relays
                .get(&pin_number)
                .copied()
                .ok_or_else(|| GpioError::PinNotFound(pin_number).into())
        }

        pub fn init_vcc_controller(&self, pin_number: u8) -> Result<()> {
            info!("VCC controller initialized on GPIO {} (stub)", pin_number);
            Ok(())
        }

        pub async fn power_cycle_vcc(&self, down_time: std::time::Duration) -> Result<()> {
            info!("Starting VCC power cycle (down for {:?}) (stub)", down_time);
            {
                let mut vcc = self.vcc_on.lock().unwrap();
                *vcc = false;
            }
            embassy_time::Timer::after(embassy_time::Duration::from_millis(
                down_time.as_millis() as u64
            ))
            .await;
            {
                let mut vcc = self.vcc_on.lock().unwrap();
                *vcc = true;
            }
            info!("VCC power cycle complete (stub)");
            Ok(())
        }

        pub fn cleanup(&self) {
            info!("Cleaning up GPIO resources (stub)");
            let mut relays = self.relays.lock().unwrap();
            relays.clear();
        }
    }

    impl Drop for GpioController {
        fn drop(&mut self) {
            self.cleanup();
        }
    }
}

// Re-export the appropriate implementation
#[cfg(target_os = "linux")]
pub use linux_impl::GpioController;

#[cfg(not(target_os = "linux"))]
pub use stub_impl::GpioController;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_state_equality() {
        assert_eq!(RelayState::On, RelayState::On);
        assert_eq!(RelayState::Off, RelayState::Off);
        assert_ne!(RelayState::On, RelayState::Off);
    }

    #[test]
    fn test_gpio_controller_stub() {
        // This test works on all platforms
        let gpio = GpioController::new().unwrap();
        gpio.init_relay(23).unwrap();
        gpio.set_relay(23, RelayState::On).unwrap();
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::On);
        gpio.set_relay(23, RelayState::Off).unwrap();
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::Off);
    }

    #[test]
    fn test_gpio_pin_not_found() {
        let gpio = GpioController::new().unwrap();
        assert!(gpio.get_relay_state(99).is_err());
        assert!(gpio.set_relay(99, RelayState::On).is_err());
    }

    #[test]
    fn test_gpio_pin_already_in_use() {
        let gpio = GpioController::new().unwrap();
        gpio.init_relay(23).unwrap();
        assert!(gpio.init_relay(23).is_err());
    }
}
