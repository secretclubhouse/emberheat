//! Matter stack integration for thermos-esp32
//!
//! Provides the glue between rs-matter-embassy and our zone thermostat states.
//!
//! # Design Decision
//!
//! This uses a thin wrapper approach (Option B from Phase 3 design) that delegates
//! directly to `ZoneThermostatState`. See `docs/plans/2026-01-27-phase3-matter-integration.md`
//! for rationale and future refactor path to move handler logic to thermos-matter.
//!
//! # Handler Pattern
//!
//! rs-matter-embassy uses handler chaining with `EmptyHandler.chain(...)`. Each zone
//! gets its own `ZoneThermostatCluster` handler chained for its endpoint.

use rs_matter_embassy::matter::dm::Dataver;

use crate::config::NUM_ZONES;
use crate::matter::zones::ZONE_STATES;

use thermos_matter::ZoneThermostatState;

// These imports will be used when implementing the full handler in Task 4:
// use rs_matter_embassy::matter::dm::clusters::desc::{self, ClusterHandler as _};
// use rs_matter_embassy::matter::dm::{Async, EmptyHandler, Endpoint, EpClMatcher, Node};
// use rs_matter_embassy::matter::{clusters, devices};
// use crate::config::{MATTER_DEVICE_NAME, MATTER_DISCRIMINATOR, MATTER_PASSCODE, MATTER_PRODUCT_ID, MATTER_VENDOR_ID};

// =============================================================================
// Constants
// =============================================================================

/// Thermostat cluster ID (Matter spec 0x0201)
pub const CLUSTER_THERMOSTAT: u32 = 0x0201;

/// Matter device type for Thermostat (0x0301)
pub const DEV_TYPE_THERMOSTAT: u32 = 0x0301;

/// First zone endpoint ID (endpoint 0 is root/system)
pub const FIRST_ZONE_ENDPOINT: u16 = 1;

// =============================================================================
// Thermostat Attribute IDs (heating-only subset)
// =============================================================================

/// Thermostat attribute IDs from Matter specification
pub mod attr {
    /// Current temperature in 0.01°C units (nullable i16)
    pub const LOCAL_TEMPERATURE: u16 = 0x0000;
    /// Target heating temperature in 0.01°C units (i16)
    pub const OCCUPIED_HEATING_SETPOINT: u16 = 0x0012;
    /// Minimum allowed setpoint (i16)
    pub const MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0015;
    /// Maximum allowed setpoint (i16)
    pub const MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0016;
    /// Control sequence: 0x02 = Heating Only
    pub const CONTROL_SEQUENCE_OF_OPERATION: u16 = 0x001B;
    /// System mode: 0x00 = Off, 0x04 = Heat
    pub const SYSTEM_MODE: u16 = 0x001C;
    /// Running state bitmap: bit 0 = Heat active
    pub const THERMOSTAT_RUNNING_STATE: u16 = 0x0029;
}

// =============================================================================
// Zone Thermostat Cluster Handler
// =============================================================================

/// Handler for a single zone's thermostat cluster
///
/// This is a thin wrapper that reads/writes state from `ZoneThermostatState`.
/// The actual hysteresis control logic runs in the control loop task.
pub struct ZoneThermostatCluster {
    /// Matter endpoint ID for this zone (1-based)
    endpoint_id: u16,
    /// Data version for change tracking
    dataver: Dataver,
}

impl ZoneThermostatCluster {
    /// Create a new thermostat cluster handler for a zone
    ///
    /// # Arguments
    ///
    /// * `endpoint_id` - Matter endpoint ID (1 = zone 0, 2 = zone 1, etc.)
    /// * `dataver` - Data version tracker for attribute changes
    pub fn new(endpoint_id: u16, dataver: Dataver) -> Self {
        Self {
            endpoint_id,
            dataver,
        }
    }

    /// Get the zone index (0-based) for this handler
    fn zone_index(&self) -> usize {
        (self.endpoint_id - FIRST_ZONE_ENDPOINT) as usize
    }

    /// Get reference to the zone state
    fn zone_state(&self) -> Option<&'static ZoneThermostatState> {
        ZONE_STATES.get(self.zone_index())
    }
}

// TODO: Implement the actual handler traits in Task 4
// The exact traits depend on rs-matter-embassy's API:
// - Need to implement attribute read/write
// - Need to handle subscription notifications
//
// Based on the light_wifi.rs example, we need to:
// 1. Define a CLUSTER constant with cluster metadata
// 2. Implement hooks trait similar to OnOffHooks
// 3. Wrap with HandlerAsyncAdaptor for the handler chain

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert zone index to Matter endpoint ID
pub fn zone_to_endpoint(zone_index: usize) -> u16 {
    FIRST_ZONE_ENDPOINT + zone_index as u16
}

/// Convert Matter endpoint ID to zone index
pub fn endpoint_to_zone(endpoint_id: u16) -> Option<usize> {
    if endpoint_id >= FIRST_ZONE_ENDPOINT {
        let index = (endpoint_id - FIRST_ZONE_ENDPOINT) as usize;
        if index < NUM_ZONES {
            return Some(index);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_endpoint_conversion() {
        assert_eq!(zone_to_endpoint(0), 1);
        assert_eq!(zone_to_endpoint(1), 2);
        assert_eq!(zone_to_endpoint(6), 7);

        assert_eq!(endpoint_to_zone(1), Some(0));
        assert_eq!(endpoint_to_zone(2), Some(1));
        assert_eq!(endpoint_to_zone(0), None); // Root endpoint
    }
}
