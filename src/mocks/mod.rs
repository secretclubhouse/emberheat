//! Mock implementations for testing
//!
//! This module provides mock implementations of hardware dependencies
//! so tests can run on non-Raspberry Pi hardware.
//!
//! # Usage
//!
//! ```rust
//! use thermos_rust::mocks::{MockGpioController, MockTemperatureSensor, MockStateManager};
//!
//! #[cfg(test)]
//! mod tests {
//!     use super::*;
//!
//!     #[tokio::test]
//!     async fn test_with_mocks() {
//!         let mut gpio = MockGpioController::new();
//!         gpio.expect_init_relay()
//!             .returning(|_| Ok(()));
//!
//!         // Use the mock in tests...
//!     }
//! }
//! ```

#[cfg(test)]
pub mod gpio_mock;
#[cfg(test)]
pub mod sensor_mock;
#[cfg(test)]
pub mod state_mock;

#[cfg(test)]
pub use gpio_mock::MockGpioController;
#[cfg(test)]
pub use sensor_mock::MockTemperatureSensor;
#[cfg(test)]
pub use state_mock::MockStateManager;
