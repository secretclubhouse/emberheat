//! Mock GPIO controller for testing

use crate::gpio::{GpioError, RelayState};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Mock GPIO controller that doesn't require actual hardware
///
/// This mock implementation allows tests to run on any system without
/// requiring Raspberry Pi GPIO hardware.
#[derive(Clone)]
pub struct MockGpioController {
    /// Track initialized relays
    relays: Arc<Mutex<HashMap<u8, RelayState>>>,
    /// Track VCC controller state
    vcc_initialized: Arc<Mutex<bool>>,
    /// Track VCC power state
    vcc_powered: Arc<Mutex<bool>>,
}

impl MockGpioController {
    /// Create a new mock GPIO controller
    pub fn new() -> Self {
        Self {
            relays: Arc::new(Mutex::new(HashMap::new())),
            vcc_initialized: Arc::new(Mutex::new(false)),
            vcc_powered: Arc::new(Mutex::new(true)),
        }
    }

    /// Initialize a relay on the specified pin
    pub fn init_relay(&self, pin_number: u8) -> Result<()> {
        let mut relays = self.relays.lock().unwrap();

        if relays.contains_key(&pin_number) {
            return Err(GpioError::PinInUse(pin_number).into());
        }

        relays.insert(pin_number, RelayState::Off);
        Ok(())
    }

    /// Set relay state
    pub fn set_relay(&self, pin_number: u8, state: RelayState) -> Result<()> {
        let mut relays = self.relays.lock().unwrap();

        if !relays.contains_key(&pin_number) {
            return Err(GpioError::PinNotFound(pin_number).into());
        }

        relays.insert(pin_number, state);
        Ok(())
    }

    /// Get relay state
    pub fn get_relay_state(&self, pin_number: u8) -> Result<RelayState> {
        let relays = self.relays.lock().unwrap();

        relays
            .get(&pin_number)
            .copied()
            .ok_or_else(|| GpioError::PinNotFound(pin_number).into())
    }

    /// Initialize VCC controller
    pub fn init_vcc_controller(&self, _pin_number: u8) -> Result<()> {
        let mut vcc = self.vcc_initialized.lock().unwrap();
        *vcc = true;
        Ok(())
    }

    /// Power cycle the VCC controller
    pub async fn power_cycle_vcc(&self, down_time: Duration) -> Result<()> {
        let vcc_initialized = *self.vcc_initialized.lock().unwrap();

        if !vcc_initialized {
            return Ok(());
        }

        // Simulate power cycle
        {
            let mut powered = self.vcc_powered.lock().unwrap();
            *powered = false;
        }

        tokio::time::sleep(down_time).await;

        {
            let mut powered = self.vcc_powered.lock().unwrap();
            *powered = true;
        }

        Ok(())
    }

    /// Check if VCC is powered
    pub fn is_vcc_powered(&self) -> bool {
        *self.vcc_powered.lock().unwrap()
    }

    /// Get all relay states (for testing)
    #[allow(dead_code)]
    pub fn get_all_relay_states(&self) -> HashMap<u8, RelayState> {
        self.relays.lock().unwrap().clone()
    }

    /// Cleanup
    #[allow(dead_code)]
    pub fn cleanup(&self) {
        let mut relays = self.relays.lock().unwrap();
        relays.clear();

        let mut vcc = self.vcc_initialized.lock().unwrap();
        *vcc = false;
    }
}

impl Default for MockGpioController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_gpio_init_relay() {
        let gpio = MockGpioController::new();

        // Should succeed first time
        assert!(gpio.init_relay(23).is_ok());

        // Should fail second time (pin in use)
        assert!(gpio.init_relay(23).is_err());
    }

    #[test]
    fn test_mock_gpio_set_relay() {
        let gpio = MockGpioController::new();

        // Initialize relay
        gpio.init_relay(23).unwrap();

        // Set to ON
        gpio.set_relay(23, RelayState::On).unwrap();
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::On);

        // Set to OFF
        gpio.set_relay(23, RelayState::Off).unwrap();
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::Off);
    }

    #[test]
    fn test_mock_gpio_uninitialized_pin() {
        let gpio = MockGpioController::new();

        // Should fail for uninitialized pin
        assert!(gpio.set_relay(23, RelayState::On).is_err());
        assert!(gpio.get_relay_state(23).is_err());
    }

    #[tokio::test]
    async fn test_mock_vcc_power_cycle() {
        let gpio = MockGpioController::new();

        gpio.init_vcc_controller(0).unwrap();
        assert!(gpio.is_vcc_powered());

        // Start power cycle in background
        let gpio_clone = gpio.clone();
        let handle =
            tokio::spawn(
                async move { gpio_clone.power_cycle_vcc(Duration::from_millis(50)).await },
            );

        // Wait a bit and check it's off
        tokio::time::sleep(Duration::from_millis(10)).await;
        assert!(!gpio.is_vcc_powered());

        // Wait for cycle to complete
        handle.await.unwrap().unwrap();

        // Should be back on
        assert!(gpio.is_vcc_powered());
    }

    #[test]
    fn test_relay_initialization_multiple_pins() {
        let gpio = MockGpioController::new();

        // Initialize multiple relays on different pins
        let pins = [17, 18, 23, 24, 25];

        for pin in pins {
            let result = gpio.init_relay(pin);
            assert!(
                result.is_ok(),
                "Initialization should succeed for pin {}",
                pin
            );
        }

        // Verify all relays are accessible
        for pin in pins {
            let state = gpio.get_relay_state(pin);
            assert!(
                state.is_ok(),
                "Should be able to read state for pin {}",
                pin
            );
            assert_eq!(state.unwrap(), RelayState::Off);
        }
    }

    #[test]
    fn test_relay_control_after_initialization() {
        let gpio = MockGpioController::new();

        // Initialize relay
        gpio.init_relay(23).expect("Initialization should succeed");

        // Should be able to control relay after initialization
        gpio.set_relay(23, RelayState::On)
            .expect("Should be able to turn relay ON");
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::On);

        gpio.set_relay(23, RelayState::Off)
            .expect("Should be able to turn relay OFF");
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::Off);
    }

    #[test]
    fn test_relay_initialization_sequence() {
        let gpio = MockGpioController::new();

        // Simulate the sequence that would have failed before the fix:
        // 1. Initialize a relay (internally reads current state then sets as output)
        // 2. Control the relay
        // 3. Initialize another relay on different pin
        // 4. Both relays should work independently

        // Initialize first relay
        gpio.init_relay(23)
            .expect("First relay init should succeed");
        gpio.set_relay(23, RelayState::On)
            .expect("Should control first relay");

        // Initialize second relay - this tests that pin resources are properly managed
        gpio.init_relay(24)
            .expect("Second relay init should succeed");
        gpio.set_relay(24, RelayState::On)
            .expect("Should control second relay");

        // Both relays should maintain independent state
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::On);
        assert_eq!(gpio.get_relay_state(24).unwrap(), RelayState::On);

        // Change first relay state
        gpio.set_relay(23, RelayState::Off)
            .expect("Should change first relay");

        // Second relay should be unaffected
        assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::Off);
        assert_eq!(gpio.get_relay_state(24).unwrap(), RelayState::On);
    }

    #[test]
    fn test_relay_duplicate_initialization_error_message() {
        let gpio = MockGpioController::new();

        // First initialization should succeed
        let result1 = gpio.init_relay(23);
        assert!(result1.is_ok(), "First initialization should succeed");

        // Second initialization on same pin should fail
        let result2 = gpio.init_relay(23);
        assert!(result2.is_err(), "Second initialization should fail");

        // Verify error message mentions pin in use
        let err = result2.unwrap_err();
        let err_msg = format!("{}", err);
        assert!(
            err_msg.contains("already in use") || err_msg.contains("23"),
            "Error should mention pin is in use: {}",
            err_msg
        );
    }
}
