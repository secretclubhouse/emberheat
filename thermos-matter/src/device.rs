//! Matter device configuration for multi-zone thermostat
//!
//! Provides endpoint constants and helper functions for zone/endpoint mapping.
//! The actual zone states are owned by the application (thermos-esp32).

/// Matter endpoint number for root node (device info, descriptors)
pub const ROOT_ENDPOINT: u16 = 0;

/// First zone endpoint (zones are 1-indexed in Matter)
pub const FIRST_ZONE_ENDPOINT: u16 = 1;

/// Maximum supported zones
///
/// This is a soft limit for validation. The actual limit depends on:
/// - Available GPIO pins on your ESP32 variant (~15-20 usable on ESP32-C6)
/// - Task functions defined in main.rs (must add zoneN_task for each zone)
/// - Available memory (~1-2KB per zone, not typically a constraint)
pub const MAX_ZONES: usize = 16;

/// Convert zone index to Matter endpoint number
///
/// # Arguments
///
/// * `index` - Zone index (0-based)
/// * `num_zones` - Total number of configured zones
///
/// # Returns
///
/// Matter endpoint number (1-based), or `None` if index is out of range.
pub fn zone_index_to_endpoint(index: usize, num_zones: usize) -> Option<u16> {
    if index < num_zones && num_zones <= MAX_ZONES {
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
/// * `num_zones` - Total number of configured zones
///
/// # Returns
///
/// Zone index (0-based), or `None` if endpoint is not a valid zone.
pub fn endpoint_to_zone_index(endpoint: u16, num_zones: usize) -> Option<usize> {
    let index = endpoint.checked_sub(FIRST_ZONE_ENDPOINT)? as usize;
    if index < num_zones && num_zones <= MAX_ZONES {
        Some(index)
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
    /// Number of zones
    pub num_zones: usize,
}

impl DeviceInfo {
    /// Create device info with specified zone count
    ///
    /// # Panics
    ///
    /// Debug builds will panic if `num_zones` is 0 or greater than `MAX_ZONES`.
    pub fn new(num_zones: usize) -> Self {
        debug_assert!(
            num_zones > 0 && num_zones <= MAX_ZONES,
            "num_zones must be 1-{MAX_ZONES}"
        );
        Self {
            vendor_id: 0xFFF1,  // Test vendor ID (for development)
            product_id: 0x8001, // Test product ID
            device_name: "Thermos Multi-Zone",
            serial_number: "THERMOS-001",
            num_zones,
        }
    }

    /// Create device info with custom serial number
    pub fn with_serial(num_zones: usize, serial: &'static str) -> Self {
        Self {
            serial_number: serial,
            ..Self::new(num_zones)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_endpoint_conversions_3_zones() {
        let num_zones = 3;

        // Valid conversions
        assert_eq!(zone_index_to_endpoint(0, num_zones), Some(1));
        assert_eq!(zone_index_to_endpoint(2, num_zones), Some(3));
        assert_eq!(zone_index_to_endpoint(3, num_zones), None); // Out of range

        assert_eq!(endpoint_to_zone_index(1, num_zones), Some(0));
        assert_eq!(endpoint_to_zone_index(3, num_zones), Some(2));
        assert_eq!(endpoint_to_zone_index(4, num_zones), None); // Out of range
        assert_eq!(endpoint_to_zone_index(0, num_zones), None); // Root endpoint
    }

    #[test]
    fn test_endpoint_conversions_7_zones() {
        let num_zones = 7;

        assert_eq!(zone_index_to_endpoint(0, num_zones), Some(1));
        assert_eq!(zone_index_to_endpoint(6, num_zones), Some(7));
        assert_eq!(zone_index_to_endpoint(7, num_zones), None);

        assert_eq!(endpoint_to_zone_index(1, num_zones), Some(0));
        assert_eq!(endpoint_to_zone_index(7, num_zones), Some(6));
        assert_eq!(endpoint_to_zone_index(8, num_zones), None);
    }

    #[test]
    fn test_max_zones_limit() {
        // Can't exceed MAX_ZONES (16)
        assert_eq!(zone_index_to_endpoint(0, 17), None);
        assert_eq!(endpoint_to_zone_index(1, 17), None);
        // 16 zones should work
        assert_eq!(zone_index_to_endpoint(0, 16), Some(1));
        assert_eq!(zone_index_to_endpoint(15, 16), Some(16));
        assert_eq!(endpoint_to_zone_index(1, 16), Some(0));
        assert_eq!(endpoint_to_zone_index(16, 16), Some(15));
    }

    #[test]
    fn test_device_info() {
        let info = DeviceInfo::new(3);
        assert_eq!(info.num_zones, 3);
        assert_eq!(info.vendor_id, 0xFFF1);
    }

    #[test]
    fn test_device_info_with_serial() {
        let info = DeviceInfo::with_serial(4, "CUSTOM-SERIAL-123");
        assert_eq!(info.num_zones, 4);
        assert_eq!(info.serial_number, "CUSTOM-SERIAL-123");
        // Should inherit defaults from new()
        assert_eq!(info.vendor_id, 0xFFF1);
        assert_eq!(info.product_id, 0x8001);
        assert_eq!(info.device_name, "Thermos Multi-Zone");
    }
}
