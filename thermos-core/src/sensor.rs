//! Temperature sensor trait and utilities
//!
//! This module provides the core temperature sensor abstraction that can be
//! implemented by different sensor backends (1-Wire, I2C, HTTP, etc.).

use crate::types::TemperatureReading;
use core::fmt;

#[cfg(not(feature = "std"))]
use alloc::boxed::Box;

/// Sensor read timeout in seconds
pub const SENSOR_READ_TIMEOUT_SECS: u64 = 3;

/// Sensor errors
#[derive(Debug)]
pub enum SensorError {
    Timeout,
    OutOfRange(f32),
    NotFound(heapless::String<128>),
    IoError(heapless::String<128>),
    HttpError(heapless::String<128>),
    InvalidData(heapless::String<128>),
    Unavailable(heapless::String<128>),
}

impl fmt::Display for SensorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SensorError::Timeout => write!(f, "Sensor read timeout"),
            SensorError::OutOfRange(temp) => {
                write!(f, "Temperature out of valid range: {}°C", temp)
            }
            SensorError::NotFound(msg) => write!(f, "Sensor not found: {}", msg),
            SensorError::IoError(msg) => write!(f, "I/O error: {}", msg),
            SensorError::HttpError(msg) => write!(f, "HTTP request failed: {}", msg),
            SensorError::InvalidData(msg) => write!(f, "Invalid sensor data: {}", msg),
            SensorError::Unavailable(msg) => write!(f, "Sensor unavailable: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for SensorError {}

/// Temperature sensor trait
///
/// This trait defines the interface for reading temperature from any sensor type.
/// Implementations must be Send + Sync to work with async runtimes.
///
/// Note: Uses native async fn in trait (requires Rust 1.75+)
pub trait TemperatureSensor: Send + Sync {
    /// Read temperature from the sensor
    ///
    /// # Returns
    ///
    /// * `Result<TemperatureReading, SensorError>` - Temperature reading or error
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError>;

    /// Get sensor identifier
    ///
    /// This should return a unique identifier for the sensor (e.g., 1-Wire ID,
    /// I2C address, HTTP URL).
    fn id(&self) -> &str;

    /// Check if sensor is available
    ///
    /// This is a lightweight check to see if the sensor can be accessed.
    /// The default implementation tries to read temperature.
    async fn is_available(&self) -> bool {
        self.read_temperature().await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mock sensor for testing
    struct MockSensor {
        id: String,
        temp_c: f32,
        should_error: bool,
    }

    impl TemperatureSensor for MockSensor {
        async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
            if self.should_error {
                Err(SensorError::Timeout)
            } else {
                Ok(TemperatureReading::from_celsius(self.temp_c))
            }
        }

        fn id(&self) -> &str {
            &self.id
        }
    }

    #[tokio::test]
    async fn test_mock_sensor_read() {
        let sensor = MockSensor {
            id: "test-sensor".to_string(),
            temp_c: 20.0,
            should_error: false,
        };

        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 20.0);
    }

    #[tokio::test]
    async fn test_mock_sensor_error() {
        let sensor = MockSensor {
            id: "test-sensor".to_string(),
            temp_c: 20.0,
            should_error: true,
        };

        let result = sensor.read_temperature().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sensor_availability() {
        let good_sensor = MockSensor {
            id: "good".to_string(),
            temp_c: 20.0,
            should_error: false,
        };
        assert!(good_sensor.is_available().await);

        let bad_sensor = MockSensor {
            id: "bad".to_string(),
            temp_c: 20.0,
            should_error: true,
        };
        assert!(!bad_sensor.is_available().await);
    }
}
