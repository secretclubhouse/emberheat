//! Matter Thermostat Cluster implementation
//!
//! Provides Matter Thermostat Cluster (0x0201) implementations for both
//! heating-only and heating+cooling thermostats, designed for integration
//! with rs-matter on embedded systems.
//!
//! # Features
//!
//! - `std` - Enable std library support (default: no_std)
//!
//! # Architecture
//!
//! - [`cluster`]: Cluster definition from Matter IDL and ThermostatHooks trait
//! - [`types`]: Type definitions (attribute IDs, enums, temperature conversions)
//! - [`state`]: Thread-safe state container for zone thermostat data
//! - [`handler`]: Matter cluster handler implementation
//! - [`device`]: Multi-zone device configuration
//!
//! # Example
//!
//! ```ignore
//! use emberheat_matter::{DeviceInfo, ZoneThermostatState, endpoint_to_zone_index};
//!
//! // Create device configuration
//! let device = DeviceInfo::new(3); // 3-zone thermostat
//!
//! // Application owns zone states (typically static or in main)
//! let zones: [ZoneThermostatState; 3] = core::array::from_fn(|_| ZoneThermostatState::new());
//!
//! // Map Matter endpoint to zone index
//! if let Some(idx) = endpoint_to_zone_index(1, device.num_zones) {
//!     zones[idx].set_setpoint_celsius(21.5).unwrap();
//!
//!     // Check if heating
//!     if zones[idx].is_heating() {
//!         // Relay is on
//!     }
//! }
//! ```

#![cfg_attr(not(feature = "std"), no_std)]

pub mod cluster;
pub mod device;
pub mod handler;
pub mod state;
pub mod types;

// Re-export cluster types (from rs-matter IDL)
pub use cluster::{
    AttributeId, ControlSequenceOfOperationEnum, Feature, OutOfBandMessage, RelayStateBitmap,
    SystemModeEnum, ThermostatHooks, HEATING_AND_COOLING_CLUSTER, HEATING_ONLY_CLUSTER,
};

// Re-export device types
pub use device::{
    endpoint_to_zone_index, zone_index_to_endpoint, DeviceInfo, FIRST_ZONE_ENDPOINT, MAX_ZONES,
    ROOT_ENDPOINT,
};

// Re-export handler
pub use handler::ThermostatHandler;

// Re-export state types
pub use state::{SetpointError, ZoneThermostatState};
