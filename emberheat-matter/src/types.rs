//! Matter Thermostat Cluster type definitions
//!
//! Based on Matter Specification 1.2, Cluster 0x0201
//!
//! This module defines the constants, enums, and conversion utilities needed
//! for implementing a heating-only thermostat cluster.

/// Thermostat Cluster ID
pub const CLUSTER_ID: u32 = 0x0201;

/// Thermostat Device Type ID
pub const DEVICE_TYPE_ID: u32 = 0x0301;

/// Attribute IDs for Thermostat Cluster
pub mod attribute {
    /// Current temperature in 0.01°C units (read-only)
    pub const LOCAL_TEMPERATURE: u16 = 0x0000;
    /// Target heating setpoint in 0.01°C units (read/write)
    pub const OCCUPIED_HEATING_SETPOINT: u16 = 0x0012;
    /// Minimum allowed heating setpoint (read-only)
    pub const MIN_HEAT_SETPOINT_LIMIT: u16 = 0x0015;
    /// Maximum allowed heating setpoint (read-only)
    pub const MAX_HEAT_SETPOINT_LIMIT: u16 = 0x0016;
    /// Control sequence (heating-only, cooling-only, etc.) (read-only)
    pub const CONTROL_SEQUENCE_OF_OPERATION: u16 = 0x001B;
    /// Current system mode (Off, Heat, etc.) (read/write)
    pub const SYSTEM_MODE: u16 = 0x001C;
    /// Running state bitmap (heating active, etc.) (read-only)
    pub const THERMOSTAT_RUNNING_STATE: u16 = 0x0029;
}

/// System Mode values (enum8)
///
/// Defines the operating mode of the thermostat.
/// For heating-only systems, only Off and Heat are valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SystemMode {
    /// Thermostat is off
    Off = 0x00,
    /// Auto mode (not used for heating-only)
    Auto = 0x01,
    /// Cooling mode (not used for heating-only)
    Cool = 0x03,
    /// Heating mode
    Heat = 0x04,
    /// Emergency heat (not used)
    EmergencyHeat = 0x05,
}

impl SystemMode {
    /// Convert from u8 value to SystemMode
    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x00 => Some(Self::Off),
            0x01 => Some(Self::Auto),
            0x03 => Some(Self::Cool),
            0x04 => Some(Self::Heat),
            0x05 => Some(Self::EmergencyHeat),
            _ => None,
        }
    }

    /// Check if this mode is valid for a heating-only thermostat
    pub fn is_valid_for_heating_only(&self) -> bool {
        matches!(self, Self::Off | Self::Heat)
    }

    /// Check if this mode is valid for a heating+cooling thermostat
    pub fn is_valid_for_heat_cool(&self) -> bool {
        matches!(self, Self::Off | Self::Heat | Self::Cool)
    }
}

impl From<SystemMode> for u8 {
    fn from(mode: SystemMode) -> u8 {
        mode as u8
    }
}

/// Control Sequence of Operation values (enum8)
///
/// Defines the type of HVAC system the thermostat controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ControlSequence {
    /// Cooling only system
    CoolingOnly = 0x00,
    /// Cooling with reheat
    CoolingWithReheat = 0x01,
    /// Heating only system (used by emberheat)
    HeatingOnly = 0x02,
    /// Heating with reheat
    HeatingWithReheat = 0x03,
    /// Both cooling and heating
    CoolingAndHeating = 0x04,
    /// Both cooling and heating with reheat
    CoolingAndHeatingWithReheat = 0x05,
}

/// Thermostat Running State bitmap (bitmap16)
///
/// Indicates which HVAC functions are currently active.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunningState(pub u16);

impl RunningState {
    /// Heat output is active (Bit 0)
    pub const HEAT: u16 = 1 << 0;
    /// Cool output is active (Bit 1)
    pub const COOL: u16 = 1 << 1;
    /// Fan is active (Bit 2)
    pub const FAN: u16 = 1 << 2;
    /// Second stage heat is active (Bit 3)
    pub const HEAT_STAGE_2: u16 = 1 << 3;
    /// Second stage cool is active (Bit 4)
    pub const COOL_STAGE_2: u16 = 1 << 4;
    /// Second stage fan is active (Bit 5)
    pub const FAN_STAGE_2: u16 = 1 << 5;
    /// Third stage fan is active (Bit 6)
    pub const FAN_STAGE_3: u16 = 1 << 6;

    /// Create a new empty running state
    pub const fn new() -> Self {
        Self(0)
    }

    /// Set whether heating is active
    pub fn set_heating(&mut self, active: bool) {
        if active {
            self.0 |= Self::HEAT;
        } else {
            self.0 &= !Self::HEAT;
        }
    }

    /// Check if heating is currently active
    pub fn is_heating(&self) -> bool {
        self.0 & Self::HEAT != 0
    }

    /// Set whether cooling is active
    pub fn set_cooling(&mut self, active: bool) {
        if active {
            self.0 |= Self::COOL;
        } else {
            self.0 &= !Self::COOL;
        }
    }

    /// Check if cooling is currently active
    pub fn is_cooling(&self) -> bool {
        self.0 & Self::COOL != 0
    }
}

/// Temperature conversion utilities
///
/// Matter uses centi-degrees (0.01°C) for temperature values.
pub mod temperature {
    /// Convert Celsius to Matter centi-degrees (0.01°C units)
    ///
    /// # Examples
    /// ```
    /// use emberheat_matter::types::temperature::celsius_to_matter;
    /// assert_eq!(celsius_to_matter(21.5), 2150);
    /// assert_eq!(celsius_to_matter(0.0), 0);
    /// assert_eq!(celsius_to_matter(-10.0), -1000);
    /// ```
    pub fn celsius_to_matter(temp_c: f32) -> i16 {
        (temp_c * 100.0) as i16
    }

    /// Convert Matter centi-degrees to Celsius
    ///
    /// # Examples
    /// ```
    /// use emberheat_matter::types::temperature::matter_to_celsius;
    /// assert_eq!(matter_to_celsius(2150), 21.5);
    /// assert_eq!(matter_to_celsius(0), 0.0);
    /// assert_eq!(matter_to_celsius(-1000), -10.0);
    /// ```
    pub fn matter_to_celsius(centi_c: i16) -> f32 {
        (centi_c as f32) / 100.0
    }

    /// Null temperature value (sensor unavailable)
    ///
    /// Used when the sensor reading is not available.
    pub const NULL_TEMPERATURE: i16 = 0x8000_u16 as i16;
}

/// Default configuration values for the thermostat
pub mod defaults {
    /// Default setpoint: 20°C (2000 centi-degrees)
    pub const OCCUPIED_HEATING_SETPOINT: i16 = 2000;

    /// Minimum setpoint: 7°C (frost protection)
    pub const MIN_HEAT_SETPOINT: i16 = 700;

    /// Maximum setpoint: 30°C
    pub const MAX_HEAT_SETPOINT: i16 = 3000;

    /// Default cooling setpoint: 24°C (2400 centi-degrees)
    pub const OCCUPIED_COOLING_SETPOINT: i16 = 2400;

    /// Minimum cooling setpoint: 10°C
    pub const MIN_COOL_SETPOINT: i16 = 1000;

    /// Maximum cooling setpoint: 35°C
    pub const MAX_COOL_SETPOINT: i16 = 3500;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_celsius_to_matter() {
        assert_eq!(temperature::celsius_to_matter(21.5), 2150);
        assert_eq!(temperature::celsius_to_matter(0.0), 0);
        assert_eq!(temperature::celsius_to_matter(-10.0), -1000);
        assert_eq!(temperature::celsius_to_matter(20.0), 2000);
    }

    #[test]
    fn test_temperature_matter_to_celsius() {
        assert_eq!(temperature::matter_to_celsius(2150), 21.5);
        assert_eq!(temperature::matter_to_celsius(0), 0.0);
        assert_eq!(temperature::matter_to_celsius(-1000), -10.0);
        assert_eq!(temperature::matter_to_celsius(2000), 20.0);
    }

    #[test]
    fn test_system_mode_from_u8() {
        assert_eq!(SystemMode::from_u8(0x00), Some(SystemMode::Off));
        assert_eq!(SystemMode::from_u8(0x01), Some(SystemMode::Auto));
        assert_eq!(SystemMode::from_u8(0x03), Some(SystemMode::Cool));
        assert_eq!(SystemMode::from_u8(0x04), Some(SystemMode::Heat));
        assert_eq!(SystemMode::from_u8(0x05), Some(SystemMode::EmergencyHeat));
        assert_eq!(SystemMode::from_u8(0x02), None);
        assert_eq!(SystemMode::from_u8(0xFF), None);
    }

    #[test]
    fn test_system_mode_to_u8() {
        assert_eq!(u8::from(SystemMode::Off), 0x00);
        assert_eq!(u8::from(SystemMode::Heat), 0x04);
        assert_eq!(u8::from(SystemMode::Cool), 0x03);
    }

    #[test]
    fn test_system_mode_valid_for_heating_only() {
        assert!(SystemMode::Off.is_valid_for_heating_only());
        assert!(SystemMode::Heat.is_valid_for_heating_only());
        assert!(!SystemMode::Cool.is_valid_for_heating_only());
        assert!(!SystemMode::Auto.is_valid_for_heating_only());
    }

    #[test]
    fn test_running_state() {
        let mut state = RunningState::new();
        assert!(!state.is_heating());
        assert_eq!(state.0, 0);

        state.set_heating(true);
        assert!(state.is_heating());
        assert_eq!(state.0, RunningState::HEAT);

        state.set_heating(false);
        assert!(!state.is_heating());
        assert_eq!(state.0, 0);
    }

    #[test]
    fn test_running_state_default() {
        let state = RunningState::default();
        assert_eq!(state.0, 0);
        assert!(!state.is_heating());
    }

    #[test]
    fn test_defaults() {
        // Verify defaults match expected values
        assert_eq!(defaults::OCCUPIED_HEATING_SETPOINT, 2000); // 20°C
        assert_eq!(defaults::MIN_HEAT_SETPOINT, 700); // 7°C
        assert_eq!(defaults::MAX_HEAT_SETPOINT, 3000); // 30°C
    }

    #[test]
    fn test_null_temperature() {
        // NULL_TEMPERATURE should be the minimum i16 value
        assert_eq!(temperature::NULL_TEMPERATURE, i16::MIN);
    }

    #[test]
    fn test_cooling_defaults() {
        assert_eq!(defaults::OCCUPIED_COOLING_SETPOINT, 2400); // 24°C
        assert_eq!(defaults::MIN_COOL_SETPOINT, 1000); // 10°C
        assert_eq!(defaults::MAX_COOL_SETPOINT, 3500); // 35°C
    }

    #[test]
    fn test_system_mode_valid_for_heat_cool() {
        assert!(SystemMode::Off.is_valid_for_heat_cool());
        assert!(SystemMode::Heat.is_valid_for_heat_cool());
        assert!(SystemMode::Cool.is_valid_for_heat_cool());
        assert!(!SystemMode::Auto.is_valid_for_heat_cool());
    }

    #[test]
    fn test_running_state_cooling() {
        let mut state = RunningState::new();
        assert!(!state.is_cooling());

        state.set_cooling(true);
        assert!(state.is_cooling());
        assert_eq!(state.0, RunningState::COOL);

        state.set_cooling(false);
        assert!(!state.is_cooling());
    }
}
