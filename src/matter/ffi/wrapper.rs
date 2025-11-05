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
        check_error(error)
            .map_err(|e| anyhow!("Event loop error: {}", e))?;
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
            matter_device_get_qr_code(
                self.ptr,
                buffer.as_mut_ptr() as *mut c_char,
                buffer.len(),
            )
        };
        check_error(error)
            .map_err(|e| anyhow!("Failed to get QR code: {}", e))?;

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
        check_error(error)
            .map_err(|e| anyhow!("Failed to get pairing code: {}", e))?;

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
        check_error(error)
            .map_err(|e| anyhow!("Failed to set temperature: {}", e))?;
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
        check_error(error)
            .map_err(|e| anyhow!("Failed to get heating setpoint: {}", e))?;
        Ok(setpoint as f32 / 100.0)
    }

    /// Get cooling setpoint
    #[allow(dead_code)]
    pub fn get_cooling_setpoint(&self) -> Result<f32> {
        let mut setpoint: i16 = 0;
        let error = unsafe { matter_thermostat_get_cooling_setpoint(self.ptr, &mut setpoint) };
        check_error(error)
            .map_err(|e| anyhow!("Failed to get cooling setpoint: {}", e))?;
        Ok(setpoint as f32 / 100.0)
    }

    /// Get system mode
    #[allow(dead_code)]
    pub fn get_system_mode(&self) -> Result<ThermostatMode> {
        let mut mode: u8 = 0;
        let error = unsafe { matter_thermostat_get_system_mode(self.ptr, &mut mode) };
        check_error(error)
            .map_err(|e| anyhow!("Failed to get system mode: {}", e))?;
        ThermostatMode::from_u8(mode)
    }

    /// Set running state
    pub fn set_running_state(&mut self, state: ThermostatRunningState) -> Result<()> {
        let error = unsafe { matter_thermostat_set_running_state(self.ptr, state as u16) };
        check_error(error)
            .map_err(|e| anyhow!("Failed to set running state: {}", e))?;
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
            vendor_id: 0xFFF1,        // Test vendor ID
            product_id: 0x8001,       // Thermostat product ID
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

    #[test]
    fn test_device_config_default() {
        let config = DeviceConfig::default();
        assert_eq!(config.vendor_id, 0xFFF1);
        assert_eq!(config.port, 5540);
    }

    #[test]
    fn test_thermostat_config_new() {
        let config = ThermostatConfig::new(1, "Zone1".to_string());
        assert_eq!(config.endpoint_id, 1);
        assert_eq!(config.zone_name, "Zone1");
    }

    #[test]
    fn test_thermostat_mode_conversion() {
        assert_eq!(ThermostatMode::from_u8(0x00).unwrap(), ThermostatMode::Off);
        assert_eq!(ThermostatMode::from_u8(0x04).unwrap(), ThermostatMode::Heat);
        assert_eq!(ThermostatMode::Heat.to_u8(), 0x04);
    }

    #[test]
    fn test_running_state_conversion() {
        let state: u16 = ThermostatRunningState::Heat.into();
        assert_eq!(state, 0x0001);
    }
}
