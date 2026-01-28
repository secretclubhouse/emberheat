//! Zone state storage for Matter integration
//!
//! Owns the static ZoneThermostatState array, sized according to config.

use crate::config::NUM_ZONES;
use thermos_matter::state::ZoneThermostatState;

/// Static state for all configured zones
///
/// Sized according to NUM_ZONES in config.rs.
/// The control loop and Matter handlers access these via atomic operations.
pub static ZONE_STATES: [ZoneThermostatState; NUM_ZONES] = {
    // Use a const block to initialize the array
    // This works because ZoneThermostatState::new() is const
    const INIT: ZoneThermostatState = ZoneThermostatState::new();
    [INIT; NUM_ZONES]
};

/// Get zone state by index
///
/// # Arguments
///
/// * `index` - Zone index (0 to NUM_ZONES-1)
///
/// # Returns
///
/// Reference to the zone state, or `None` if index is out of range.
pub fn get_zone_state(index: usize) -> Option<&'static ZoneThermostatState> {
    ZONE_STATES.get(index)
}

/// Get zone state by Matter endpoint number
///
/// # Arguments
///
/// * `endpoint` - Matter endpoint number (1 to NUM_ZONES)
///
/// # Returns
///
/// Reference to the zone state, or `None` if endpoint is not a valid zone.
pub fn get_zone_state_by_endpoint(endpoint: u16) -> Option<&'static ZoneThermostatState> {
    thermos_matter::endpoint_to_zone_index(endpoint, NUM_ZONES).and_then(get_zone_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_states_len_equals_num_zones() {
        assert_eq!(ZONE_STATES.len(), NUM_ZONES);
    }

    #[test]
    fn test_get_zone_state_valid_indices() {
        // Test all valid indices (0 to NUM_ZONES-1)
        for i in 0..NUM_ZONES {
            let state = get_zone_state(i);
            assert!(state.is_some(), "Zone {} should be valid", i);
        }
    }

    #[test]
    fn test_get_zone_state_invalid_index() {
        // Test invalid indices (NUM_ZONES and above)
        assert!(get_zone_state(NUM_ZONES).is_none());
        assert!(get_zone_state(NUM_ZONES + 1).is_none());
        assert!(get_zone_state(100).is_none());
    }

    #[test]
    fn test_get_zone_state_by_endpoint_valid() {
        // Valid endpoints are 1 to NUM_ZONES
        for endpoint in 1..=NUM_ZONES as u16 {
            let state = get_zone_state_by_endpoint(endpoint);
            assert!(state.is_some(), "Endpoint {} should be valid", endpoint);
        }
    }

    #[test]
    fn test_get_zone_state_by_endpoint_invalid() {
        // Endpoint 0 is invalid (bridge endpoint in Matter)
        assert!(get_zone_state_by_endpoint(0).is_none());

        // Endpoint NUM_ZONES+1 and above are invalid
        assert!(get_zone_state_by_endpoint(NUM_ZONES as u16 + 1).is_none());
        assert!(get_zone_state_by_endpoint(100).is_none());
    }
}
