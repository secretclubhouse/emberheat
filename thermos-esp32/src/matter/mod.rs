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

pub mod bridge;

// Re-export everything from thermos-matter
pub use thermos_matter::*;

// Re-export bridge functions
pub use bridge::{
    get_matter_setpoint, is_zone_enabled_in_matter, set_matter_mode, set_matter_setpoint,
    sync_local_state_to_matter, update_matter_from_control_loop,
};
