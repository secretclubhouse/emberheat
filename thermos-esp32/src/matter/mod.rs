//! Matter protocol integration for thermos-esp32
//!
//! Implements Matter Thermostat Cluster (0x0201) for HomeKit/Google/Alexa control.
//!
//! # Architecture
//!
//! This module provides a heating-only thermostat cluster implementation that bridges
//! Matter commands to thermos-core's hysteresis control system.
//!
//! # Modules
//!
//! - [`cluster`]: Thermostat cluster definition from Matter IDL and ThermostatHooks trait
//! - [`types`]: Local thermostat type definitions (attribute IDs, enums, conversions)
//! - [`state`]: Thread-safe state container for zone thermostat data
//! - [`handler`]: Matter cluster handler implementation
//! - [`device`]: Multi-zone device configuration
//! - [`bridge`]: Bidirectional sync with thermos-core control loop

pub mod cluster;
pub mod state;
pub mod types;

// Re-export cluster types (from rs-matter IDL)
pub use cluster::{
    AttributeId, ControlSequenceOfOperationEnum, Feature, OutOfBandMessage, RelayStateBitmap,
    SystemModeEnum, ThermostatHooks, HEATING_ONLY_CLUSTER,
};

// Re-export state types
pub use state::{SetpointError, ZoneThermostatState};
