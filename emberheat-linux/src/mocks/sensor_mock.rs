//! Mock temperature sensor for testing

use crate::sensors::{SensorError, TemperatureReading, TemperatureSensor};
use anyhow::Result;
use std::sync::{Arc, Mutex};

/// Convert a std::String to heapless::String, truncating if necessary
fn to_heapless(s: &str) -> heapless::String<128> {
    let mut hs = heapless::String::new();
    for c in s.chars().take(127) {
        let _ = hs.push(c);
    }
    hs
}

/// Mock temperature sensor that returns configurable values
///
/// This mock allows tests to simulate various sensor conditions:
/// - Normal temperature readings
/// - Sensor failures
/// - Out-of-range temperatures
/// - Sensor unavailability
#[derive(Clone)]
pub struct MockTemperatureSensor {
    /// Sensor identifier
    id: String,
    /// Current temperature reading (Celsius)
    temperature: Arc<Mutex<Option<f32>>>,
    /// Whether the sensor is available
    available: Arc<Mutex<bool>>,
    /// Number of times read_temperature has been called
    read_count: Arc<Mutex<usize>>,
}

impl MockTemperatureSensor {
    /// Create a new mock sensor with a default temperature
    pub fn new(id: &str, initial_temp_c: f32) -> Self {
        Self {
            id: id.to_string(),
            temperature: Arc::new(Mutex::new(Some(initial_temp_c))),
            available: Arc::new(Mutex::new(true)),
            read_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a new unavailable sensor
    pub fn new_unavailable(id: &str) -> Self {
        Self {
            id: id.to_string(),
            temperature: Arc::new(Mutex::new(None)),
            available: Arc::new(Mutex::new(false)),
            read_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Create a sensor that returns an error
    pub fn new_failing(id: &str) -> Self {
        Self {
            id: id.to_string(),
            temperature: Arc::new(Mutex::new(None)),
            available: Arc::new(Mutex::new(true)),
            read_count: Arc::new(Mutex::new(0)),
        }
    }

    /// Set the temperature reading
    pub fn set_temperature(&self, temp_c: f32) {
        let mut temp = self.temperature.lock().unwrap();
        *temp = Some(temp_c);
    }

    /// Set sensor availability
    #[allow(dead_code)]
    pub fn set_available(&self, available: bool) {
        let mut avail = self.available.lock().unwrap();
        *avail = available;
    }

    /// Make the sensor fail (return error on read)
    #[allow(dead_code)]
    pub fn set_failing(&self) {
        let mut temp = self.temperature.lock().unwrap();
        *temp = None;
    }

    /// Get the number of times read_temperature was called
    pub fn get_read_count(&self) -> usize {
        *self.read_count.lock().unwrap()
    }

    /// Reset the read count
    #[allow(dead_code)]
    pub fn reset_read_count(&self) {
        let mut count = self.read_count.lock().unwrap();
        *count = 0;
    }
}

impl TemperatureSensor for MockTemperatureSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        // Increment read count
        {
            let mut count = self.read_count.lock().unwrap();
            *count += 1;
        }

        // Check if sensor is available
        if !*self.available.lock().unwrap() {
            return Err(SensorError::NotFound(to_heapless(&self.id)));
        }

        // Get temperature
        let temp = self.temperature.lock().unwrap();

        match *temp {
            Some(celsius) => Ok(TemperatureReading::from_celsius(celsius)),
            None => Err(SensorError::IoError(to_heapless("Sensor read failed"))),
        }
    }

    fn id(&self) -> &str {
        &self.id
    }

    async fn is_available(&self) -> bool {
        *self.available.lock().unwrap()
    }
}

/// Builder for creating mock sensors with specific behaviors
pub struct MockSensorBuilder {
    id: String,
    temperature: Option<f32>,
    available: bool,
}

impl MockSensorBuilder {
    /// Create a new builder
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            temperature: Some(20.0), // Default 20°C
            available: true,
        }
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temp_c: f32) -> Self {
        self.temperature = Some(temp_c);
        self
    }

    /// Make the sensor unavailable
    pub fn unavailable(mut self) -> Self {
        self.available = false;
        self
    }

    /// Make the sensor fail on read
    #[allow(dead_code)]
    pub fn failing(mut self) -> Self {
        self.temperature = None;
        self
    }

    /// Build the mock sensor
    pub fn build(self) -> MockTemperatureSensor {
        MockTemperatureSensor {
            id: self.id,
            temperature: Arc::new(Mutex::new(self.temperature)),
            available: Arc::new(Mutex::new(self.available)),
            read_count: Arc::new(Mutex::new(0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_sensor_normal_reading() {
        let sensor = MockTemperatureSensor::new("test-sensor", 22.5);

        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 22.5);
        assert_eq!(sensor.get_read_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_sensor_unavailable() {
        let sensor = MockTemperatureSensor::new_unavailable("test-sensor");

        assert!(!sensor.is_available().await);
        assert!(sensor.read_temperature().await.is_err());
    }

    #[tokio::test]
    async fn test_mock_sensor_failing() {
        let sensor = MockTemperatureSensor::new_failing("test-sensor");

        assert!(sensor.read_temperature().await.is_err());
        assert_eq!(sensor.get_read_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_sensor_temperature_change() {
        let sensor = MockTemperatureSensor::new("test-sensor", 20.0);

        let reading1 = sensor.read_temperature().await.unwrap();
        assert_eq!(reading1.celsius, 20.0);

        sensor.set_temperature(25.0);

        let reading2 = sensor.read_temperature().await.unwrap();
        assert_eq!(reading2.celsius, 25.0);
    }

    #[tokio::test]
    async fn test_mock_sensor_builder() {
        let sensor = MockSensorBuilder::new("test-sensor")
            .with_temperature(18.5)
            .build();

        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 18.5);
    }

    #[tokio::test]
    async fn test_mock_sensor_builder_unavailable() {
        let sensor = MockSensorBuilder::new("test-sensor").unavailable().build();

        assert!(!sensor.is_available().await);
    }
}
