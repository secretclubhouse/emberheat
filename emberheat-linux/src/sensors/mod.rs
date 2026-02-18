//! Temperature sensor interfaces
//!
//! This module provides abstraction for reading temperature from multiple sources:
//! - 1-Wire DS18B20 sensors (primary)
//! - HTTP endpoints (fallback/supplementary)
//!
//! # Temperature Reading Strategy
//!
//! 1. Attempt to read from 1-Wire sensor
//! 2. If configured, fall back to HTTP sensor
//! 3. Validate temperature is within reasonable range
//! 4. Trigger power cycling if sensor returns invalid readings
//!
//! # Example
//!
//! ```no_run
//! use emberheat::sensors::{TemperatureSensor, OneWireSensor};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let sensor = OneWireSensor::new("00000ab12345")?;
//!     let temp = sensor.read_temperature().await?;
//!     println!("Temperature: {}°C", temp);
//!     Ok(())
//! }
//! ```

#![allow(dead_code)]

use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;
use tracing::{debug, warn};

// Re-export from emberheat-core
pub use emberheat_core::{
    SensorError, TemperatureReading, TemperatureSensor, SENSOR_READ_TIMEOUT_SECS,
};

/// Sensor read timeout as Duration
const SENSOR_READ_TIMEOUT: Duration = Duration::from_secs(SENSOR_READ_TIMEOUT_SECS);

mod onewire;
pub use onewire::OneWireSensor;

/// Convert a std::String to heapless::String, truncating if necessary
fn to_heapless(s: &str) -> heapless::String<128> {
    let mut hs = heapless::String::new();
    for c in s.chars().take(127) {
        let _ = hs.push(c);
    }
    hs
}

/// HTTP-based temperature sensor
///
/// Fetches temperature from an HTTP endpoint that returns JSON:
/// ```json
/// {"temp_c": 20.5}
/// ```
pub struct HttpSensor {
    url: String,
    client: reqwest::Client,
}

#[derive(Deserialize)]
struct HttpTempResponse {
    temp_c: f32,
}

impl HttpSensor {
    /// Create a new HTTP sensor
    ///
    /// # Arguments
    ///
    /// * `url` - HTTP endpoint URL
    pub fn new(url: String) -> Result<Self> {
        let client = reqwest::Client::builder()
            .timeout(SENSOR_READ_TIMEOUT)
            .build()
            .context("Failed to create HTTP client")?;

        Ok(Self { url, client })
    }
}

impl TemperatureSensor for HttpSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        debug!("Reading temperature from HTTP sensor: {}", self.url);

        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| SensorError::HttpError(to_heapless(&e.to_string())))?;

        let data: HttpTempResponse = response
            .json()
            .await
            .map_err(|e| SensorError::InvalidData(to_heapless(&e.to_string())))?;

        let reading = TemperatureReading::from_celsius(data.temp_c);

        if !reading.is_valid() {
            return Err(SensorError::OutOfRange(reading.celsius));
        }

        debug!(
            "HTTP sensor reading: {:.1}°C / {:.1}°F",
            reading.celsius, reading.fahrenheit
        );

        Ok(reading)
    }

    fn id(&self) -> &str {
        &self.url
    }

    async fn is_available(&self) -> bool {
        self.read_temperature().await.is_ok()
    }
}

/// Composite sensor that tries multiple sources
///
/// Attempts to read from primary 1-Wire sensor first, then falls back to HTTP sensor.
/// Uses concrete types to avoid dyn-compatibility issues with async trait methods.
pub struct CompositeSensor {
    primary: OneWireSensor,
    fallback: Option<HttpSensor>,
}

impl CompositeSensor {
    /// Create from 1-Wire sensor with optional HTTP fallback
    pub fn from_onewire(sensor_id: &str, http_url: Option<String>) -> Result<Self> {
        let primary = OneWireSensor::new(sensor_id)?;

        let fallback = if let Some(url) = http_url {
            Some(HttpSensor::new(url)?)
        } else {
            None
        };

        Ok(Self { primary, fallback })
    }
}

impl TemperatureSensor for CompositeSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        // Try primary sensor first
        match self.primary.read_temperature().await {
            Ok(reading) => Ok(reading),
            Err(primary_err) => {
                warn!(
                    "Primary sensor {} failed: {}",
                    self.primary.id(),
                    primary_err
                );

                // Try fallback if available
                if let Some(fallback) = &self.fallback {
                    debug!("Attempting fallback sensor: {}", fallback.id());
                    match fallback.read_temperature().await {
                        Ok(reading) => {
                            debug!("Fallback sensor succeeded");
                            Ok(reading)
                        }
                        Err(fallback_err) => {
                            warn!("Fallback sensor also failed: {}", fallback_err);
                            Err(primary_err) // Return original SensorError
                        }
                    }
                } else {
                    Err(primary_err)
                }
            }
        }
    }

    fn id(&self) -> &str {
        self.primary.id()
    }

    async fn is_available(&self) -> bool {
        if self.primary.is_available().await {
            return true;
        }
        if let Some(fallback) = &self.fallback {
            return fallback.is_available().await;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use emberheat_core::{MAX_VALID_TEMP_C, MIN_VALID_TEMP_C};

    #[test]
    fn test_temperature_reading_from_core() {
        // Test that TemperatureReading from core works
        let reading = TemperatureReading::from_celsius(20.0);
        assert_eq!(reading.celsius, 20.0);
        assert_eq!(reading.fahrenheit, 68.0);

        let reading = TemperatureReading::from_celsius(0.0);
        assert_eq!(reading.fahrenheit, 32.0);
    }

    #[test]
    fn test_temperature_validation_from_core() {
        let valid = TemperatureReading::from_celsius(20.0);
        assert!(valid.is_valid());

        let too_cold = TemperatureReading::from_celsius(2.0);
        assert!(!too_cold.is_valid());

        let edge_min = TemperatureReading::from_celsius(MIN_VALID_TEMP_C);
        assert!(edge_min.is_valid());

        let edge_max = TemperatureReading::from_celsius(MAX_VALID_TEMP_C);
        assert!(edge_max.is_valid());
    }

    #[tokio::test]
    async fn test_http_sensor_invalid_url() {
        let sensor = HttpSensor::new("http://invalid.local/temp".to_string()).unwrap();
        let result = sensor.read_temperature().await;
        assert!(result.is_err());
    }

    #[test]
    fn test_sensor_error_display() {
        let err = SensorError::OutOfRange(50.0);
        assert_eq!(err.to_string(), "Temperature out of valid range: 50°C");

        let err = SensorError::Timeout;
        assert_eq!(err.to_string(), "Sensor read timeout");
    }
}
