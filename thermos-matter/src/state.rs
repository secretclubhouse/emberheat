//! Thermostat state management
//!
//! Provides thread-safe access to thermostat state for bridging
//! between Matter cluster and thermos-core control loops.
//!
//! # Thread Safety
//!
//! All state access uses atomic operations, making it safe to share
//! between the Matter stack (handling HomeKit/Google/Alexa commands)
//! and the thermos-core control loop (running hysteresis algorithm).

use core::sync::atomic::{AtomicI16, AtomicU16, AtomicU8, Ordering};

use crate::types::{defaults, temperature, RunningState, SystemMode};

/// Thread-safe thermostat state for a single zone
///
/// This container holds all Matter-relevant state for one heating zone.
/// It's designed to be shared between:
/// - Matter cluster handler (responds to HomeKit/Google/Alexa)
/// - thermos-core control loop (manages heating via hysteresis)
///
/// # Example
///
/// ```ignore
/// use thermos_esp32::matter::state::ZoneThermostatState;
///
/// // Create state (usually static for embedded)
/// static ZONE1_STATE: ZoneThermostatState = ZoneThermostatState::new();
///
/// // Matter handler updates setpoint from HomeKit
/// ZONE1_STATE.set_occupied_heating_setpoint(2200).unwrap(); // 22°C
///
/// // Control loop reads setpoint and updates temperature
/// let setpoint = ZONE1_STATE.get_setpoint_celsius(); // 22.0
/// ZONE1_STATE.set_local_temperature_celsius(21.5);
/// ZONE1_STATE.set_heating_active(true);
/// ```
pub struct ZoneThermostatState {
    /// Current temperature in Matter centi-degrees (0.01°C)
    local_temperature: AtomicI16,

    /// Target heating setpoint in Matter centi-degrees
    occupied_heating_setpoint: AtomicI16,

    /// Target cooling setpoint in Matter centi-degrees
    occupied_cooling_setpoint: AtomicI16,

    /// Current system mode (Off/Heat/Cool)
    system_mode: AtomicU8,

    /// Running state bitmap
    running_state: AtomicU16,
}

impl ZoneThermostatState {
    /// Create a new thermostat state with default values
    ///
    /// Defaults:
    /// - Temperature: NULL (sensor not yet read)
    /// - Setpoint: 20°C
    /// - Mode: Heat
    /// - Running state: Not heating
    pub const fn new() -> Self {
        Self {
            local_temperature: AtomicI16::new(temperature::NULL_TEMPERATURE),
            occupied_heating_setpoint: AtomicI16::new(defaults::OCCUPIED_HEATING_SETPOINT),
            occupied_cooling_setpoint: AtomicI16::new(defaults::OCCUPIED_COOLING_SETPOINT),
            system_mode: AtomicU8::new(SystemMode::Heat as u8),
            running_state: AtomicU16::new(0),
        }
    }

    // ---- Temperature getters/setters ----

    /// Get the current local temperature in Matter centi-degrees
    pub fn get_local_temperature(&self) -> i16 {
        self.local_temperature.load(Ordering::Relaxed)
    }

    /// Set the local temperature in Matter centi-degrees
    pub fn set_local_temperature(&self, temp_centi_c: i16) {
        self.local_temperature
            .store(temp_centi_c, Ordering::Relaxed);
    }

    /// Set the local temperature from Celsius
    ///
    /// Converts from Celsius to Matter centi-degrees.
    pub fn set_local_temperature_celsius(&self, temp_c: f32) {
        self.set_local_temperature(temperature::celsius_to_matter(temp_c));
    }

    /// Get the local temperature in Celsius
    ///
    /// Returns `None` if the temperature is the NULL value (sensor unavailable).
    pub fn get_local_temperature_celsius(&self) -> Option<f32> {
        let temp = self.get_local_temperature();
        if temp == temperature::NULL_TEMPERATURE {
            None
        } else {
            Some(temperature::matter_to_celsius(temp))
        }
    }

    /// Check if the temperature reading is valid (not NULL)
    pub fn has_valid_temperature(&self) -> bool {
        self.get_local_temperature() != temperature::NULL_TEMPERATURE
    }

    // ---- Setpoint getters/setters ----

    /// Get the occupied heating setpoint in Matter centi-degrees
    pub fn get_occupied_heating_setpoint(&self) -> i16 {
        self.occupied_heating_setpoint.load(Ordering::Relaxed)
    }

    /// Set the occupied heating setpoint in Matter centi-degrees
    ///
    /// # Errors
    ///
    /// Returns `SetpointError::BelowMinimum` if below 7°C.
    /// Returns `SetpointError::AboveMaximum` if above 30°C.
    pub fn set_occupied_heating_setpoint(
        &self,
        setpoint_centi_c: i16,
    ) -> Result<(), SetpointError> {
        if setpoint_centi_c < defaults::MIN_HEAT_SETPOINT {
            return Err(SetpointError::BelowMinimum);
        }
        if setpoint_centi_c > defaults::MAX_HEAT_SETPOINT {
            return Err(SetpointError::AboveMaximum);
        }
        self.occupied_heating_setpoint
            .store(setpoint_centi_c, Ordering::Relaxed);
        Ok(())
    }

    /// Get the heating setpoint in Celsius
    pub fn get_setpoint_celsius(&self) -> f32 {
        temperature::matter_to_celsius(self.get_occupied_heating_setpoint())
    }

    /// Set the heating setpoint from Celsius
    ///
    /// # Errors
    ///
    /// Returns error if outside valid range (7-30°C).
    pub fn set_setpoint_celsius(&self, temp_c: f32) -> Result<(), SetpointError> {
        self.set_occupied_heating_setpoint(temperature::celsius_to_matter(temp_c))
    }

    // ---- Cooling Setpoint getters/setters ----

    /// Get the occupied cooling setpoint in Matter centi-degrees
    pub fn get_occupied_cooling_setpoint(&self) -> i16 {
        self.occupied_cooling_setpoint.load(Ordering::Relaxed)
    }

    /// Set the occupied cooling setpoint in Matter centi-degrees
    ///
    /// # Errors
    ///
    /// Returns `SetpointError::BelowMinimum` if below 10°C.
    /// Returns `SetpointError::AboveMaximum` if above 35°C.
    pub fn set_occupied_cooling_setpoint(
        &self,
        setpoint_centi_c: i16,
    ) -> Result<(), SetpointError> {
        if setpoint_centi_c < defaults::MIN_COOL_SETPOINT {
            return Err(SetpointError::BelowMinimum);
        }
        if setpoint_centi_c > defaults::MAX_COOL_SETPOINT {
            return Err(SetpointError::AboveMaximum);
        }
        self.occupied_cooling_setpoint
            .store(setpoint_centi_c, Ordering::Relaxed);
        Ok(())
    }

    /// Get the cooling setpoint in Celsius
    pub fn get_cooling_setpoint_celsius(&self) -> f32 {
        temperature::matter_to_celsius(self.get_occupied_cooling_setpoint())
    }

    // ---- System mode getters/setters ----

    /// Get the current system mode
    pub fn get_system_mode(&self) -> SystemMode {
        SystemMode::from_u8(self.system_mode.load(Ordering::Relaxed)).unwrap_or(SystemMode::Off)
    }

    /// Set the system mode
    pub fn set_system_mode(&self, mode: SystemMode) {
        self.system_mode.store(mode as u8, Ordering::Relaxed);
    }

    /// Check if the thermostat is enabled (mode is Heat or Cool)
    pub fn is_enabled(&self) -> bool {
        matches!(self.get_system_mode(), SystemMode::Heat | SystemMode::Cool)
    }

    // ---- Running state getters/setters ----

    /// Get the running state bitmap
    pub fn get_running_state(&self) -> RunningState {
        RunningState(self.running_state.load(Ordering::Relaxed))
    }

    /// Set whether heating is currently active
    pub fn set_heating_active(&self, active: bool) {
        let mut state = self.get_running_state();
        state.set_heating(active);
        self.running_state.store(state.0, Ordering::Relaxed);
    }

    /// Check if heating is currently active
    pub fn is_heating(&self) -> bool {
        self.get_running_state().is_heating()
    }

    /// Set whether cooling is currently active
    pub fn set_cooling_active(&self, active: bool) {
        let mut state = self.get_running_state();
        state.set_cooling(active);
        self.running_state.store(state.0, Ordering::Relaxed);
    }

    /// Check if cooling is currently active
    pub fn is_cooling(&self) -> bool {
        self.get_running_state().is_cooling()
    }
}

impl Default for ZoneThermostatState {
    fn default() -> Self {
        Self::new()
    }
}

/// Error when setting an invalid setpoint
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetpointError {
    /// Setpoint is below the minimum (7°C)
    BelowMinimum,
    /// Setpoint is above the maximum (30°C)
    AboveMaximum,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state_defaults() {
        let state = ZoneThermostatState::new();

        // Temperature should be NULL
        assert_eq!(state.get_local_temperature(), temperature::NULL_TEMPERATURE);
        assert!(!state.has_valid_temperature());
        assert!(state.get_local_temperature_celsius().is_none());

        // Setpoint should be 20°C
        assert_eq!(
            state.get_occupied_heating_setpoint(),
            defaults::OCCUPIED_HEATING_SETPOINT
        );
        assert_eq!(state.get_setpoint_celsius(), 20.0);

        // Mode should be Heat
        assert_eq!(state.get_system_mode(), SystemMode::Heat);
        assert!(state.is_enabled());

        // Should not be heating
        assert!(!state.is_heating());
    }

    #[test]
    fn test_temperature_operations() {
        let state = ZoneThermostatState::new();

        // Set via Celsius
        state.set_local_temperature_celsius(21.5);
        assert_eq!(state.get_local_temperature(), 2150);
        assert!(state.has_valid_temperature());
        assert_eq!(state.get_local_temperature_celsius(), Some(21.5));

        // Set via centi-degrees
        state.set_local_temperature(1850);
        assert_eq!(state.get_local_temperature_celsius(), Some(18.5));
    }

    #[test]
    fn test_setpoint_valid_range() {
        let state = ZoneThermostatState::new();

        // Valid setpoint
        assert!(state.set_occupied_heating_setpoint(2200).is_ok());
        assert_eq!(state.get_setpoint_celsius(), 22.0);

        // Valid via Celsius
        assert!(state.set_setpoint_celsius(18.0).is_ok());
        assert_eq!(state.get_occupied_heating_setpoint(), 1800);

        // Boundary: exactly minimum (7°C)
        assert!(state.set_occupied_heating_setpoint(700).is_ok());

        // Boundary: exactly maximum (30°C)
        assert!(state.set_occupied_heating_setpoint(3000).is_ok());
    }

    #[test]
    fn test_setpoint_below_minimum() {
        let state = ZoneThermostatState::new();

        // Below minimum
        assert_eq!(
            state.set_occupied_heating_setpoint(500),
            Err(SetpointError::BelowMinimum)
        );
        assert_eq!(
            state.set_setpoint_celsius(5.0),
            Err(SetpointError::BelowMinimum)
        );

        // Setpoint should not have changed
        assert_eq!(state.get_setpoint_celsius(), 20.0);
    }

    #[test]
    fn test_setpoint_above_maximum() {
        let state = ZoneThermostatState::new();

        // Above maximum
        assert_eq!(
            state.set_occupied_heating_setpoint(3500),
            Err(SetpointError::AboveMaximum)
        );
        assert_eq!(
            state.set_setpoint_celsius(35.0),
            Err(SetpointError::AboveMaximum)
        );

        // Setpoint should not have changed
        assert_eq!(state.get_setpoint_celsius(), 20.0);
    }

    #[test]
    fn test_system_mode() {
        let state = ZoneThermostatState::new();

        // Default is Heat
        assert_eq!(state.get_system_mode(), SystemMode::Heat);
        assert!(state.is_enabled());

        // Turn off
        state.set_system_mode(SystemMode::Off);
        assert_eq!(state.get_system_mode(), SystemMode::Off);
        assert!(!state.is_enabled());

        // Turn back on
        state.set_system_mode(SystemMode::Heat);
        assert!(state.is_enabled());
    }

    #[test]
    fn test_heating_state() {
        let state = ZoneThermostatState::new();

        // Initially not heating
        assert!(!state.is_heating());
        assert_eq!(state.get_running_state().0, 0);

        // Start heating
        state.set_heating_active(true);
        assert!(state.is_heating());
        assert_eq!(state.get_running_state().0, RunningState::HEAT);

        // Stop heating
        state.set_heating_active(false);
        assert!(!state.is_heating());
        assert_eq!(state.get_running_state().0, 0);
    }

    #[test]
    fn test_default_trait() {
        let state = ZoneThermostatState::default();
        assert_eq!(state.get_system_mode(), SystemMode::Heat);
        assert_eq!(state.get_setpoint_celsius(), 20.0);
    }

    #[test]
    fn test_cooling_setpoint_operations() {
        let state = ZoneThermostatState::new();

        // Default is 24°C
        assert_eq!(state.get_occupied_cooling_setpoint(), 2400);
        assert_eq!(state.get_cooling_setpoint_celsius(), 24.0);

        // Valid setpoint
        assert!(state.set_occupied_cooling_setpoint(2600).is_ok());
        assert_eq!(state.get_cooling_setpoint_celsius(), 26.0);

        // Below minimum
        assert_eq!(
            state.set_occupied_cooling_setpoint(500),
            Err(SetpointError::BelowMinimum)
        );

        // Above maximum
        assert_eq!(
            state.set_occupied_cooling_setpoint(4000),
            Err(SetpointError::AboveMaximum)
        );
    }

    #[test]
    fn test_cooling_state() {
        let state = ZoneThermostatState::new();

        assert!(!state.is_cooling());

        state.set_cooling_active(true);
        assert!(state.is_cooling());

        state.set_cooling_active(false);
        assert!(!state.is_cooling());
    }

    #[test]
    fn test_is_enabled_includes_cool() {
        let state = ZoneThermostatState::new();

        state.set_system_mode(SystemMode::Cool);
        assert!(state.is_enabled());

        state.set_system_mode(SystemMode::Off);
        assert!(!state.is_enabled());
    }
}

// -----------------------------------------------------------------------------
// ThermostatHooks Implementation
// -----------------------------------------------------------------------------

use crate::cluster::{
    ControlSequenceOfOperationEnum, RelayStateBitmap, SystemModeEnum, ThermostatHooks,
    HEATING_ONLY_CLUSTER,
};
use rs_matter::dm::Cluster;
use rs_matter::error::{Error, ErrorCode};
use rs_matter::tlv::Nullable;

impl ThermostatHooks for ZoneThermostatState {
    const CLUSTER: Cluster<'static> = HEATING_ONLY_CLUSTER;

    fn local_temperature(&self) -> Nullable<i16> {
        let temp = self.get_local_temperature();
        if temp == temperature::NULL_TEMPERATURE {
            Nullable::none()
        } else {
            Nullable::some(temp)
        }
    }

    fn occupied_heating_setpoint(&self) -> i16 {
        self.get_occupied_heating_setpoint()
    }

    fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), Error> {
        ZoneThermostatState::set_occupied_heating_setpoint(self, value)
            .map_err(|_| ErrorCode::ConstraintError.into())
    }

    fn min_heat_setpoint_limit(&self) -> i16 {
        defaults::MIN_HEAT_SETPOINT
    }

    fn max_heat_setpoint_limit(&self) -> i16 {
        defaults::MAX_HEAT_SETPOINT
    }

    fn system_mode(&self) -> SystemModeEnum {
        match self.get_system_mode() {
            SystemMode::Off => SystemModeEnum::Off,
            SystemMode::Heat => SystemModeEnum::Heat,
            // Map unsupported modes to Off for safety
            _ => SystemModeEnum::Off,
        }
    }

    fn set_system_mode(&self, mode: SystemModeEnum) -> Result<(), Error> {
        let our_mode = match mode {
            SystemModeEnum::Off => SystemMode::Off,
            SystemModeEnum::Heat => SystemMode::Heat,
            // Reject unsupported modes
            _ => return Err(ErrorCode::ConstraintError.into()),
        };
        ZoneThermostatState::set_system_mode(self, our_mode);
        Ok(())
    }

    fn thermostat_running_state(&self) -> RelayStateBitmap {
        if self.is_heating() {
            RelayStateBitmap::HEAT
        } else {
            RelayStateBitmap::empty()
        }
    }

    fn control_sequence_of_operation(&self) -> ControlSequenceOfOperationEnum {
        // Heating-only thermostat
        ControlSequenceOfOperationEnum::HeatingOnly
    }

    // Use default run() implementation - pends forever
    // The control loop will update state directly
}
