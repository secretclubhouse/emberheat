//! Bridge between Matter cluster state and emberheat-core control loop
//!
//! Syncs temperature readings, setpoints, and heating state between:
//! - Matter cluster (for HomeKit/Google/Alexa)
//! - emberheat-core hysteresis control (actual heating logic)
//!
//! # Data Flow
//!
//! ```text
//! Matter Controller (HomeKit/Google)
//!        │
//!        ▼ (setpoint change)
//! ┌─────────────────────┐
//! │ ZoneThermostatState │  ◄──── Matter cluster handler
//! └─────────────────────┘
//!        │
//!        ▼ (get_matter_setpoint)
//! ┌─────────────────────┐
//! │ Control Loop        │  ◄──── emberheat-core hysteresis
//! └─────────────────────┘
//!        │
//!        ▼ (update_matter_from_control_loop)
//! ┌─────────────────────┐
//! │ ZoneThermostatState │  ──── Pushes to Matter
//! └─────────────────────┘
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use emberheat_esp32::matter::bridge;
//!
//! // In control loop, read setpoint from Matter
//! if let Some(target) = bridge::get_matter_setpoint(zone_index) {
//!     state.target_temp_c = target;
//! }
//!
//! // After control iteration, sync state back to Matter
//! bridge::update_matter_from_control_loop(
//!     zone_index,
//!     current_temp,
//!     heating_active,
//! );
//! ```

use crate::matter::zones::get_zone_state;
use emberheat_core::HeatingState;
use emberheat_matter::types::SystemMode;

/// Update Matter state from emberheat-core control loop
///
/// Call this after each control cycle to sync the current temperature
/// and heating state to Matter for controller updates.
///
/// # Arguments
///
/// * `zone_index` - Zero-based zone index (0 to NUM_ZONES-1)
/// * `current_temp_c` - Current temperature in Celsius from sensor
/// * `heating_active` - Whether heating relay is currently on
pub fn update_matter_from_control_loop(
    zone_index: usize,
    current_temp_c: f32,
    heating_active: bool,
) {
    if let Some(state) = get_zone_state(zone_index) {
        state.set_local_temperature_celsius(current_temp_c);
        state.set_heating_active(heating_active);
    }
}

/// What the control loop should regulate to for a zone, as set through Matter.
///
/// Returns the heating state and target temperature in Celsius. Heating is
/// only called for when the zone's Matter mode is `Heat`; any other mode, or a
/// zone with no Matter state, yields `HeatingState::Off` so the relay stays off.
/// The target is returned in both cases so it can still be logged and published.
pub fn get_control_target(zone_index: usize) -> (HeatingState, f32) {
    match get_zone_state(zone_index) {
        Some(state) => match state.heating_demand_celsius() {
            Some(target_c) => (HeatingState::Heat, target_c),
            None => (HeatingState::Off, state.get_setpoint_celsius()),
        },
        None => (HeatingState::Off, emberheat_core::DEFAULT_TEMP_C),
    }
}

/// Get setpoint from Matter for emberheat-core control loop
///
/// Call this in the control loop to get the current target temperature
/// set via HomeKit/Google/Alexa.
///
/// # Arguments
///
/// * `zone_index` - Zero-based zone index (0 to NUM_ZONES-1)
///
/// # Returns
///
/// The setpoint in Celsius, or `None` if zone index is invalid.
pub fn get_matter_setpoint(zone_index: usize) -> Option<f32> {
    get_zone_state(zone_index).map(|state| state.get_setpoint_celsius())
}

/// Check if zone is enabled in Matter (SystemMode == Heat)
///
/// # Arguments
///
/// * `zone_index` - Zero-based zone index (0 to NUM_ZONES-1)
///
/// # Returns
///
/// `true` if the zone is in Heat mode, `false` if Off or invalid index.
pub fn is_zone_enabled_in_matter(zone_index: usize) -> bool {
    get_zone_state(zone_index)
        .map(|state| state.is_enabled())
        .unwrap_or(false)
}

/// Set the Matter setpoint from the control loop
///
/// Use this when the setpoint is changed locally (e.g., physical buttons)
/// to sync to Matter controllers.
///
/// # Arguments
///
/// * `zone_index` - Zero-based zone index (0 to NUM_ZONES-1)
/// * `setpoint_c` - New setpoint in Celsius
///
/// # Returns
///
/// `true` if setpoint was successfully set, `false` if out of range or invalid zone.
pub fn set_matter_setpoint(zone_index: usize, setpoint_c: f32) -> bool {
    get_zone_state(zone_index)
        .map(|state| state.set_setpoint_celsius(setpoint_c).is_ok())
        .unwrap_or(false)
}

/// Set the Matter system mode from the control loop
///
/// Use this when the mode is changed locally (e.g., physical switch)
/// to sync to Matter controllers.
///
/// # Arguments
///
/// * `zone_index` - Zero-based zone index (0 to NUM_ZONES-1)
/// * `enabled` - `true` for Heat mode, `false` for Off
pub fn set_matter_mode(zone_index: usize, enabled: bool) {
    if let Some(state) = get_zone_state(zone_index) {
        let mode = if enabled {
            SystemMode::Heat
        } else {
            SystemMode::Off
        };
        state.set_system_mode(mode);
    }
}

/// Sync local zone state to Matter
///
/// Convenience function to sync all zone state at once.
/// Call this when the local state changes (e.g., from NVS restore).
///
/// # Arguments
///
/// * `zone_index` - Zero-based zone index (0 to NUM_ZONES-1)
/// * `setpoint_c` - Target temperature in Celsius
/// * `enabled` - Whether the zone is enabled (Heat mode)
/// * `current_temp_c` - Optional current temperature (if sensor has been read)
pub fn sync_local_state_to_matter(
    zone_index: usize,
    setpoint_c: f32,
    enabled: bool,
    current_temp_c: Option<f32>,
) {
    if let Some(state) = get_zone_state(zone_index) {
        // Set setpoint (ignore errors for out of range)
        let _ = state.set_setpoint_celsius(setpoint_c);

        // Set mode
        let mode = if enabled {
            SystemMode::Heat
        } else {
            SystemMode::Off
        };
        state.set_system_mode(mode);

        // Set temperature if available
        if let Some(temp) = current_temp_c {
            state.set_local_temperature_celsius(temp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matter::zones::ZONE_STATES;

    #[test]
    fn test_update_matter_from_control_loop() {
        // Update zone 0
        update_matter_from_control_loop(0, 21.5, true);

        // Verify state was updated
        let state = &ZONE_STATES[0];
        assert_eq!(state.get_local_temperature_celsius(), Some(21.5));
        assert!(state.is_heating());

        // Update with heating off
        update_matter_from_control_loop(0, 22.0, false);
        assert_eq!(state.get_local_temperature_celsius(), Some(22.0));
        assert!(!state.is_heating());
    }

    #[test]
    fn test_get_control_target() {
        // Heat mode: heat to the Matter setpoint
        sync_local_state_to_matter(0, 23.0, true, None);
        assert_eq!(get_control_target(0), (HeatingState::Heat, 23.0));

        // Off mode: relay must stay off, setpoint still reported
        set_matter_mode(0, false);
        assert_eq!(get_control_target(0), (HeatingState::Off, 23.0));

        // Unknown zone: off
        assert_eq!(get_control_target(99).0, HeatingState::Off);

        set_matter_mode(0, true);
    }

    #[test]
    fn test_get_matter_setpoint() {
        // Get default setpoint
        let setpoint = get_matter_setpoint(0);
        assert!(setpoint.is_some());
        assert_eq!(setpoint.unwrap(), 20.0); // Default is 20°C

        // Invalid zone
        assert!(get_matter_setpoint(10).is_none());
    }

    #[test]
    fn test_is_zone_enabled_in_matter() {
        // Default is Heat mode (enabled)
        assert!(is_zone_enabled_in_matter(0));

        // Invalid zone
        assert!(!is_zone_enabled_in_matter(10));
    }

    #[test]
    fn test_set_matter_setpoint() {
        // Valid setpoint
        assert!(set_matter_setpoint(1, 22.0));
        assert_eq!(get_matter_setpoint(1), Some(22.0));

        // Out of range (below min 7°C)
        assert!(!set_matter_setpoint(1, 5.0));

        // Invalid zone
        assert!(!set_matter_setpoint(10, 20.0));
    }

    #[test]
    fn test_set_matter_mode() {
        // Disable zone
        set_matter_mode(2, false);
        assert!(!is_zone_enabled_in_matter(2));

        // Re-enable
        set_matter_mode(2, true);
        assert!(is_zone_enabled_in_matter(2));
    }

    #[test]
    fn test_sync_local_state_to_matter() {
        // Sync with temperature
        sync_local_state_to_matter(3, 23.0, true, Some(21.0));

        let state = &ZONE_STATES[3];
        assert_eq!(state.get_setpoint_celsius(), 23.0);
        assert!(state.is_enabled());
        assert_eq!(state.get_local_temperature_celsius(), Some(21.0));

        // Sync without temperature
        sync_local_state_to_matter(4, 19.0, false, None);

        let state = &ZONE_STATES[4];
        assert_eq!(state.get_setpoint_celsius(), 19.0);
        assert!(!state.is_enabled());
    }
}
