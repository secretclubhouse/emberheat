#![no_std]

//! Thermos ESP32-C6 platform-specific implementations
//!
//! This library provides ESP32-specific implementations of the
//! thermos-core traits for bare-metal embedded systems.

extern crate alloc;

pub mod gpio_impl;
pub mod persistence_nvs;
pub mod sensor_i2c;
pub mod sensor_onewire;
pub mod wifi;
pub mod zone;

#[cfg(test)]
mod mocks;

// Re-export commonly used items
pub use gpio_impl::Esp32GpioPin;
pub use persistence_nvs::NvsStorage;
pub use sensor_i2c::{Bme280Sensor, MockSensor};
pub use sensor_onewire::Ds18b20Sensor;
pub use wifi::WifiConfig;
pub use zone::{SensorType, ZoneConfig, ZoneState, MAX_ZONES};
