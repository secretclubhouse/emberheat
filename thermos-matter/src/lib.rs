//! Matter Thermostat Cluster implementation
//!
//! Provides a heating-only Matter Thermostat Cluster (0x0201) implementation
//! designed for integration with rs-matter on embedded systems.
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
//! use thermos_matter::{ZoneThermostatState, ThermostatHandler, ZONE_STATES};
//!
//! // Access zone state
//! let zone = &ZONE_STATES[0];
//! zone.set_setpoint_celsius(21.5).unwrap();
//!
//! // Check if heating
//! if zone.is_heating() {
//!     // Relay is on
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
    SystemModeEnum, ThermostatHooks, HEATING_ONLY_CLUSTER,
};

// Re-export device types
pub use device::{
    endpoint_to_zone_index, get_zone_state, get_zone_state_by_endpoint, zone_index_to_endpoint,
    DeviceInfo, NUM_ZONES, ROOT_ENDPOINT, ZONE1_ENDPOINT, ZONE2_ENDPOINT, ZONE3_ENDPOINT,
    ZONE4_ENDPOINT, ZONE5_ENDPOINT, ZONE6_ENDPOINT, ZONE7_ENDPOINT, ZONE_STATES,
};

// Re-export handler
pub use handler::ThermostatHandler;

// Re-export state types
pub use state::{SetpointError, ZoneThermostatState};
