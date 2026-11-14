//! Matter protocol integration for emberheat-esp32
//!
//! Re-exports emberheat-matter and provides the bridge to emberheat-core.
//!
//! # Architecture
//!
//! The Matter thermostat cluster implementation lives in the `emberheat-matter` crate
//! for testability. This module re-exports everything and adds the `bridge` module
//! which connects Matter state to emberheat-core's control loop.
//!
//! # Modules
//!
//! - All types from `emberheat-matter` (cluster, types, state, handler, device)
//! - [`bridge`]: Bidirectional sync with emberheat-core control loop
//! - [`zones`]: Zone state storage sized by config::NUM_ZONES

pub mod bridge;
pub mod mqtt_bridge;
pub mod stack;
pub mod zones;

// Re-export everything from emberheat-matter
pub use emberheat_matter::*;

// Re-export local zone state functions
pub use zones::{get_zone_state, get_zone_state_by_endpoint, ZONE_STATES};

// Re-export bridge functions
pub use bridge::{
    get_control_target, get_matter_setpoint, is_zone_enabled_in_matter, set_matter_mode,
    set_matter_setpoint, sync_local_state_to_matter, update_matter_from_control_loop,
};

// Re-export MQTT bridge for Matter+MQTT mode
pub use mqtt_bridge::MqttBridge;
