/**
 * Safe Rust wrapper for Matter FFI bindings
 *
 * This module provides safe, idiomatic Rust interfaces around the unsafe
 * C FFI bindings to the Connected Home IP library.
 */
use super::bindings::*;
use anyhow::{anyhow, Result};
use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use tracing::{debug, info};

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert Matter error code to Result
fn check_error(error_code: MatterErrorCode) -> Result<(), String> {
    if error_code == MATTER_SUCCESS as i32 {
        Ok(())
    } else {
        let msg = unsafe {
            let ptr = matter_get_error_message(error_code);
            if ptr.is_null() {
                "Unknown error".to_string()
            } else {
                CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };
        Err(msg)
    }
}

// =============================================================================
// Safe Wrapper Types
// =============================================================================

/// Safe wrapper for MatterDevice
pub struct Device {
    ptr: *mut MatterDevice,
}

impl Device {
    /// Create a new Matter device
    pub fn new(config: DeviceConfig) -> Result<Self> {
        let device_name = CString::new(config.device_name.as_str())?;
        let serial_number = CString::new(config.serial_number.as_str())?;

        let c_config = MatterDeviceConfig {
            vendor_id: config.vendor_id,
            product_id: config.product_id,
            device_name: device_name.as_ptr(),
            serial_number: serial_number.as_ptr(),
            discriminator: config.discriminator,
            setup_passcode: config.setup_passcode,
            port: config.port,
        };

        let ptr = unsafe { matter_device_init(&c_config) };

        if ptr.is_null() {
            return Err(anyhow!("Failed to initialize Matter device"));
        }

        info!(
            "Matter device initialized: {} (vendor: 0x{:04X}, product: 0x{:04X})",
            config.device_name, config.vendor_id, config.product_id
        );

        Ok(Device { ptr })
    }

    /// Run the Matter event loop (non-blocking)
    pub fn run_event_loop(&mut self) -> Result<()> {
        let error = unsafe { matter_device_run_event_loop(self.ptr) };
        check_error(error).map_err(|e| anyhow!("Event loop error: {}", e))?;
        Ok(())
    }

    /// Check if device is commissioned
    #[allow(dead_code)]
    pub fn is_commissioned(&self) -> bool {
        unsafe { matter_device_is_commissioned(self.ptr) }
    }

    /// Get commissioning QR code
    pub fn get_qr_code(&self) -> Result<String> {
        let mut buffer = vec![0u8; 256];
        let error = unsafe {
            matter_device_get_qr_code(self.ptr, buffer.as_mut_ptr() as *mut c_char, buffer.len())
        };
        check_error(error).map_err(|e| anyhow!("Failed to get QR code: {}", e))?;

        let cstr = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) };
        Ok(cstr.to_string_lossy().into_owned())
    }

    /// Get manual pairing code
    pub fn get_pairing_code(&self) -> Result<String> {
        let mut buffer = vec![0u8; 32];
        let error = unsafe {
            matter_device_get_pairing_code(
                self.ptr,
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len(),
            )
        };
        check_error(error).map_err(|e| anyhow!("Failed to get pairing code: {}", e))?;

        let cstr = unsafe { CStr::from_ptr(buffer.as_ptr() as *const c_char) };
        Ok(cstr.to_string_lossy().into_owned())
    }

    /// Get raw pointer (for creating thermostats)
    pub(crate) fn as_ptr(&self) -> *mut MatterDevice {
        self.ptr
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                matter_device_shutdown(self.ptr);
            }
            info!("Matter device shutdown");
        }
    }
}

// Device must be Send + Sync for use in async contexts
unsafe impl Send for Device {}
unsafe impl Sync for Device {}

/// Safe wrapper for MatterThermostat
#[allow(dead_code)]
pub struct Thermostat {
    ptr: *mut MatterThermostat,
    #[allow(dead_code)]
    endpoint_id: u16,
    zone_name: String,
}

impl Thermostat {
    /// Create a new Matter thermostat
    pub fn new(device: &Device, config: ThermostatConfig) -> Result<Self> {
        let zone_name_c = CString::new(config.zone_name.as_str())?;

        let c_config = MatterThermostatConfig {
            endpoint_id: config.endpoint_id,
            zone_name: zone_name_c.as_ptr(),
            min_heat_setpoint: config.min_heat_setpoint,
            max_heat_setpoint: config.max_heat_setpoint,
            min_cool_setpoint: config.min_cool_setpoint,
            max_cool_setpoint: config.max_cool_setpoint,
        };

        let ptr = unsafe { matter_thermostat_create(device.as_ptr(), &c_config) };

        if ptr.is_null() {
            return Err(anyhow!("Failed to create Matter thermostat"));
        }

        info!(
            "Matter thermostat created: {} (endpoint: {})",
            config.zone_name, config.endpoint_id
        );

        Ok(Thermostat {
            ptr,
            endpoint_id: config.endpoint_id,
            zone_name: config.zone_name,
        })
    }

    /// Update local temperature reading
    pub fn set_local_temperature(&mut self, temp_celsius: f32) -> Result<()> {
        let centi_degrees = (temp_celsius * 100.0) as i16;
        let error = unsafe { matter_thermostat_set_local_temperature(self.ptr, centi_degrees) };
        check_error(error).map_err(|e| anyhow!("Failed to set temperature: {}", e))?;
        debug!(
            "Thermostat '{}': Updated temperature to {:.1}°C",
            self.zone_name, temp_celsius
        );
        Ok(())
    }

    /// Get heating setpoint
    #[allow(dead_code)]
    pub fn get_heating_setpoint(&self) -> Result<f32> {
        let mut setpoint: i16 = 0;
        let error = unsafe { matter_thermostat_get_heating_setpoint(self.ptr, &mut setpoint) };
        check_error(error).map_err(|e| anyhow!("Failed to get heating setpoint: {}", e))?;
        Ok(setpoint as f32 / 100.0)
    }

    /// Get cooling setpoint
    #[allow(dead_code)]
    pub fn get_cooling_setpoint(&self) -> Result<f32> {
        let mut setpoint: i16 = 0;
        let error = unsafe { matter_thermostat_get_cooling_setpoint(self.ptr, &mut setpoint) };
        check_error(error).map_err(|e| anyhow!("Failed to get cooling setpoint: {}", e))?;
        Ok(setpoint as f32 / 100.0)
    }

    /// Get system mode
    #[allow(dead_code)]
    pub fn get_system_mode(&self) -> Result<ThermostatMode> {
        let mut mode: u8 = 0;
        let error = unsafe { matter_thermostat_get_system_mode(self.ptr, &mut mode) };
        check_error(error).map_err(|e| anyhow!("Failed to get system mode: {}", e))?;
        ThermostatMode::from_u8(mode)
    }

    /// Set running state
    pub fn set_running_state(&mut self, state: ThermostatRunningState) -> Result<()> {
        let error = unsafe { matter_thermostat_set_running_state(self.ptr, state as u16) };
        check_error(error).map_err(|e| anyhow!("Failed to set running state: {}", e))?;
        debug!(
            "Thermostat '{}': Updated running state to {:?}",
            self.zone_name, state
        );
        Ok(())
    }

    /// Get endpoint ID
    #[allow(dead_code)]
    pub fn endpoint_id(&self) -> u16 {
        self.endpoint_id
    }

    /// Get zone name
    #[allow(dead_code)]
    pub fn zone_name(&self) -> &str {
        &self.zone_name
    }
}

impl Drop for Thermostat {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                matter_thermostat_destroy(self.ptr);
            }
            debug!("Matter thermostat '{}' destroyed", self.zone_name);
        }
    }
}

unsafe impl Send for Thermostat {}
unsafe impl Sync for Thermostat {}

// =============================================================================
// Configuration Types
// =============================================================================

/// Device configuration
#[derive(Debug, Clone)]
pub struct DeviceConfig {
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_name: String,
    pub serial_number: String,
    pub discriminator: u16,
    pub setup_passcode: u32,
    pub port: u16,
}

impl Default for DeviceConfig {
    fn default() -> Self {
        Self {
            vendor_id: 0xFFF1,  // Test vendor ID
            product_id: 0x8001, // Thermostat product ID
            device_name: "Thermostat".to_string(),
            serial_number: "TEST123".to_string(),
            discriminator: 3840,      // Default discriminator
            setup_passcode: 20202021, // Default passcode
            port: 5540,               // Standard Matter port
        }
    }
}

/// Thermostat configuration
#[derive(Debug, Clone)]
pub struct ThermostatConfig {
    pub endpoint_id: u16,
    pub zone_name: String,
    pub min_heat_setpoint: i16, // centi-degrees Celsius
    pub max_heat_setpoint: i16,
    pub min_cool_setpoint: i16,
    pub max_cool_setpoint: i16,
}

impl ThermostatConfig {
    pub fn new(endpoint_id: u16, zone_name: String) -> Self {
        Self {
            endpoint_id,
            zone_name,
            min_heat_setpoint: 700,  // 7°C
            max_heat_setpoint: 3500, // 35°C
            min_cool_setpoint: 1600, // 16°C
            max_cool_setpoint: 3200, // 32°C
        }
    }
}

// =============================================================================
// Enum Wrappers
// =============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ThermostatMode {
    Off,
    Auto,
    Cool,
    Heat,
    EmergencyHeat,
    Precooling,
    FanOnly,
}

impl ThermostatMode {
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0x00 => Ok(ThermostatMode::Off),
            0x01 => Ok(ThermostatMode::Auto),
            0x03 => Ok(ThermostatMode::Cool),
            0x04 => Ok(ThermostatMode::Heat),
            0x05 => Ok(ThermostatMode::EmergencyHeat),
            0x06 => Ok(ThermostatMode::Precooling),
            0x07 => Ok(ThermostatMode::FanOnly),
            _ => Err(anyhow!("Invalid thermostat mode: {}", value)),
        }
    }

    #[allow(dead_code)]
    pub fn to_u8(self) -> u8 {
        match self {
            ThermostatMode::Off => 0x00,
            ThermostatMode::Auto => 0x01,
            ThermostatMode::Cool => 0x03,
            ThermostatMode::Heat => 0x04,
            ThermostatMode::EmergencyHeat => 0x05,
            ThermostatMode::Precooling => 0x06,
            ThermostatMode::FanOnly => 0x07,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermostatRunningState {
    Idle,
    Heat,
    Cool,
    Fan,
}

impl From<ThermostatRunningState> for u16 {
    fn from(state: ThermostatRunningState) -> u16 {
        match state {
            ThermostatRunningState::Idle => 0x0000,
            ThermostatRunningState::Heat => 0x0001,
            ThermostatRunningState::Cool => 0x0002,
            ThermostatRunningState::Fan => 0x0004,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // DeviceConfig tests
    #[test]
    fn test_device_config_default() {
        let config = DeviceConfig::default();
        assert_eq!(config.vendor_id, 0xFFF1);
        assert_eq!(config.product_id, 0x8001);
        assert_eq!(config.port, 5540);
        assert_eq!(config.discriminator, 3840);
        assert_eq!(config.setup_passcode, 20202021);
        assert!(!config.device_name.is_empty());
        assert!(!config.serial_number.is_empty());
    }

    #[test]
    fn test_device_config_custom() {
        let mut config = DeviceConfig::default();
        config.vendor_id = 0x1234;
        config.product_id = 0x5678;
        config.port = 9000;
        config.device_name = "Custom Device".to_string();

        assert_eq!(config.vendor_id, 0x1234);
        assert_eq!(config.product_id, 0x5678);
        assert_eq!(config.port, 9000);
        assert_eq!(config.device_name, "Custom Device");
    }

    #[test]
    fn test_device_config_discriminator_range() {
        let config = DeviceConfig::default();
        // Discriminator is 12-bit (0-4095)
        assert!(config.discriminator <= 4095);
    }

    #[test]
    fn test_device_config_passcode_valid() {
        let config = DeviceConfig::default();
        // Setup passcode should be non-zero
        assert_ne!(config.setup_passcode, 0);
    }

    // ThermostatConfig tests
    #[test]
    fn test_thermostat_config_new() {
        let config = ThermostatConfig::new(1, "Zone1".to_string());
        assert_eq!(config.endpoint_id, 1);
        assert_eq!(config.zone_name, "Zone1");
        assert_eq!(config.min_heat_setpoint, 700);  // 7.0°C
        assert_eq!(config.max_heat_setpoint, 3500); // 35.0°C
        assert_eq!(config.min_cool_setpoint, 1600); // 16.0°C
        assert_eq!(config.max_cool_setpoint, 3200); // 32.0°C
    }

    #[test]
    fn test_thermostat_config_custom_endpoint() {
        let config = ThermostatConfig::new(42, "Test Zone".to_string());
        assert_eq!(config.endpoint_id, 42);
        assert_eq!(config.zone_name, "Test Zone");
    }

    #[test]
    fn test_thermostat_config_setpoint_ranges() {
        let config = ThermostatConfig::new(1, "Zone1".to_string());

        // Heat setpoints
        assert!(config.min_heat_setpoint < config.max_heat_setpoint);
        assert!(config.min_heat_setpoint >= 0);

        // Cool setpoints
        assert!(config.min_cool_setpoint < config.max_cool_setpoint);
        assert!(config.min_cool_setpoint > config.min_heat_setpoint);
    }

    #[test]
    fn test_thermostat_config_zone_name_empty() {
        let config = ThermostatConfig::new(1, String::new());
        assert_eq!(config.zone_name, "");
    }

    #[test]
    fn test_thermostat_config_zone_name_special_chars() {
        let config = ThermostatConfig::new(1, "Zone-1_Test (Main)".to_string());
        assert_eq!(config.zone_name, "Zone-1_Test (Main)");
    }

    // ThermostatMode tests
    #[test]
    fn test_thermostat_mode_conversion() {
        assert_eq!(ThermostatMode::from_u8(0x00).unwrap(), ThermostatMode::Off);
        assert_eq!(ThermostatMode::from_u8(0x01).unwrap(), ThermostatMode::Auto);
        assert_eq!(ThermostatMode::from_u8(0x03).unwrap(), ThermostatMode::Cool);
        assert_eq!(ThermostatMode::from_u8(0x04).unwrap(), ThermostatMode::Heat);
        assert_eq!(ThermostatMode::from_u8(0x05).unwrap(), ThermostatMode::EmergencyHeat);
        assert_eq!(ThermostatMode::from_u8(0x06).unwrap(), ThermostatMode::Precooling);
        assert_eq!(ThermostatMode::from_u8(0x07).unwrap(), ThermostatMode::FanOnly);
    }

    #[test]
    fn test_thermostat_mode_invalid() {
        assert!(ThermostatMode::from_u8(0xFF).is_err());
        assert!(ThermostatMode::from_u8(0x02).is_err());
        assert!(ThermostatMode::from_u8(0x08).is_err());
    }

    #[test]
    fn test_thermostat_mode_to_u8() {
        assert_eq!(ThermostatMode::Off.to_u8(), 0x00);
        assert_eq!(ThermostatMode::Auto.to_u8(), 0x01);
        assert_eq!(ThermostatMode::Cool.to_u8(), 0x03);
        assert_eq!(ThermostatMode::Heat.to_u8(), 0x04);
        assert_eq!(ThermostatMode::EmergencyHeat.to_u8(), 0x05);
        assert_eq!(ThermostatMode::Precooling.to_u8(), 0x06);
        assert_eq!(ThermostatMode::FanOnly.to_u8(), 0x07);
    }

    #[test]
    fn test_thermostat_mode_round_trip() {
        let modes = vec![
            ThermostatMode::Off,
            ThermostatMode::Auto,
            ThermostatMode::Cool,
            ThermostatMode::Heat,
            ThermostatMode::EmergencyHeat,
            ThermostatMode::Precooling,
            ThermostatMode::FanOnly,
        ];

        for mode in modes {
            let u8_val = mode.to_u8();
            let converted = ThermostatMode::from_u8(u8_val).unwrap();
            assert_eq!(mode, converted);
        }
    }

    #[test]
    fn test_thermostat_mode_debug() {
        // Verify Debug trait works
        let mode = ThermostatMode::Heat;
        let debug_str = format!("{:?}", mode);
        assert!(debug_str.contains("Heat"));
    }

    // ThermostatRunningState tests
    #[test]
    fn test_running_state_conversion() {
        let state: u16 = ThermostatRunningState::Idle.into();
        assert_eq!(state, 0x0000);

        let state: u16 = ThermostatRunningState::Heat.into();
        assert_eq!(state, 0x0001);

        let state: u16 = ThermostatRunningState::Cool.into();
        assert_eq!(state, 0x0002);

        let state: u16 = ThermostatRunningState::Fan.into();
        assert_eq!(state, 0x0004);
    }

    #[test]
    fn test_running_state_all_values() {
        let states = vec![
            (ThermostatRunningState::Idle, 0x0000),
            (ThermostatRunningState::Heat, 0x0001),
            (ThermostatRunningState::Cool, 0x0002),
            (ThermostatRunningState::Fan, 0x0004),
        ];

        for (state, expected) in states {
            let value: u16 = state.into();
            assert_eq!(value, expected);
        }
    }

    #[test]
    fn test_running_state_values_are_bitflags() {
        // Verify that running state values are powers of 2 (except Idle)
        let heat: u16 = ThermostatRunningState::Heat.into();
        let cool: u16 = ThermostatRunningState::Cool.into();
        let fan: u16 = ThermostatRunningState::Fan.into();

        assert_eq!(heat & (heat - 1), 0); // Power of 2
        assert_eq!(cool & (cool - 1), 0); // Power of 2
        assert_eq!(fan & (fan - 1), 0);   // Power of 2
    }

    #[test]
    fn test_running_state_debug() {
        let state = ThermostatRunningState::Heat;
        let debug_str = format!("{:?}", state);
        assert!(debug_str.contains("Heat"));
    }

    #[test]
    fn test_running_state_clone() {
        let state1 = ThermostatRunningState::Heat;
        let state2 = state1.clone();

        let val1: u16 = state1.into();
        let val2: u16 = state2.into();
        assert_eq!(val1, val2);
    }

    // Temperature conversion tests
    #[test]
    fn test_temperature_centi_degrees() {
        // 20.0°C should be 2000 centi-degrees
        let temp_c = 20.0;
        let centi = (temp_c * 100.0) as i16;
        assert_eq!(centi, 2000);

        // 21.5°C should be 2150 centi-degrees
        let temp_c = 21.5;
        let centi = (temp_c * 100.0) as i16;
        assert_eq!(centi, 2150);
    }

    #[test]
    fn test_temperature_negative() {
        // -5.0°C should be -500 centi-degrees
        let temp_c = -5.0;
        let centi = (temp_c * 100.0) as i16;
        assert_eq!(centi, -500);
    }

    // Helper function tests
    #[test]
    fn test_check_error_success() {
        let result = check_error(MATTER_SUCCESS);
        assert!(result.is_ok());
    }

    #[test]
    fn test_check_error_failure() {
        let result = check_error(MATTER_ERROR_INIT_FAILED);
        assert!(result.is_err());
    }

    #[test]
    fn test_check_error_returns_message() {
        let result = check_error(MATTER_ERROR_INVALID_PARAM);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(!err.is_empty());
    }
}
