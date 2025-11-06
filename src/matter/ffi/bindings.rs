//! Rust FFI bindings for Matter C wrapper
//!
//! These bindings provide a Rust interface to the C wrapper around the
//! Connected Home IP C++ library.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(dead_code)]

use std::os::raw::{c_char, c_int, c_void};

// =============================================================================
// Error Codes
// =============================================================================

pub type MatterErrorCode = c_int;

pub const MATTER_SUCCESS: MatterErrorCode = 0;
pub const MATTER_ERROR_INIT_FAILED: MatterErrorCode = 1;
pub const MATTER_ERROR_INVALID_PARAM: MatterErrorCode = 2;
pub const MATTER_ERROR_NOT_INITIALIZED: MatterErrorCode = 3;
pub const MATTER_ERROR_NETWORK_FAILED: MatterErrorCode = 4;
pub const MATTER_ERROR_COMMISSIONING_FAILED: MatterErrorCode = 5;
pub const MATTER_ERROR_CLUSTER_FAILED: MatterErrorCode = 6;
pub const MATTER_ERROR_UNKNOWN: MatterErrorCode = 99;

// =============================================================================
// Opaque Types
// =============================================================================

/// Opaque handle to Matter device
#[repr(C)]
pub struct MatterDevice {
    _unused: [u8; 0],
}

/// Opaque handle to Matter thermostat
#[repr(C)]
pub struct MatterThermostat {
    _unused: [u8; 0],
}

// =============================================================================
// Enums
// =============================================================================

/// System mode values (Matter spec)
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatterThermostatMode {
    Off = 0x00,
    Auto = 0x01,
    Cool = 0x03,
    Heat = 0x04,
    EmergencyHeat = 0x05,
    Precooling = 0x06,
    FanOnly = 0x07,
}

/// Running state flags (Matter spec)
#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatterThermostatRunningState {
    Idle = 0x0000,
    Heat = 0x0001,
    Cool = 0x0002,
    Fan = 0x0004,
    HeatStage2 = 0x0008,
    CoolStage2 = 0x0010,
    FanStage2 = 0x0020,
    FanStage3 = 0x0040,
}

// =============================================================================
// Structures
// =============================================================================

/// Device configuration
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MatterDeviceConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_name: *const c_char,
    pub serial_number: *const c_char,
    pub discriminator: u16,
    pub setup_passcode: u32,
    pub port: u16,
}

/// Thermostat configuration
#[repr(C)]
#[derive(Debug, Clone)]
pub struct MatterThermostatConfig {
    pub endpoint_id: u16,
    pub zone_name: *const c_char,
    pub min_heat_setpoint: i16,
    pub max_heat_setpoint: i16,
    pub min_cool_setpoint: i16,
    pub max_cool_setpoint: i16,
}

// =============================================================================
// Callback Types
// =============================================================================

/// Callback for thermostat setpoint changes
pub type MatterThermostatSetpointChangedCallback = extern "C" fn(
    context: *mut c_void,
    endpoint_id: u16,
    heating_setpoint: i16,
    cooling_setpoint: i16,
);

/// Callback for thermostat mode changes
pub type MatterThermostatModeChangedCallback = extern "C" fn(
    context: *mut c_void,
    endpoint_id: u16,
    mode: u8, // MatterThermostatMode as u8
);

/// Callback for commissioning complete
pub type MatterCommissioningCompleteCallback = extern "C" fn(context: *mut c_void, success: bool);

// =============================================================================
// External C Functions
// =============================================================================

extern "C" {
    // -------------------------------------------------------------------------
    // Device Lifecycle Functions
    // -------------------------------------------------------------------------

    pub fn matter_device_init(config: *const MatterDeviceConfig) -> *mut MatterDevice;

    pub fn matter_device_shutdown(device: *mut MatterDevice);

    pub fn matter_device_run_event_loop(device: *mut MatterDevice) -> MatterErrorCode;

    pub fn matter_device_is_commissioned(device: *mut MatterDevice) -> bool;

    // -------------------------------------------------------------------------
    // Commissioning Functions
    // -------------------------------------------------------------------------

    pub fn matter_device_get_qr_code(
        device: *mut MatterDevice,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> MatterErrorCode;

    pub fn matter_device_get_pairing_code(
        device: *mut MatterDevice,
        buffer: *mut c_char,
        buffer_size: usize,
    ) -> MatterErrorCode;

    pub fn matter_device_set_commissioning_callback(
        device: *mut MatterDevice,
        callback: MatterCommissioningCompleteCallback,
        context: *mut c_void,
    );

    // -------------------------------------------------------------------------
    // Thermostat Functions
    // -------------------------------------------------------------------------

    pub fn matter_thermostat_create(
        device: *mut MatterDevice,
        config: *const MatterThermostatConfig,
    ) -> *mut MatterThermostat;

    pub fn matter_thermostat_destroy(thermostat: *mut MatterThermostat);

    pub fn matter_thermostat_set_local_temperature(
        thermostat: *mut MatterThermostat,
        temperature: i16,
    ) -> MatterErrorCode;

    pub fn matter_thermostat_get_heating_setpoint(
        thermostat: *mut MatterThermostat,
        setpoint: *mut i16,
    ) -> MatterErrorCode;

    pub fn matter_thermostat_get_cooling_setpoint(
        thermostat: *mut MatterThermostat,
        setpoint: *mut i16,
    ) -> MatterErrorCode;

    pub fn matter_thermostat_get_system_mode(
        thermostat: *mut MatterThermostat,
        mode: *mut u8,
    ) -> MatterErrorCode;

    pub fn matter_thermostat_set_running_state(
        thermostat: *mut MatterThermostat,
        state: u16,
    ) -> MatterErrorCode;

    pub fn matter_thermostat_set_setpoint_callback(
        thermostat: *mut MatterThermostat,
        callback: MatterThermostatSetpointChangedCallback,
        context: *mut c_void,
    );

    pub fn matter_thermostat_set_mode_callback(
        thermostat: *mut MatterThermostat,
        callback: MatterThermostatModeChangedCallback,
        context: *mut c_void,
    );

    // -------------------------------------------------------------------------
    // Utility Functions
    // -------------------------------------------------------------------------

    pub fn matter_get_error_message(error: MatterErrorCode) -> *const c_char;

    pub fn matter_get_version() -> *const c_char;
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert MatterErrorCode to Result
pub fn check_error(error: MatterErrorCode) -> Result<(), String> {
    if error == MATTER_SUCCESS {
        Ok(())
    } else {
        unsafe {
            let msg_ptr = matter_get_error_message(error);
            let msg = if !msg_ptr.is_null() {
                std::ffi::CStr::from_ptr(msg_ptr)
                    .to_string_lossy()
                    .into_owned()
            } else {
                format!("Matter error code: {}", error)
            };
            Err(msg)
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_codes() {
        assert_eq!(MATTER_SUCCESS, 0);
        assert_eq!(MATTER_ERROR_INIT_FAILED, 1);
        assert_eq!(MATTER_ERROR_INVALID_PARAM, 2);
        assert_eq!(MATTER_ERROR_NOT_INITIALIZED, 3);
        assert_eq!(MATTER_ERROR_NETWORK_FAILED, 4);
        assert_eq!(MATTER_ERROR_COMMISSIONING_FAILED, 5);
        assert_eq!(MATTER_ERROR_CLUSTER_FAILED, 6);
        assert_eq!(MATTER_ERROR_UNKNOWN, 99);
    }

    #[test]
    fn test_error_codes_unique() {
        // Ensure all error codes are unique
        let codes = [
            MATTER_SUCCESS,
            MATTER_ERROR_INIT_FAILED,
            MATTER_ERROR_INVALID_PARAM,
            MATTER_ERROR_NOT_INITIALIZED,
            MATTER_ERROR_NETWORK_FAILED,
            MATTER_ERROR_COMMISSIONING_FAILED,
            MATTER_ERROR_CLUSTER_FAILED,
            MATTER_ERROR_UNKNOWN,
        ];

        for (i, &code1) in codes.iter().enumerate() {
            for (j, &code2) in codes.iter().enumerate() {
                if i != j {
                    assert_ne!(code1, code2, "Error codes must be unique");
                }
            }
        }
    }

    #[test]
    fn test_thermostat_mode_values() {
        assert_eq!(MatterThermostatMode::Off as u8, 0x00);
        assert_eq!(MatterThermostatMode::Auto as u8, 0x01);
        assert_eq!(MatterThermostatMode::Cool as u8, 0x03);
        assert_eq!(MatterThermostatMode::Heat as u8, 0x04);
        assert_eq!(MatterThermostatMode::EmergencyHeat as u8, 0x05);
        assert_eq!(MatterThermostatMode::Precooling as u8, 0x06);
        assert_eq!(MatterThermostatMode::FanOnly as u8, 0x07);
    }

    #[test]
    fn test_thermostat_mode_ordering() {
        // Verify mode values follow Matter spec
        assert!((MatterThermostatMode::Off as u8) < (MatterThermostatMode::Auto as u8));
        assert!((MatterThermostatMode::Auto as u8) < (MatterThermostatMode::Cool as u8));
        assert!((MatterThermostatMode::Cool as u8) < (MatterThermostatMode::Heat as u8));
    }

    #[test]
    fn test_running_state_values() {
        assert_eq!(MatterThermostatRunningState::Idle as u16, 0x0000);
        assert_eq!(MatterThermostatRunningState::Heat as u16, 0x0001);
        assert_eq!(MatterThermostatRunningState::Cool as u16, 0x0002);
        assert_eq!(MatterThermostatRunningState::Fan as u16, 0x0004);
        assert_eq!(MatterThermostatRunningState::HeatStage2 as u16, 0x0008);
        assert_eq!(MatterThermostatRunningState::CoolStage2 as u16, 0x0010);
        assert_eq!(MatterThermostatRunningState::FanStage2 as u16, 0x0020);
        assert_eq!(MatterThermostatRunningState::FanStage3 as u16, 0x0040);
    }

    #[test]
    fn test_running_state_flags() {
        // Test that running states are bitflags (power of 2 or 0)
        let states = vec![
            MatterThermostatRunningState::Idle as u16,
            MatterThermostatRunningState::Heat as u16,
            MatterThermostatRunningState::Cool as u16,
            MatterThermostatRunningState::Fan as u16,
            MatterThermostatRunningState::HeatStage2 as u16,
            MatterThermostatRunningState::CoolStage2 as u16,
            MatterThermostatRunningState::FanStage2 as u16,
            MatterThermostatRunningState::FanStage3 as u16,
        ];

        for &state in &states {
            if state == 0 {
                continue; // Idle is valid 0
            }
            // Check if it's a power of 2 (only one bit set)
            assert_eq!(state & (state - 1), 0, "{} should be a power of 2", state);
        }
    }

    #[test]
    fn test_running_state_combinations() {
        // Test that running states can be combined with bitwise OR
        let heat_and_fan =
            MatterThermostatRunningState::Heat as u16 | MatterThermostatRunningState::Fan as u16;
        assert_eq!(heat_and_fan, 0x0005);

        let cool_stage_2_and_fan = MatterThermostatRunningState::CoolStage2 as u16
            | MatterThermostatRunningState::Fan as u16;
        assert_eq!(cool_stage_2_and_fan, 0x0014);
    }

    #[test]
    fn test_check_error() {
        assert!(check_error(MATTER_SUCCESS).is_ok());
        assert!(check_error(MATTER_ERROR_INIT_FAILED).is_err());
        assert!(check_error(MATTER_ERROR_INVALID_PARAM).is_err());
        assert!(check_error(MATTER_ERROR_NOT_INITIALIZED).is_err());
    }

    #[test]
    fn test_check_error_messages() {
        // Verify error messages are present
        let err = check_error(MATTER_ERROR_INIT_FAILED);
        assert!(err.is_err());
        let err_msg = err.unwrap_err();
        assert!(!err_msg.is_empty());
        assert!(err_msg.contains("Initialization failed") || err_msg.contains("init"));
    }

    #[test]
    fn test_device_config_struct_size() {
        // Verify struct is C-compatible (reasonable size)
        let size = std::mem::size_of::<MatterDeviceConfig>();
        assert!(size > 0);
        assert!(size < 1024); // Should be relatively small
    }

    #[test]
    fn test_thermostat_config_struct_size() {
        let size = std::mem::size_of::<MatterThermostatConfig>();
        assert!(size > 0);
        assert!(size < 256);
    }

    #[test]
    fn test_opaque_types_are_pointers() {
        // Verify opaque types are represented as pointers
        assert_eq!(
            std::mem::size_of::<*mut MatterDevice>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<*mut MatterThermostat>(),
            std::mem::size_of::<usize>()
        );
    }

    #[test]
    fn test_callback_types_are_function_pointers() {
        // Verify callback types are function pointers
        assert_eq!(
            std::mem::size_of::<MatterThermostatSetpointChangedCallback>(),
            std::mem::size_of::<usize>()
        );
        assert_eq!(
            std::mem::size_of::<MatterCommissioningCompleteCallback>(),
            std::mem::size_of::<usize>()
        );
    }
}
