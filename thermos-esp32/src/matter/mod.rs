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
//! - [`types`]: Thermostat cluster type definitions (attribute IDs, enums, conversions)
//! - [`state`]: Thread-safe state container for zone thermostat data
//! - [`handler`]: Matter cluster handler implementation
//! - [`device`]: Multi-zone device configuration
//! - [`bridge`]: Bidirectional sync with thermos-core control loop

pub mod types;
