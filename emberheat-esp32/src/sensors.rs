//! Multi-sensor manager for shared 1-Wire bus
//!
//! This module provides infrastructure for managing multiple DS18B20 temperature
//! sensors on a single shared 1-Wire bus. It includes:
//!
//! - `SharedOneWireBus`: Thread-safe wrapper for the 1-Wire bus
//! - `Ds18b20Sensor`: Individual sensor wrapper that references the shared bus
//! - `MockSensor`: Testing sensor that returns configurable temperatures
//!
//! # Note on Hardware Support
//!
//! The actual 1-Wire bus operations require `esp-hal-rmt-onewire` which has
//! compatibility issues with esp-hal 1.0. The structure and API are ready;
//! hardware implementations will be enabled when the dependency is updated.
//!
//! # Example
//!
//! ```no_run
//! use emberheat_esp32::sensors::{SharedOneWireBus, Ds18b20Sensor, MockSensor};
//!
//! // For testing without hardware
//! let mock = MockSensor::new(21.5, "living-room");
//! let temp = mock.read_temperature().await?;
//! ```

extern crate alloc;

use core::fmt::Write as FmtWrite;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use emberheat_core::{SensorError, TemperatureReading, TemperatureSensor};
use heapless::String as HeaplessString;
use heapless::Vec as HeaplessVec;

/// DS18B20 family code (first byte of 64-bit ROM address)
pub const DS18B20_FAMILY_CODE: u8 = 0x28;

/// Temperature conversion time for 12-bit resolution (max 750ms)
pub const CONVERSION_TIME_MS: u64 = 750;

/// Maximum number of sensors that can be discovered on a single bus
pub const MAX_SENSORS_PER_BUS: usize = 8;

/// Placeholder for the 1-Wire bus inner type
///
/// This will be replaced with the actual esp-hal-rmt-onewire::OneWire type
/// when the dependency compatibility is resolved.
struct OneWireBusPlaceholder {
    _private: (),
}

/// Shared 1-Wire bus manager
///
/// Wraps a 1-Wire bus in a mutex for safe concurrent access from multiple
/// sensor instances. Each zone's sensor can reference this shared bus to
/// read its specific temperature sensor by address.
///
/// # Thread Safety
///
/// Uses `embassy_sync::mutex::Mutex` with `CriticalSectionRawMutex` for
/// interrupt-safe access on ESP32.
///
/// # Note
///
/// The actual 1-Wire bus type depends on esp-hal-rmt-onewire compatibility.
/// This implementation provides the API structure; hardware operations will
/// be implemented when the dependency is properly configured.
pub struct SharedOneWireBus {
    /// The underlying 1-Wire bus wrapped in a mutex
    /// Using Option to allow placeholder implementation when hardware unavailable
    bus: Mutex<CriticalSectionRawMutex, Option<OneWireBusPlaceholder>>,
}

impl SharedOneWireBus {
    /// Create an uninitialized shared bus
    ///
    /// The bus will not be functional for hardware operations.
    /// Use this for testing or when hardware initialization is deferred.
    ///
    /// # Example
    ///
    /// ```
    /// use emberheat_esp32::sensors::SharedOneWireBus;
    ///
    /// let bus = SharedOneWireBus::new_placeholder();
    /// // Bus is not functional for hardware reads
    /// ```
    pub fn new_placeholder() -> Self {
        Self {
            bus: Mutex::new(None),
        }
    }

    /// Scan the bus for all DS18B20 sensors
    ///
    /// Returns a list of 8-byte ROM addresses for each DS18B20 found on the bus.
    /// The addresses can be used to create individual `Ds18b20Sensor` instances.
    ///
    /// # Returns
    /// Vector of sensor addresses (up to MAX_SENSORS_PER_BUS)
    ///
    /// # Note
    /// Currently returns an empty list as hardware support is pending.
    pub async fn scan(&self) -> Result<HeaplessVec<[u8; 8], MAX_SENSORS_PER_BUS>, SensorError> {
        let _bus = self.bus.lock().await;
        let sensors = HeaplessVec::new();

        // TODO: Implement when esp-hal-rmt-onewire compatibility is resolved
        // Hardware implementation will:
        // 1. Call onewire.search() to discover devices
        // 2. Filter by DS18B20_FAMILY_CODE (0x28)
        // 3. Return list of addresses

        log::warn!("SharedOneWireBus::scan() - hardware support pending");
        Ok(sensors)
    }

    /// Read temperature from a specific sensor address
    ///
    /// This method handles the full read sequence:
    /// 1. Reset bus and select device by address
    /// 2. Issue temperature conversion command
    /// 3. Wait for conversion (750ms for 12-bit resolution)
    /// 4. Read and parse scratchpad data
    /// 5. Validate CRC and return temperature
    ///
    /// # Arguments
    /// * `address` - 8-byte ROM address of the sensor
    ///
    /// # Returns
    /// Temperature in Celsius or error
    ///
    /// # Note
    /// Currently returns an error as hardware support is pending.
    pub async fn read_temperature(&self, address: [u8; 8]) -> Result<f32, SensorError> {
        let _bus = self.bus.lock().await;

        // TODO: Implement when esp-hal-rmt-onewire compatibility is resolved
        // Hardware implementation will:
        // 1. Reset bus, match ROM address
        // 2. Send CONVERT_T command (0x44)
        // 3. Wait CONVERSION_TIME_MS (750ms)
        // 4. Reset bus, match ROM, send READ_SCRATCHPAD (0xBE)
        // 5. Read 9 bytes, verify CRC
        // 6. Parse temperature from bytes 0-1

        let _ = address; // Suppress unused warning
        let mut msg = HeaplessString::<128>::new();
        let _ = write!(
            &mut msg,
            "Hardware support pending - use MockSensor for testing"
        );
        Err(SensorError::Unavailable(msg))
    }
}

/// DS18B20 sensor wrapper for shared bus access
///
/// Represents a single DS18B20 temperature sensor on a shared 1-Wire bus.
/// Each sensor is identified by its unique 8-byte ROM address and has an
/// associated zone name for identification.
///
/// This differs from `sensor_onewire::Ds18b20Sensor` which owns its own bus.
/// Use `SharedDs18b20Sensor` when you have multiple sensors on a single bus.
///
/// # Usage
///
/// ```no_run
/// use emberheat_esp32::sensors::{SharedOneWireBus, SharedDs18b20Sensor};
///
/// // First, create a shared bus (hardware initialization pending)
/// let bus = SharedOneWireBus::new_placeholder();
///
/// // Create a sensor with known address
/// let address = [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
/// let sensor = SharedDs18b20Sensor::new(&bus, address, "zone1").unwrap();
/// ```
pub struct SharedDs18b20Sensor<'a> {
    /// Reference to the shared 1-Wire bus
    bus: &'a SharedOneWireBus,
    /// 8-byte ROM address of this sensor
    address: [u8; 8],
    /// Human-readable zone/sensor identifier
    zone_name: HeaplessString<32>,
}

impl<'a> SharedDs18b20Sensor<'a> {
    /// Create a new DS18B20 sensor wrapper
    ///
    /// # Arguments
    /// * `bus` - Reference to the shared 1-Wire bus
    /// * `address` - 8-byte ROM address of the sensor
    /// * `zone_name` - Human-readable identifier for this sensor/zone
    ///
    /// # Returns
    /// Result containing the sensor or error if zone name is too long
    pub fn new(
        bus: &'a SharedOneWireBus,
        address: [u8; 8],
        zone_name: &str,
    ) -> Result<Self, SensorError> {
        let mut name = HeaplessString::new();
        write!(&mut name, "{}", zone_name).map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Zone name too long (max 32 chars)");
            SensorError::InvalidData(msg)
        })?;

        Ok(Self {
            bus,
            address,
            zone_name: name,
        })
    }

    /// Get the 8-byte ROM address of this sensor
    pub fn address(&self) -> [u8; 8] {
        self.address
    }

    /// Get the ROM address as a hex string for display/logging
    pub fn address_hex(&self) -> HeaplessString<24> {
        let mut hex = HeaplessString::new();
        for (i, byte) in self.address.iter().enumerate() {
            if i > 0 {
                let _ = write!(&mut hex, "-");
            }
            let _ = write!(&mut hex, "{:02X}", byte);
        }
        hex
    }
}

impl<'a> TemperatureSensor for SharedDs18b20Sensor<'a> {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        let temp_c = self.bus.read_temperature(self.address).await?;

        // Validate temperature is in reasonable range
        if temp_c < emberheat_core::MIN_VALID_TEMP_C || temp_c > emberheat_core::MAX_VALID_TEMP_C {
            return Err(SensorError::OutOfRange(temp_c));
        }

        log::debug!(
            "Sensor {} ({}) read: {:.2}C",
            self.zone_name,
            self.address_hex(),
            temp_c
        );

        Ok(TemperatureReading::from_celsius(temp_c))
    }

    fn id(&self) -> &str {
        self.zone_name.as_str()
    }
}

/// Mock sensor for testing without hardware
///
/// Returns a configurable fixed temperature value. Useful for:
/// - Unit testing zone logic
/// - Integration testing without physical sensors
/// - Development on non-ESP32 platforms
///
/// # Example
///
/// ```
/// use emberheat_esp32::sensors::MockSensor;
/// use emberheat_core::TemperatureSensor;
///
/// let mut sensor = MockSensor::new(21.5, "test-zone");
/// // Read returns the configured temperature
/// // sensor.set_temperature(25.0) to change it
/// ```
pub struct MockSensor {
    /// Temperature to return on read
    temperature_c: f32,
    /// Zone/sensor identifier
    zone_name: HeaplessString<32>,
}

impl MockSensor {
    /// Create a new mock sensor
    ///
    /// # Arguments
    /// * `temperature_c` - Initial temperature in Celsius to return
    /// * `zone_name` - Human-readable identifier for this mock sensor
    ///
    /// # Returns
    /// The mock sensor instance (infallible, truncates long names)
    pub fn new(temperature_c: f32, zone_name: &str) -> Self {
        let mut name = HeaplessString::new();
        // Truncate if necessary rather than failing
        for c in zone_name.chars().take(32) {
            if name.push(c).is_err() {
                break;
            }
        }

        Self {
            temperature_c,
            zone_name: name,
        }
    }

    /// Update the temperature that will be returned by read_temperature
    ///
    /// # Arguments
    /// * `temp` - New temperature in Celsius
    pub fn set_temperature(&mut self, temp: f32) {
        self.temperature_c = temp;
    }

    /// Get the current configured temperature
    pub fn temperature(&self) -> f32 {
        self.temperature_c
    }
}

impl TemperatureSensor for MockSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        log::trace!(
            "MockSensor {} returning {:.2}C",
            self.zone_name,
            self.temperature_c
        );
        Ok(TemperatureReading::from_celsius(self.temperature_c))
    }

    fn id(&self) -> &str {
        self.zone_name.as_str()
    }
}

// ============================================================================
// Helper Functions (for future hardware implementation)
// ============================================================================

/// Parse temperature from DS18B20 scratchpad data
///
/// DS18B20 stores temperature as a 16-bit signed value in 1/16C increments.
/// Bytes 0-1 of scratchpad contain temp_lsb and temp_msb.
///
/// # Arguments
/// * `scratchpad` - 9-byte scratchpad data from sensor
///
/// # Returns
/// Temperature in Celsius
pub fn parse_temperature(scratchpad: &[u8; 9]) -> f32 {
    let temp_lsb = scratchpad[0];
    let temp_msb = scratchpad[1];

    // Combine into 16-bit signed integer
    let raw = i16::from_le_bytes([temp_lsb, temp_msb]);

    // Convert to Celsius (1/16C resolution = 0.0625C)
    raw as f32 / 16.0
}

/// Verify CRC-8 of DS18B20 scratchpad data
///
/// Uses Dallas/Maxim 1-Wire CRC-8 algorithm (polynomial 0x8C).
///
/// # Arguments
/// * `data` - 9-byte scratchpad data (bytes 0-7 are data, byte 8 is CRC)
///
/// # Returns
/// true if CRC is valid
pub fn verify_crc(data: &[u8; 9]) -> bool {
    let mut crc = 0u8;

    for i in 0..8 {
        let mut byte = data[i];
        for _ in 0..8 {
            let mix = (crc ^ byte) & 0x01;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
            byte >>= 1;
        }
    }

    crc == data[8]
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_sensor_creation() {
        let sensor = MockSensor::new(21.5, "living-room");
        assert_eq!(sensor.id(), "living-room");
        assert_eq!(sensor.temperature(), 21.5);
    }

    #[test]
    fn test_mock_sensor_long_name_truncated() {
        let long_name = "this-is-a-very-long-zone-name-that-exceeds-thirty-two-characters";
        let sensor = MockSensor::new(20.0, long_name);
        // Should be truncated to 32 chars, not error
        assert!(sensor.id().len() <= 32);
    }

    #[test]
    fn test_mock_sensor_set_temperature() {
        let mut sensor = MockSensor::new(20.0, "test");
        assert_eq!(sensor.temperature(), 20.0);

        sensor.set_temperature(25.5);
        assert_eq!(sensor.temperature(), 25.5);
    }

    #[tokio::test]
    async fn test_mock_sensor_read() {
        let sensor = MockSensor::new(21.5, "test");
        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 21.5);
    }

    #[tokio::test]
    async fn test_mock_sensor_temperature_update() {
        let mut sensor = MockSensor::new(20.0, "test");

        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 20.0);

        sensor.set_temperature(25.0);
        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 25.0);
    }

    #[test]
    fn test_parse_temperature_positive() {
        // 25.0625C = 401 raw = 0x0191
        let scratchpad = [0x91, 0x01, 0, 0, 0, 0, 0, 0, 0];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - 25.0625).abs() < 0.001);
    }

    #[test]
    fn test_parse_temperature_negative() {
        // -10.125C = -162 raw = 0xFF5E
        let scratchpad = [0x5E, 0xFF, 0, 0, 0, 0, 0, 0, 0];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - (-10.125)).abs() < 0.001);
    }

    #[test]
    fn test_parse_temperature_zero() {
        let scratchpad = [0x00, 0x00, 0, 0, 0, 0, 0, 0, 0];
        let temp = parse_temperature(&scratchpad);
        assert_eq!(temp, 0.0);
    }

    #[test]
    fn test_verify_crc_valid() {
        // Real DS18B20 scratchpad with valid CRC
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        assert!(verify_crc(&scratchpad));
    }

    #[test]
    fn test_verify_crc_invalid() {
        // Same data but wrong CRC
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0xFF];
        assert!(!verify_crc(&scratchpad));
    }

    #[test]
    fn test_ds18b20_family_code() {
        assert_eq!(DS18B20_FAMILY_CODE, 0x28);
    }

    #[test]
    fn test_conversion_time() {
        // 12-bit resolution requires 750ms max
        assert_eq!(CONVERSION_TIME_MS, 750);
    }

    #[test]
    fn test_shared_ds18b20_sensor_creation() {
        let bus = SharedOneWireBus::new_placeholder();
        let address = [0x28, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        let sensor = SharedDs18b20Sensor::new(&bus, address, "zone1").unwrap();

        assert_eq!(sensor.id(), "zone1");
        assert_eq!(sensor.address(), address);
    }

    #[test]
    fn test_shared_ds18b20_sensor_address_hex() {
        let bus = SharedOneWireBus::new_placeholder();
        let address = [0x28, 0xAB, 0xCD, 0xEF, 0x12, 0x34, 0x56, 0x78];
        let sensor = SharedDs18b20Sensor::new(&bus, address, "test").unwrap();

        let hex = sensor.address_hex();
        assert_eq!(hex.as_str(), "28-AB-CD-EF-12-34-56-78");
    }

    #[test]
    fn test_shared_ds18b20_sensor_long_name_error() {
        let bus = SharedOneWireBus::new_placeholder();
        let address = [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let long_name = "this-is-a-very-long-zone-name-that-exceeds-thirty-two-characters";

        let result = SharedDs18b20Sensor::new(&bus, address, long_name);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shared_bus_scan_returns_empty() {
        let bus = SharedOneWireBus::new_placeholder();
        let sensors = bus.scan().await.unwrap();
        assert!(sensors.is_empty());
    }

    #[tokio::test]
    async fn test_shared_bus_read_returns_unavailable() {
        let bus = SharedOneWireBus::new_placeholder();
        let address = [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let result = bus.read_temperature(address).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_temperature_range() {
        // Test minimum temperature (-55C = 0xFC90)
        let scratchpad = [0x90, 0xFC, 0, 0, 0, 0, 0, 0, 0];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - (-55.0)).abs() < 0.1);

        // Test maximum temperature (125C = 0x07D0)
        let scratchpad = [0xD0, 0x07, 0, 0, 0, 0, 0, 0, 0];
        let temp = parse_temperature(&scratchpad);
        assert!((temp - 125.0).abs() < 0.1);
    }
}
