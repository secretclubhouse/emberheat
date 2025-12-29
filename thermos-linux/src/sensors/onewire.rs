//! 1-Wire DS18B20 temperature sensor implementation
//!
//! This module provides support for reading DS18B20 temperature sensors
//! via the 1-Wire protocol on Linux systems.

use super::{SensorError, TemperatureReading, TemperatureSensor, SENSOR_READ_TIMEOUT};
use anyhow::Result;
use std::path::PathBuf;
use tokio::fs;
use tokio::time::timeout;
use tracing::{debug, trace};

/// 1-Wire base directory on Linux
const ONEWIRE_BASE_PATH: &str = "/sys/bus/w1/devices";

/// 1-Wire temperature sensor (DS18B20)
pub struct OneWireSensor {
    /// Sensor ID (e.g., "00000ab12345")
    sensor_id: String,
    /// Full path to sensor device file
    device_path: PathBuf,
}

impl OneWireSensor {
    /// Create a new 1-Wire sensor
    ///
    /// # Arguments
    ///
    /// * `sensor_id` - Sensor ID string (e.g., "28-00000ab12345" or "00000ab12345")
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New sensor instance or error
    pub fn new(sensor_id: &str) -> Result<Self> {
        // Normalize sensor ID (add family code if missing)
        let normalized_id = if sensor_id.starts_with("28-") {
            sensor_id.to_string()
        } else {
            format!("28-{}", sensor_id)
        };

        let device_path = PathBuf::from(ONEWIRE_BASE_PATH)
            .join(&normalized_id)
            .join("w1_slave");

        debug!(
            "Initialized 1-Wire sensor: {} at {:?}",
            normalized_id, device_path
        );

        Ok(Self {
            sensor_id: normalized_id,
            device_path,
        })
    }

    /// Parse temperature from w1_slave file content
    ///
    /// Expected format:
    /// ```text
    /// 5e 01 4b 46 7f ff 0c 10 1c : crc=1c YES
    /// 5e 01 4b 46 7f ff 0c 10 1c t=21875
    /// ```
    ///
    /// The temperature is in millidegrees Celsius (t=21875 means 21.875°C)
    fn parse_temperature(content: &str) -> Result<f32, SensorError> {
        // Check CRC validity
        if !content.contains("YES") {
            return Err(SensorError::InvalidData("CRC check failed".to_string()));
        }

        // Find temperature value
        let temp_pos = content
            .find("t=")
            .ok_or_else(|| SensorError::InvalidData("Temperature value not found".to_string()))?;

        let temp_str = &content[temp_pos + 2..];
        let temp_end = temp_str
            .find(|c: char| !c.is_numeric() && c != '-')
            .unwrap_or(temp_str.len());

        let millidegrees: i32 = temp_str[..temp_end]
            .parse()
            .map_err(|e| SensorError::InvalidData(format!("Failed to parse temperature: {}", e)))?;

        let celsius = millidegrees as f32 / 1000.0;

        trace!(
            "Parsed temperature: {}°C (raw: {} millidegrees)",
            celsius,
            millidegrees
        );

        Ok(celsius)
    }

    /// List all available 1-Wire sensors
    ///
    /// # Returns
    ///
    /// * `Result<Vec<String>>` - List of sensor IDs
    pub async fn list_sensors() -> Result<Vec<String>> {
        let base_path = PathBuf::from(ONEWIRE_BASE_PATH);

        if !base_path.exists() {
            return Ok(Vec::new());
        }

        let mut sensors = Vec::new();
        let mut entries = fs::read_dir(&base_path).await?;

        while let Some(entry) = entries.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();

            // DS18B20 sensors start with "28-"
            if name_str.starts_with("28-") {
                sensors.push(name_str.to_string());
            }
        }

        debug!("Found {} 1-Wire sensors", sensors.len());

        Ok(sensors)
    }
}

impl TemperatureSensor for OneWireSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        debug!("Reading 1-Wire sensor: {}", self.sensor_id);

        // Read with timeout using tokio
        let content = timeout(SENSOR_READ_TIMEOUT, fs::read_to_string(&self.device_path))
            .await
            .map_err(|_| SensorError::Timeout)?
            .map_err(|e| SensorError::IoError(e.to_string()))?;

        let celsius = Self::parse_temperature(&content)?;
        let reading = TemperatureReading::from_celsius(celsius);

        if !reading.is_valid() {
            return Err(SensorError::OutOfRange(celsius));
        }

        debug!(
            "1-Wire sensor {} reading: {:.1}°C / {:.1}°F",
            self.sensor_id, reading.celsius, reading.fahrenheit
        );

        Ok(reading)
    }

    fn id(&self) -> &str {
        &self.sensor_id
    }

    async fn is_available(&self) -> bool {
        self.device_path.exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_temperature_valid() {
        let content = "5e 01 4b 46 7f ff 0c 10 1c : crc=1c YES\n\
                      5e 01 4b 46 7f ff 0c 10 1c t=21875\n";

        let temp = OneWireSensor::parse_temperature(content).unwrap();
        assert_eq!(temp, 21.875);
    }

    #[test]
    fn test_parse_temperature_negative() {
        let content = "5e 01 4b 46 7f ff 0c 10 1c : crc=1c YES\n\
                      5e 01 4b 46 7f ff 0c 10 1c t=-5500\n";

        let temp = OneWireSensor::parse_temperature(content).unwrap();
        assert_eq!(temp, -5.5);
    }

    #[test]
    fn test_parse_temperature_crc_fail() {
        let content = "5e 01 4b 46 7f ff 0c 10 1c : crc=1c NO\n\
                      5e 01 4b 46 7f ff 0c 10 1c t=21875\n";

        let result = OneWireSensor::parse_temperature(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_temperature_no_value() {
        let content = "5e 01 4b 46 7f ff 0c 10 1c : crc=1c YES\n\
                      5e 01 4b 46 7f ff 0c 10 1c\n";

        let result = OneWireSensor::parse_temperature(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_sensor_id_normalization() {
        let sensor1 = OneWireSensor::new("00000ab12345").unwrap();
        assert_eq!(sensor1.sensor_id, "28-00000ab12345");

        let sensor2 = OneWireSensor::new("28-00000ab12345").unwrap();
        assert_eq!(sensor2.sensor_id, "28-00000ab12345");
    }

    #[test]
    fn test_device_path() {
        let sensor = OneWireSensor::new("00000ab12345").unwrap();
        assert!(sensor
            .device_path
            .to_string_lossy()
            .contains("28-00000ab12345"));
        assert!(sensor.device_path.to_string_lossy().ends_with("w1_slave"));
    }
}
