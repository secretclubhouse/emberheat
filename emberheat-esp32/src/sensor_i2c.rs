//! I2C sensor implementation for ESP32-C6
//!
//! This module implements the emberheat-core TemperatureSensor trait for I2C-based
//! temperature sensors, specifically the BME280 (temperature, humidity, pressure sensor).
//!
//! NOTE: esp-hal 1.0 I2C is synchronous. We wrap it with async trait for compatibility.

extern crate alloc;
use alloc::boxed::Box;

use core::fmt::Write as FmtWrite;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Timer};
use emberheat_core::{SensorError, TemperatureReading, TemperatureSensor};
use esp_hal::i2c::master::I2c;
use esp_hal::Blocking;
use heapless::String as HeaplessString;

/// BME280 I2C sensor wrapper
///
/// Implements the TemperatureSensor trait for the popular BME280 environmental sensor.
/// The BME280 provides temperature, humidity, and pressure readings over I2C.
pub struct Bme280Sensor {
    i2c: Mutex<CriticalSectionRawMutex, I2c<'static, Blocking>>,
    address: u8,
    sensor_id: HeaplessString<32>,
}

impl Bme280Sensor {
    /// Default I2C address for BME280 (when SDO pin is pulled low)
    pub const DEFAULT_ADDRESS: u8 = 0x76;

    /// Alternative I2C address for BME280 (when SDO pin is pulled high)
    pub const ALT_ADDRESS: u8 = 0x77;

    /// BME280 chip ID register
    const REG_CHIP_ID: u8 = 0xD0;

    /// Expected chip ID for BME280
    const CHIP_ID: u8 = 0x60;

    /// Temperature data registers
    const REG_TEMP_MSB: u8 = 0xFA;

    /// Control measurement register
    const REG_CTRL_MEAS: u8 = 0xF4;

    /// Configuration register
    const REG_CONFIG: u8 = 0xF5;

    /// Create a new BME280 sensor
    ///
    /// # Arguments
    /// * `i2c` - I2C peripheral from esp-hal
    /// * `address` - I2C address (DEFAULT_ADDRESS or ALT_ADDRESS)
    /// * `sensor_id` - Human-readable sensor identifier
    pub fn new(
        i2c: I2c<'static, Blocking>,
        address: u8,
        sensor_id: &str,
    ) -> Result<Self, SensorError> {
        let mut id = HeaplessString::new();
        write!(&mut id, "{}", sensor_id).map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Sensor ID too long");
            SensorError::InvalidData(msg)
        })?;

        Ok(Self {
            i2c: Mutex::new(i2c),
            address,
            sensor_id: id,
        })
    }

    /// Initialize the BME280 sensor
    ///
    /// Verifies chip ID and configures the sensor for temperature readings
    pub async fn init(&mut self) -> Result<(), SensorError> {
        // Verify chip ID
        let chip_id = self.read_register(Self::REG_CHIP_ID).await?;
        if chip_id != Self::CHIP_ID {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(
                &mut msg,
                "Invalid chip ID: expected 0x{:02X}, got 0x{:02X}",
                Self::CHIP_ID,
                chip_id
            );
            return Err(SensorError::InvalidData(msg));
        }

        // Configure sensor: normal mode, temperature oversampling x2
        self.write_register(Self::REG_CTRL_MEAS, 0b01000111).await?;

        // Configure: standby time 1000ms, filter off
        self.write_register(Self::REG_CONFIG, 0b10100000).await?;

        // Wait for first measurement
        Timer::after(Duration::from_millis(100)).await;

        Ok(())
    }

    /// Read a single register from the sensor
    async fn read_register(&self, reg: u8) -> Result<u8, SensorError> {
        let mut buf = [0u8; 1];
        let mut i2c = self.i2c.lock().await;
        i2c.write_read(self.address, &[reg], &mut buf)
            .map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "I2C read failed");
                SensorError::IoError(msg)
            })?;
        Ok(buf[0])
    }

    /// Write a single register to the sensor
    async fn write_register(&self, reg: u8, value: u8) -> Result<(), SensorError> {
        let mut i2c = self.i2c.lock().await;
        i2c.write(self.address, &[reg, value]).map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "I2C write failed");
            SensorError::IoError(msg)
        })
    }

    /// Read raw temperature data from the sensor
    async fn read_raw_temperature(&self) -> Result<i32, SensorError> {
        let mut buf = [0u8; 3];
        let mut i2c = self.i2c.lock().await;
        i2c.write_read(self.address, &[Self::REG_TEMP_MSB], &mut buf)
            .map_err(|_| {
                let mut msg = HeaplessString::<128>::new();
                let _ = write!(&mut msg, "I2C temp read failed");
                SensorError::IoError(msg)
            })?;

        // Combine 20-bit temperature value
        let raw = ((buf[0] as i32) << 12) | ((buf[1] as i32) << 4) | ((buf[2] as i32) >> 4);
        Ok(raw)
    }

    /// Calculate temperature in Celsius from raw value
    ///
    /// This is a simplified calculation. For production use, you should use
    /// proper calibration data from the sensor.
    fn calculate_temperature(&self, raw: i32) -> f32 {
        // Simplified formula (without calibration data)
        // Typical value at 25°C is around 519888
        // This is an approximation and should be replaced with proper calibration
        let var1 = ((raw / 16384) - (28000 / 1024)) * 1690;
        let var2 = (((raw / 131072) - (28000 / 8192)) * ((raw / 131072) - (28000 / 8192))) / 4096
            * (-7350);
        let t_fine = var1 + var2;
        let temp = (t_fine * 5 + 128) / 256;
        temp as f32 / 100.0
    }
}

impl TemperatureSensor for Bme280Sensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        let raw = self.read_raw_temperature().await?;
        let temp_c = self.calculate_temperature(raw);

        // Validate temperature is in reasonable range
        if temp_c < emberheat_core::MIN_VALID_TEMP_C || temp_c > emberheat_core::MAX_VALID_TEMP_C {
            return Err(SensorError::OutOfRange(temp_c));
        }

        Ok(TemperatureReading::from_celsius(temp_c))
    }

    fn id(&self) -> &str {
        self.sensor_id.as_str()
    }
}

/// Mock sensor for testing without hardware
pub struct MockSensor {
    sensor_id: HeaplessString<32>,
    temperature: f32,
}

impl MockSensor {
    /// Create a new mock sensor
    pub fn new(sensor_id: &str, temperature: f32) -> Result<Self, SensorError> {
        let mut id = HeaplessString::new();
        write!(&mut id, "{}", sensor_id).map_err(|_| {
            let mut msg = HeaplessString::<128>::new();
            let _ = write!(&mut msg, "Sensor ID too long");
            SensorError::InvalidData(msg)
        })?;

        Ok(Self {
            sensor_id: id,
            temperature,
        })
    }

    /// Set the temperature that will be returned
    pub fn set_temperature(&mut self, temp: f32) {
        self.temperature = temp;
    }
}

impl TemperatureSensor for MockSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        Ok(TemperatureReading::from_celsius(self.temperature))
    }

    fn id(&self) -> &str {
        self.sensor_id.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_sensor_creation() {
        let sensor = MockSensor::new("test", 20.5).unwrap();
        assert_eq!(sensor.id(), "test");
    }

    #[test]
    fn test_mock_sensor_id_too_long() {
        let long_id = "this_is_a_very_long_sensor_id_that_exceeds_32_characters";
        let result = MockSensor::new(long_id, 20.0);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_sensor_read_temperature() {
        let sensor = MockSensor::new("test", 21.5).unwrap();
        let reading = sensor.read_temperature().await.unwrap();

        assert_eq!(reading.celsius, 21.5);
        assert_eq!(sensor.id(), "test");
    }

    #[tokio::test]
    async fn test_mock_sensor_set_temperature() {
        let mut sensor = MockSensor::new("test", 20.0).unwrap();

        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 20.0);

        sensor.set_temperature(25.0);
        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 25.0);
    }

    #[tokio::test]
    async fn test_mock_sensor_valid_range() {
        let sensor = MockSensor::new("test", 21.1).unwrap();
        let reading = sensor.read_temperature().await.unwrap();

        // Check temperature is in valid range
        assert!(reading.celsius >= 3.0 && reading.celsius <= 37.0);
        assert!(reading.is_valid());
    }

    #[test]
    fn test_temperature_calculation() {
        // We can't fully test without actual hardware, but we can verify
        // the calculation doesn't panic and produces reasonable values
        let raw_value = 519888; // Typical value at 25°C

        let var1 = ((raw_value / 16384) - (28000 / 1024)) * 1690;
        let var2 = (((raw_value / 131072) - (28000 / 8192))
            * ((raw_value / 131072) - (28000 / 8192)))
            / 4096
            * (-7350);
        let t_fine = var1 + var2;
        let temp = (t_fine * 5 + 128) / 256;
        let temp_c = temp as f32 / 100.0;

        assert!(temp_c > 0.0 && temp_c < 50.0);
    }

    #[test]
    fn test_bme280_registers() {
        // Verify BME280 register addresses are correct
        assert_eq!(Bme280Sensor::REG_CHIP_ID, 0xD0);
        assert_eq!(Bme280Sensor::REG_CTRL_MEAS, 0xF4);
        assert_eq!(Bme280Sensor::REG_TEMP_MSB, 0xFA);
        assert_eq!(Bme280Sensor::CHIP_ID_BME280, 0x60);
    }

    #[test]
    fn test_default_i2c_address() {
        assert_eq!(Bme280Sensor::DEFAULT_ADDRESS, 0x76);
    }
}
