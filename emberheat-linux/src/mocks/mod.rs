//! Mock implementations for testing
//!
//! This module provides mock implementations of hardware dependencies
//! so tests can run on non-Raspberry Pi hardware.
//!
//! # Usage
//!
//! ```rust
//! use emberheat::mocks::{MockGpioController, MockTemperatureSensor, MockStateManager};
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
#[allow(unused_imports)]
pub use gpio_mock::MockGpioController;
#[cfg(test)]
#[allow(unused_imports)]
pub use sensor_mock::MockTemperatureSensor;
#[cfg(test)]
#[allow(unused_imports)]
pub use state_mock::MockStateManager;
