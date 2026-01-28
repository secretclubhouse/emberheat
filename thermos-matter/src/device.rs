//! Matter device configuration for 7-zone thermostat
//!
//! Defines the Matter node with 7 thermostat endpoints (one per zone).
//! Endpoint 0 is reserved for the root node (device descriptors), and
//! endpoints 1-7 are individual heating zones.
//!
//! # Matter Endpoint Structure
//!
//! ```text
//! Endpoint 0: Root Node
//!   - Basic Information Cluster
//!   - Descriptor Cluster
//!
//! Endpoints 1-7: Thermostat Zones
//!   - Thermostat Cluster (0x0201)
//!   - Descriptor Cluster
//! ```

use crate::state::ZoneThermostatState;

/// Matter endpoint number for root node (device info, descriptors)
pub const ROOT_ENDPOINT: u16 = 0;

/// Matter endpoint number for zone 1
pub const ZONE1_ENDPOINT: u16 = 1;

/// Matter endpoint number for zone 2
pub const ZONE2_ENDPOINT: u16 = 2;

/// Matter endpoint number for zone 3
pub const ZONE3_ENDPOINT: u16 = 3;

/// Matter endpoint number for zone 4
pub const ZONE4_ENDPOINT: u16 = 4;

/// Matter endpoint number for zone 5
pub const ZONE5_ENDPOINT: u16 = 5;

/// Matter endpoint number for zone 6
pub const ZONE6_ENDPOINT: u16 = 6;

/// Matter endpoint number for zone 7
pub const ZONE7_ENDPOINT: u16 = 7;

/// Number of thermostat zones
pub const NUM_ZONES: usize = 7;

/// Static state for all 7 zones
///
/// These are initialized at compile time with default values.
/// The control loop and Matter handlers access these via atomic operations.
pub static ZONE_STATES: [ZoneThermostatState; NUM_ZONES] = [
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
    ZoneThermostatState::new(),
];

/// Get zone state by index (0-6)
///
/// # Arguments
///
/// * `index` - Zone index (0 = Zone 1, 6 = Zone 7)
///
/// # Returns
///
/// Reference to the zone state, or `None` if index is out of range.
pub fn get_zone_state(index: usize) -> Option<&'static ZoneThermostatState> {
    ZONE_STATES.get(index)
}

/// Get zone state by Matter endpoint number (1-7)
///
/// # Arguments
///
/// * `endpoint` - Matter endpoint number
///
/// # Returns
///
/// Reference to the zone state, or `None` if endpoint is not a zone.
pub fn get_zone_state_by_endpoint(endpoint: u16) -> Option<&'static ZoneThermostatState> {
    if endpoint >= ZONE1_ENDPOINT && endpoint <= ZONE7_ENDPOINT {
        get_zone_state((endpoint - 1) as usize)
    } else {
        None
    }
}

/// Convert zone index to Matter endpoint number
///
/// # Arguments
///
/// * `index` - Zone index (0-6)
///
/// # Returns
///
/// Matter endpoint number (1-7), or `None` if index is out of range.
pub fn zone_index_to_endpoint(index: usize) -> Option<u16> {
    if index < NUM_ZONES {
        Some((index + 1) as u16)
    } else {
        None
    }
}

/// Convert Matter endpoint to zone index
///
/// # Arguments
///
/// * `endpoint` - Matter endpoint number
///
/// # Returns
///
/// Zone index (0-6), or `None` if endpoint is not a zone.
pub fn endpoint_to_zone_index(endpoint: u16) -> Option<usize> {
    if endpoint >= ZONE1_ENDPOINT && endpoint <= ZONE7_ENDPOINT {
        Some((endpoint - 1) as usize)
    } else {
        None
    }
}

/// Device information for Matter commissioning
///
/// Contains vendor/product IDs and device metadata used during
/// Matter commissioning and discovery.
pub struct DeviceInfo {
    /// Matter vendor ID (0xFFF1 = test vendor)
    pub vendor_id: u16,
    /// Matter product ID
    pub product_id: u16,
    /// Human-readable device name
    pub device_name: &'static str,
    /// Device serial number
    pub serial_number: &'static str,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        Self {
            vendor_id: 0xFFF1,  // Test vendor ID (for development)
            product_id: 0x8001, // Test product ID
            device_name: "Thermos 7-Zone",
            serial_number: "THERMOS-001",
        }
    }
}

impl DeviceInfo {
    /// Create device info with custom serial number
    ///
    /// # Arguments
    ///
    /// * `serial` - Unique serial number for this device
    pub fn with_serial(serial: &'static str) -> Self {
        Self {
            serial_number: serial,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_state_access() {
        // Test by index
        assert!(get_zone_state(0).is_some());
        assert!(get_zone_state(6).is_some());
        assert!(get_zone_state(7).is_none());

        // Test by endpoint
        assert!(get_zone_state_by_endpoint(1).is_some());
        assert!(get_zone_state_by_endpoint(7).is_some());
        assert!(get_zone_state_by_endpoint(0).is_none()); // Root endpoint
        assert!(get_zone_state_by_endpoint(8).is_none());
    }

    #[test]
    fn test_endpoint_conversions() {
        // Index to endpoint
        assert_eq!(zone_index_to_endpoint(0), Some(1));
        assert_eq!(zone_index_to_endpoint(6), Some(7));
        assert_eq!(zone_index_to_endpoint(7), None);

        // Endpoint to index
        assert_eq!(endpoint_to_zone_index(1), Some(0));
        assert_eq!(endpoint_to_zone_index(7), Some(6));
        assert_eq!(endpoint_to_zone_index(0), None);
        assert_eq!(endpoint_to_zone_index(8), None);
    }

    #[test]
    fn test_device_info_defaults() {
        let info = DeviceInfo::default();
        assert_eq!(info.vendor_id, 0xFFF1);
        assert_eq!(info.product_id, 0x8001);
        assert_eq!(info.device_name, "Thermos 7-Zone");
    }

    #[test]
    fn test_device_info_with_serial() {
        let info = DeviceInfo::with_serial("CUSTOM-123");
        assert_eq!(info.serial_number, "CUSTOM-123");
        assert_eq!(info.vendor_id, 0xFFF1); // Other fields unchanged
    }
}
