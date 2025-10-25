//! GPIO abstraction layer for relay and sensor control
//!
//! This module provides a clean abstraction over Raspberry Pi GPIO operations,
//! specifically for controlling heating relays and managing sensor power.
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

use anyhow::Result;
#[cfg(target_os = "linux")]
use anyhow::Context;
#[cfg(target_os = "linux")]
use rppal::gpio::{Gpio, InputPin, Level, Mode, OutputPin, Pin, PullUpDown};
#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::sync::{Arc, Mutex};
use thiserror::Error;
use tracing::{debug, info};

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

// Linux-specific implementation using rppal
#[cfg(target_os = "linux")]
impl From<RelayState> for Level {
    fn from(state: RelayState) -> Self {
        match state {
            RelayState::On => Level::Low,  // Active-LOW
            RelayState::Off => Level::High,
        }
    }
}

#[cfg(target_os = "linux")]
impl From<Level> for RelayState {
    fn from(level: Level) -> Self {
        match level {
            Level::Low => RelayState::On,   // Active-LOW
            Level::High => RelayState::Off,
        }
    }
}

/// VCC pin controller for sensor power cycling
#[cfg(target_os = "linux")]
pub struct VccController {
    pin: OutputPin,
}

#[cfg(not(target_os = "linux"))]
pub struct VccController {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(target_os = "linux")]
impl VccController {
    /// Create a new VCC controller
    ///
    /// # Arguments
    ///
    /// * `gpio` - GPIO instance
    /// * `pin_number` - GPIO pin number (BCM mode)
    pub fn new(gpio: &Gpio, pin_number: u8) -> Result<Self> {
        let pin = gpio
            .get(pin_number)?
            .into_output();

        // Default to HIGH (power on)
        pin.set_high();

        info!("VCC controller initialized on GPIO {}", pin_number);

        Ok(Self { pin })
    }

    /// Power cycle the VCC pin
    ///
    /// Turns power off, waits for specified duration, then turns back on
    ///
    /// # Arguments
    ///
    /// * `down_time` - Duration to keep power off
    pub async fn power_cycle(&mut self, down_time: std::time::Duration) {
        info!("Starting VCC power cycle (down for {:?})", down_time);

        self.pin.set_low();
        tokio::time::sleep(down_time).await;
        self.pin.set_high();

        info!("VCC power cycle complete");
    }

    /// Turn power on
    pub fn power_on(&mut self) {
        self.pin.set_high();
    }

    /// Turn power off
    pub fn power_off(&mut self) {
        self.pin.set_low();
    }
}

#[cfg(not(target_os = "linux"))]
impl VccController {
    /// Create a new VCC controller (stub for non-Linux)
    pub fn new(_gpio: &(), _pin_number: u8) -> Result<Self> {
        info!("VCC controller stub initialized (non-Linux platform)");
        Ok(Self { _phantom: std::marker::PhantomData })
    }

    /// Power cycle stub
    pub async fn power_cycle(&mut self, down_time: std::time::Duration) {
        info!("VCC power cycle stub (sleeping for {:?})", down_time);
        tokio::time::sleep(down_time).await;
    }

    /// Power on stub
    pub fn power_on(&mut self) {
        debug!("VCC power on (stub)");
    }

    /// Power off stub
    pub fn power_off(&mut self) {
        debug!("VCC power off (stub)");
    }
}

/// Relay controller for heating control
#[cfg(target_os = "linux")]
pub struct RelayController {
    /// Output pin for controlling the relay
    output_pin: OutputPin,
    /// Pin number for reference
    pin_number: u8,
}

/// Stub relay controller for non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub struct RelayController {
    pin_number: u8,
}

#[cfg(target_os = "linux")]
impl RelayController {
    /// Create a new relay controller
    ///
    /// # Arguments
    ///
    /// * `gpio` - GPIO instance
    /// * `pin_number` - GPIO pin number (BCM mode)
    pub fn new(gpio: &Gpio, pin_number: u8) -> Result<Self> {
        // First, get the pin as input to read current state
        let input_pin = gpio.get(pin_number)?.into_input();
        let current_level = input_pin.read();

        // Now configure as output, preserving current state
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

    /// Set relay state
    ///
    /// # Arguments
    ///
    /// * `state` - Desired relay state
    pub fn set_state(&mut self, state: RelayState) -> Result<()> {
        let level = Level::from(state);
        self.output_pin.write(level);

        debug!(
            "Relay GPIO {} set to {:?} (level: {:?})",
            self.pin_number, state, level
        );

        Ok(())
    }

    /// Get current relay state
    pub fn get_state(&self) -> RelayState {
        RelayState::from(self.output_pin.read())
    }

    /// Get pin number
    pub fn pin_number(&self) -> u8 {
        self.pin_number
    }
}

#[cfg(not(target_os = "linux"))]
impl RelayController {
    pub fn new(_gpio: &(), pin_number: u8) -> Result<Self> {
        Ok(Self { pin_number })
    }

    pub fn set_state(&mut self, _state: RelayState) -> Result<()> {
        anyhow::bail!("Relay control not available on non-Linux platforms");
    }

    pub fn get_state(&self) -> RelayState {
        RelayState::Off
    }

    pub fn pin_number(&self) -> u8 {
        self.pin_number
    }
}

/// Main GPIO controller managing all GPIO operations
#[cfg(target_os = "linux")]
pub struct GpioController {
    gpio: Gpio,
    relays: Arc<Mutex<HashMap<u8, RelayController>>>,
    vcc_controller: Arc<Mutex<Option<VccController>>>,
}

/// Stub GPIO controller for non-Linux platforms
#[cfg(not(target_os = "linux"))]
pub struct GpioController {
    _phantom: std::marker::PhantomData<()>,
}

#[cfg(target_os = "linux")]
impl GpioController {
    /// Create a new GPIO controller
    pub fn new() -> Result<Self> {
        let gpio = Gpio::new().context("Failed to initialize GPIO")?;

        info!("GPIO controller initialized");

        Ok(Self {
            gpio,
            relays: Arc::new(Mutex::new(HashMap::new())),
            vcc_controller: Arc::new(Mutex::new(None)),
        })
    }

    /// Initialize a relay on the specified pin
    ///
    /// # Arguments
    ///
    /// * `pin_number` - GPIO pin number (BCM mode)
    ///
    /// # Errors
    ///
    /// Returns error if pin is already in use
    pub fn init_relay(&self, pin_number: u8) -> Result<()> {
        let mut relays = self.relays.lock().unwrap();

        if relays.contains_key(&pin_number) {
            return Err(GpioError::PinInUse(pin_number).into());
        }

        let controller = RelayController::new(&self.gpio, pin_number)?;
        relays.insert(pin_number, controller);

        Ok(())
    }

    /// Set relay state
    ///
    /// # Arguments
    ///
    /// * `pin_number` - GPIO pin number
    /// * `state` - Desired relay state
    pub fn set_relay(&self, pin_number: u8, state: RelayState) -> Result<()> {
        let mut relays = self.relays.lock().unwrap();

        let controller = relays
            .get_mut(&pin_number)
            .ok_or(GpioError::PinNotFound(pin_number))?;

        controller.set_state(state)
    }

    /// Get relay state
    ///
    /// # Arguments
    ///
    /// * `pin_number` - GPIO pin number
    pub fn get_relay_state(&self, pin_number: u8) -> Result<RelayState> {
        let relays = self.relays.lock().unwrap();

        let controller = relays
            .get(&pin_number)
            .ok_or(GpioError::PinNotFound(pin_number))?;

        Ok(controller.get_state())
    }

    /// Initialize VCC controller for sensor power management
    ///
    /// # Arguments
    ///
    /// * `pin_number` - GPIO pin number (BCM mode)
    pub fn init_vcc_controller(&self, pin_number: u8) -> Result<()> {
        let controller = VccController::new(&self.gpio, pin_number)?;

        let mut vcc = self.vcc_controller.lock().unwrap();
        *vcc = Some(controller);

        Ok(())
    }

    /// Power cycle the VCC controller
    ///
    /// # Arguments
    ///
    /// * `down_time` - Duration to keep power off
    pub async fn power_cycle_vcc(&self, down_time: std::time::Duration) -> Result<()> {
        let mut vcc = self.vcc_controller.lock().unwrap();

        if let Some(controller) = vcc.as_mut() {
            drop(vcc); // Release lock before async operation

            // Re-acquire for the actual operation
            let mut vcc = self.vcc_controller.lock().unwrap();
            if let Some(controller) = vcc.as_mut() {
                controller.power_cycle(down_time).await;
            }
        }

        Ok(())
    }

    /// Cleanup all GPIO resources
    pub fn cleanup(&self) {
        info!("Cleaning up GPIO resources");

        let mut relays = self.relays.lock().unwrap();
        relays.clear();

        let mut vcc = self.vcc_controller.lock().unwrap();
        *vcc = None;
    }
}

#[cfg(target_os = "linux")]
impl Drop for GpioController {
    fn drop(&mut self) {
        self.cleanup();
    }
}

// Stub implementation for non-Linux platforms (tests only)
#[cfg(not(target_os = "linux"))]
impl GpioController {
    pub fn new() -> Result<Self> {
        anyhow::bail!("GPIO not available on non-Linux platforms. Use MockGpioController for testing.");
    }

    pub fn init_relay(&self, _pin_number: u8) -> Result<()> {
        anyhow::bail!("GPIO not available on non-Linux platforms");
    }

    pub fn set_relay(&self, _pin_number: u8, _state: RelayState) -> Result<()> {
        anyhow::bail!("GPIO not available on non-Linux platforms");
    }

    pub fn get_relay_state(&self, _pin_number: u8) -> Result<RelayState> {
        anyhow::bail!("GPIO not available on non-Linux platforms");
    }

    pub fn init_vcc_controller(&self, _pin_number: u8) -> Result<()> {
        anyhow::bail!("GPIO not available on non-Linux platforms");
    }

    pub async fn power_cycle_vcc(&self, _down_time: std::time::Duration) -> Result<()> {
        anyhow::bail!("GPIO not available on non-Linux platforms");
    }

    pub fn cleanup(&self) {
        // No-op on non-Linux
    }
}

#[cfg(not(target_os = "linux"))]
impl Drop for GpioController {
    fn drop(&mut self) {
        self.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_relay_state_conversion() {
        assert_eq!(Level::from(RelayState::On), Level::Low);
        assert_eq!(Level::from(RelayState::Off), Level::High);
        assert_eq!(RelayState::from(Level::Low), RelayState::On);
        assert_eq!(RelayState::from(Level::High), RelayState::Off);
    }

    #[test]
    fn test_relay_state_equality() {
        assert_eq!(RelayState::On, RelayState::On);
        assert_eq!(RelayState::Off, RelayState::Off);
        assert_ne!(RelayState::On, RelayState::Off);
    }

    // Note: Hardware tests would require mock GPIO or actual Raspberry Pi hardware
    // Integration tests should be run on target hardware
}
