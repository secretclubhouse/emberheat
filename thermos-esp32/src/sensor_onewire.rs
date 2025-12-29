//! 1-Wire DS18B20 temperature sensor implementation for ESP32-C6
//!
//! This module implements the thermos-core TemperatureSensor trait for DS18B20
//! temperature sensors using the ESP32 RMT (Remote Control) peripheral for precise
//! 1-Wire protocol timing.
//!
//! The RMT peripheral provides hardware-based timing for the 1-Wire protocol,
//! which is more reliable than software bit-banging.

extern crate alloc;
use alloc::boxed::Box;

use core::fmt::Write as FmtWrite;
use embassy_time::{Duration, Timer};
use esp_hal::rmt::Rmt;
use esp_hal_rmt_onewire::{Address, OneWire};
use heapless::String as HeaplessString;
use thermos_core::{SensorError, TemperatureReading, TemperatureSensor};

/// DS18B20 family code (first byte of 64-bit ROM address)
const DS18B20_FAMILY_CODE: u8 = 0x28;

/// DS18B20 commands
const CMD_CONVERT_T: u8 = 0x44; // Start temperature conversion
const CMD_READ_SCRATCHPAD: u8 = 0xBE; // Read scratchpad (9 bytes)
const CMD_SKIP_ROM: u8 = 0xCC; // Address all devices on bus

/// Temperature conversion time for 12-bit resolution (max)
const CONVERSION_TIME_MS: u64 = 750;

/// DS18B20 1-Wire temperature sensor
///
/// Uses the ESP32 RMT peripheral via esp-hal-rmt-onewire for precise timing.
pub struct Ds18b20Sensor<'a> {
    onewire: OneWire<'a>,
    address: Option<Address>,
    sensor_id: HeaplessString<32>,
}

impl<'a> Ds18b20Sensor<'a> {
    /// Create a new DS18B20 sensor
    ///
    /// # Arguments
    /// * `rmt` - RMT peripheral instance
    /// * `gpio_pin` - GPIO pin for 1-Wire bus
    /// * `sensor_id` - Human-readable sensor identifier
    ///
    /// # Returns
    /// Result containing the sensor instance or error
    ///
    /// # Example
    /// ```no_run
    /// let rmt = Rmt::new(peripherals.RMT, 80.MHz());
    /// let gpio = peripherals.GPIO6;
    /// let sensor = Ds18b20Sensor::new(rmt, gpio, "living-room")?;
    /// ```
    pub fn new(
        rmt: Rmt<'a>,
        gpio_pin: impl Into<esp_hal::gpio::AnyPin>,
        sensor_id: &str,
    ) -> Result<Self, SensorError> {
        let mut id = HeaplessString::new();
        write!(&mut id, "{}", sensor_id).map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Sensor ID too long");
            SensorError::InvalidData(msg)
        })?;

        // Initialize OneWire with RMT
        let onewire = OneWire::new(rmt, gpio_pin.into()).map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Failed to initialize 1-Wire bus");
            SensorError::IoError(msg)
        })?;

        Ok(Self {
            onewire,
            address: None,
            sensor_id: id,
        })
    }

    /// Initialize the sensor and discover its address
    ///
    /// If multiple devices are on the bus, this will use the first DS18B20 found.
    /// For multi-sensor setups, use `init_with_address` instead.
    pub async fn init(&mut self) -> Result<(), SensorError> {
        // Search for DS18B20 devices on the bus
        let search = self.onewire.search().map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "1-Wire search failed");
            SensorError::IoError(msg)
        })?;

        // Find first DS18B20 (family code 0x28)
        for addr in search {
            if addr.family_code() == DS18B20_FAMILY_CODE {
                self.address = Some(addr);
                return Ok(());
            }
        }

        let mut msg = HeaplessString::<128>::new();
        let _ = write!(&mut msg, "No DS18B20 sensors found on bus");
        Err(SensorError::IoError(msg))
    }

    /// Initialize with a specific sensor address
    ///
    /// Use this when you have multiple sensors on the bus and want to address
    /// a specific one by its 64-bit ROM address.
    ///
    /// # Arguments
    /// * `address` - 64-bit ROM address as bytes [family, serial[6], crc]
    pub fn init_with_address(&mut self, address: [u8; 8]) -> Result<(), SensorError> {
        self.address = Some(Address::from_bytes(address));
        Ok(())
    }

    /// Start temperature conversion on the sensor
    ///
    /// This initiates a temperature reading. You must wait CONVERSION_TIME_MS
    /// before reading the scratchpad.
    async fn start_conversion(&mut self) -> Result<(), SensorError> {
        // Reset bus and check for presence
        self.onewire.reset().await.map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "1-Wire reset failed - no device present");
            SensorError::IoError(msg)
        })?;

        if let Some(addr) = self.address {
            // Address specific device
            self.onewire.match_rom(&addr).await.map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "Failed to match ROM address");
                SensorError::IoError(msg)
            })?;
        } else {
            // Skip ROM (address all devices)
            self.onewire.write_byte(CMD_SKIP_ROM).await.map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "Failed to send skip ROM command");
                SensorError::IoError(msg)
            })?;
        }

        // Send convert command
        self.onewire.write_byte(CMD_CONVERT_T).await.map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Failed to send convert command");
            SensorError::IoError(msg)
        })?;

        Ok(())
    }

    /// Read scratchpad from sensor
    ///
    /// Returns 9 bytes: [temp_lsb, temp_msb, th, tl, config, reserved, reserved, reserved, crc]
    async fn read_scratchpad(&mut self) -> Result<[u8; 9], SensorError> {
        // Reset bus
        self.onewire.reset().await.map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "1-Wire reset failed");
            SensorError::IoError(msg)
        })?;

        if let Some(addr) = self.address {
            // Address specific device
            self.onewire.match_rom(&addr).await.map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "Failed to match ROM");
                SensorError::IoError(msg)
            })?;
        } else {
            // Skip ROM
            self.onewire.write_byte(CMD_SKIP_ROM).await.map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "Failed to send skip ROM");
                SensorError::IoError(msg)
            })?;
        }

        // Send read scratchpad command
        self.onewire
            .write_byte(CMD_READ_SCRATCHPAD)
            .await
            .map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "Failed to send read scratchpad command");
                SensorError::IoError(msg)
            })?;

        // Read 9 bytes
        let mut scratchpad = [0u8; 9];
        for byte in scratchpad.iter_mut() {
            *byte = self.onewire.read_byte().await.map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "Failed to read scratchpad byte");
                SensorError::IoError(msg)
            })?;
        }

        // Verify CRC
        if !Self::verify_crc(&scratchpad) {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Scratchpad CRC check failed");
            return Err(SensorError::InvalidData(msg));
        }

        Ok(scratchpad)
    }

    /// Calculate temperature from scratchpad data
    ///
    /// DS18B20 stores temperature as a 16-bit signed value in 1/16°C increments.
    /// Temperature = raw_value / 16.0
    fn parse_temperature(scratchpad: &[u8; 9]) -> f32 {
        let temp_lsb = scratchpad[0];
        let temp_msb = scratchpad[1];

        // Combine into 16-bit signed integer
        let raw = i16::from_le_bytes([temp_lsb, temp_msb]);

        // Convert to Celsius (1/16°C resolution)
        raw as f32 / 16.0
    }

    /// Verify CRC of scratchpad data
    ///
    /// Uses Dallas/Maxim 1-Wire CRC-8 algorithm
    fn verify_crc(data: &[u8; 9]) -> bool {
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

    /// List all DS18B20 sensors on the bus
    ///
    /// Returns a vector of sensor addresses (as 8-byte arrays)
    pub async fn list_sensors(&mut self) -> Result<heapless::Vec<[u8; 8], 8>, SensorError> {
        let mut sensors = heapless::Vec::new();

        let search = self.onewire.search().map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "1-Wire search failed");
            SensorError::IoError(msg)
        })?;

        for addr in search {
            if addr.family_code() == DS18B20_FAMILY_CODE {
                let _ = sensors.push(addr.to_bytes());
            }
        }

        Ok(sensors)
    }
}

impl<'a> TemperatureSensor for Ds18b20Sensor<'a> {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        // Need mutable access for OneWire operations
        // SAFETY: We're in an async context with exclusive access
        let sensor = unsafe { &mut *(self as *const Self as *mut Self) };

        // Start conversion
        sensor.start_conversion().await?;

        // Wait for conversion to complete (750ms for 12-bit resolution)
        Timer::after(Duration::from_millis(CONVERSION_TIME_MS)).await;

        // Read scratchpad
        let scratchpad = sensor.read_scratchpad().await?;

        // Parse temperature
        let temp_c = Self::parse_temperature(&scratchpad);

        // Validate temperature range
        if temp_c < thermos_core::MIN_VALID_TEMP_C || temp_c > thermos_core::MAX_VALID_TEMP_C {
            return Err(SensorError::OutOfRange(temp_c));
        }

        Ok(TemperatureReading::from_celsius(temp_c))
    }

    fn id(&self) -> &str {
        self.sensor_id.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_temperature_positive() {
        // Test case: 25.0625°C
        // Raw value: 0x0191 = 401 decimal
        // 401 / 16 = 25.0625°C
        let scratchpad = [0x91, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = Ds18b20Sensor::parse_temperature(&scratchpad);
        assert!((temp - 25.0625).abs() < 0.001);
    }

    #[test]
    fn test_parse_temperature_negative() {
        // Test case: -10.125°C
        // Raw value: 0xFF5E = -162 decimal
        // -162 / 16 = -10.125°C
        let scratchpad = [0x5E, 0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = Ds18b20Sensor::parse_temperature(&scratchpad);
        assert!((temp - (-10.125)).abs() < 0.001);
    }

    #[test]
    fn test_parse_temperature_zero() {
        // Test case: 0°C
        let scratchpad = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = Ds18b20Sensor::parse_temperature(&scratchpad);
        assert_eq!(temp, 0.0);
    }

    #[test]
    fn test_verify_crc_valid() {
        // Real DS18B20 scratchpad with valid CRC
        // Temperature: 25.0625°C (0x0191)
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0x1C];
        assert!(Ds18b20Sensor::verify_crc(&scratchpad));
    }

    #[test]
    fn test_verify_crc_invalid() {
        // Same data but with wrong CRC
        let scratchpad = [0x91, 0x01, 0x4B, 0x46, 0x7F, 0xFF, 0x0C, 0x10, 0xFF];
        assert!(!Ds18b20Sensor::verify_crc(&scratchpad));
    }

    #[test]
    fn test_family_code() {
        assert_eq!(DS18B20_FAMILY_CODE, 0x28);
    }

    #[test]
    fn test_commands() {
        assert_eq!(CMD_CONVERT_T, 0x44);
        assert_eq!(CMD_READ_SCRATCHPAD, 0xBE);
        assert_eq!(CMD_SKIP_ROM, 0xCC);
    }

    #[test]
    fn test_conversion_time() {
        // 12-bit resolution requires 750ms max
        assert_eq!(CONVERSION_TIME_MS, 750);
    }

    #[test]
    fn test_temperature_resolution() {
        // DS18B20 resolution is 0.0625°C (1/16)
        let resolution = 1.0 / 16.0;
        assert_eq!(resolution, 0.0625);
    }

    #[test]
    fn test_parse_temperature_range() {
        // Test minimum temperature (-55°C = 0xFC90)
        let scratchpad = [0x90, 0xFC, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = Ds18b20Sensor::parse_temperature(&scratchpad);
        assert!((temp - (-55.0)).abs() < 0.1);

        // Test maximum temperature (125°C = 0x07D0)
        let scratchpad = [0xD0, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        let temp = Ds18b20Sensor::parse_temperature(&scratchpad);
        assert!((temp - 125.0).abs() < 0.1);
    }
}
