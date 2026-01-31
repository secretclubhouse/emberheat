//! Persistence data types
//!
//! Defines the data structures for persisted state.

use thermos_matter::types::SystemMode;

/// Key base for zone data in KvBlobStore
/// Keys 0x0000-0x0FFF are reserved for rs-matter
pub const ZONE_KEY_BASE: u16 = 0x1000;

/// Calculate storage key for a zone index
pub const fn zone_key(zone_index: usize) -> u16 {
    ZONE_KEY_BASE + zone_index as u16
}

/// Persisted zone state (5 bytes)
///
/// Only stores user-controlled values that should survive power cycles.
/// Current temperature and relay state are NOT persisted (restored by control loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PersistedZoneState {
    /// Heating setpoint in Matter centi-degrees (i16)
    pub heating_setpoint: i16,
    /// Cooling setpoint in Matter centi-degrees (i16)
    pub cooling_setpoint: i16,
    /// System mode: 0=Off, 1=Heat, 2=Cool
    pub system_mode: u8,
}

impl PersistedZoneState {
    /// Size of persisted data in bytes
    pub const SIZE: usize = 5;

    /// Create from ZoneThermostatState
    pub fn from_zone_state(state: &thermos_matter::state::ZoneThermostatState) -> Self {
        Self {
            heating_setpoint: state.get_occupied_heating_setpoint(),
            cooling_setpoint: state.get_occupied_cooling_setpoint(),
            system_mode: state.get_system_mode() as u8,
        }
    }

    /// Apply to ZoneThermostatState
    ///
    /// Silently ignores validation errors (uses defaults for invalid values).
    pub fn apply_to_zone_state(&self, state: &thermos_matter::state::ZoneThermostatState) {
        let _ = state.set_occupied_heating_setpoint(self.heating_setpoint);
        let _ = state.set_occupied_cooling_setpoint(self.cooling_setpoint);
        if let Some(mode) = SystemMode::from_u8(self.system_mode) {
            state.set_system_mode(mode);
        }
        // Clear dirty since we just loaded
        state.clear_dirty();
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..2].copy_from_slice(&self.heating_setpoint.to_le_bytes());
        buf[2..4].copy_from_slice(&self.cooling_setpoint.to_le_bytes());
        buf[4] = self.system_mode;
        buf
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            heating_setpoint: i16::from_le_bytes([data[0], data[1]]),
            cooling_setpoint: i16::from_le_bytes([data[2], data[3]]),
            system_mode: data[4],
        })
    }
}

impl Default for PersistedZoneState {
    fn default() -> Self {
        Self {
            heating_setpoint: thermos_matter::types::defaults::OCCUPIED_HEATING_SETPOINT,
            cooling_setpoint: thermos_matter::types::defaults::OCCUPIED_COOLING_SETPOINT,
            system_mode: SystemMode::Heat as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_key_calculation() {
        assert_eq!(zone_key(0), 0x1000);
        assert_eq!(zone_key(1), 0x1001);
        assert_eq!(zone_key(6), 0x1006);
        assert_eq!(zone_key(15), 0x100F);
    }

    #[test]
    fn test_persisted_zone_state_size() {
        assert_eq!(PersistedZoneState::SIZE, 5);
    }

    #[test]
    fn test_persisted_zone_state_serialization() {
        let state = PersistedZoneState {
            heating_setpoint: 2100, // 21°C
            cooling_setpoint: 2400, // 24°C
            system_mode: 1,         // Heat
        };

        let bytes = state.to_bytes();
        let restored = PersistedZoneState::from_bytes(&bytes).unwrap();

        assert_eq!(state, restored);
    }

    #[test]
    fn test_persisted_zone_state_from_bytes_too_short() {
        let short_data = [0u8; 4];
        assert!(PersistedZoneState::from_bytes(&short_data).is_none());
    }

    #[test]
    fn test_persisted_zone_state_default() {
        let state = PersistedZoneState::default();
        assert_eq!(state.heating_setpoint, 2000); // 20°C
        assert_eq!(state.cooling_setpoint, 2400); // 24°C
        assert_eq!(state.system_mode, 4); // Heat (0x04)
    }
}
