#![no_std]

//! Thermos ESP32-C6 platform-specific implementations
//!
//! This library provides ESP32-specific implementations of the
//! thermos-core traits for bare-metal embedded systems.
//!
//! # Feature Flags
//!
//! - `onewire-rmt`: Enable the RMT-based 1-Wire implementation (requires compatible
//!   esp-hal-rmt-onewire version). When disabled, use `sensors::SharedDs18b20Sensor`
//!   with placeholder implementation.

extern crate alloc;

pub mod controller;
pub mod gpio_impl;
pub mod persistence_nvs;
pub mod sensor_i2c;
pub mod sensors;
pub mod wifi;
pub mod zone;

// The sensor_onewire module requires esp-hal-rmt-onewire which has compatibility
// issues with esp-hal 1.0. Enable with `onewire-rmt` feature when dependency is fixed.
#[cfg(feature = "onewire-rmt")]
pub mod sensor_onewire;

#[cfg(test)]
mod mocks;

// Re-export commonly used items
pub use controller::run_zone_control_loop;
pub use gpio_impl::Esp32GpioPin;
pub use persistence_nvs::NvsStorage;
pub use sensor_i2c::Bme280Sensor;
pub use sensors::{MockSensor, SharedDs18b20Sensor, SharedOneWireBus};
pub use wifi::WifiConfig;
pub use zone::{SensorType, ZoneConfig, ZoneState, MAX_ZONES};

// Re-export Ds18b20Sensor only when onewire-rmt feature is enabled
#[cfg(feature = "onewire-rmt")]
pub use sensor_onewire::Ds18b20Sensor;
