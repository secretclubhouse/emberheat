/**
 * FFI module for Matter protocol integration
 *
 * This module provides Rust bindings to the Connected Home IP C library,
 * enabling Matter protocol support for the thermostat system.
 */
// Raw FFI bindings to C wrapper
pub mod bindings;

// Safe Rust wrapper around FFI
pub mod wrapper;

// Re-export the safe wrapper types
pub use wrapper::{Device, DeviceConfig, Thermostat, ThermostatConfig, ThermostatRunningState};
