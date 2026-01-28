//! Matter protocol integration for thermos-esp32
//!
//! Re-exports thermos-matter and provides the bridge to thermos-core.
//!
//! # Architecture
//!
//! The Matter thermostat cluster implementation lives in the `thermos-matter` crate
//! for testability. This module re-exports everything and adds the `bridge` module
//! which connects Matter state to thermos-core's control loop.
//!
//! # Modules
//!
//! - All types from `thermos-matter` (cluster, types, state, handler, device)
//! - [`bridge`]: Bidirectional sync with thermos-core control loop
//! - [`zones`]: Zone state storage sized by config::NUM_ZONES

pub mod bridge;
pub mod mqtt_bridge;
pub mod stack;
pub mod zones;

// Re-export everything from thermos-matter
pub use thermos_matter::*;

// Re-export local zone state functions
pub use zones::{get_zone_state, get_zone_state_by_endpoint, ZONE_STATES};

// Re-export bridge functions
pub use bridge::{
    get_matter_setpoint, is_zone_enabled_in_matter, set_matter_mode, set_matter_setpoint,
    sync_local_state_to_matter, update_matter_from_control_loop,
};

// Re-export MQTT bridge for Matter+MQTT mode
pub use mqtt_bridge::MqttBridge;
