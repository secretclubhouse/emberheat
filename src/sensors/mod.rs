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
//! use thermos_rust::sensors::{TemperatureSensor, OneWireSensor};
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
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, warn};

mod onewire;
pub use onewire::OneWireSensor;

/// Sensor errors
#[derive(Error, Debug)]
pub enum SensorError {
    #[error("Sensor read timeout")]
    Timeout,

    #[error("Temperature out of valid range: {0}°C")]
    OutOfRange(f32),

    #[error("Sensor not found: {0}")]
    NotFound(String),

    #[error("I/O error: {0}")]
    IoError(String),

    #[error("HTTP request failed: {0}")]
    HttpError(String),

    #[error("Invalid sensor data: {0}")]
    InvalidData(String),
}

/// Temperature reading result
#[derive(Debug, Clone, Copy)]
pub struct TemperatureReading {
    /// Temperature in Celsius
    pub celsius: f32,
    /// Temperature in Fahrenheit
    pub fahrenheit: f32,
}

impl TemperatureReading {
    /// Create a new temperature reading from Celsius
    pub fn from_celsius(celsius: f32) -> Self {
        Self {
            celsius,
            fahrenheit: celsius * 9.0 / 5.0 + 32.0,
        }
    }

    /// Validate temperature is within normal range
    ///
    /// Valid range: 3°C to 37°C (37°F to 99°F)
    pub fn is_valid(&self) -> bool {
        self.celsius >= MIN_VALID_TEMP_C && self.celsius <= MAX_VALID_TEMP_C
    }
}

/// Minimum valid temperature in Celsius
pub const MIN_VALID_TEMP_C: f32 = 3.0;

/// Maximum valid temperature in Celsius
pub const MAX_VALID_TEMP_C: f32 = 37.0;

/// Default temperature on error (21.1°C / 70°F)
pub const DEFAULT_TEMP_C: f32 = 21.1;

/// Sensor read timeout
pub const SENSOR_READ_TIMEOUT: Duration = Duration::from_secs(3);

/// Temperature sensor trait
#[async_trait]
pub trait TemperatureSensor: Send + Sync {
    /// Read temperature from the sensor
    ///
    /// # Returns
    ///
    /// * `Result<TemperatureReading>` - Temperature reading or error
    async fn read_temperature(&self) -> Result<TemperatureReading>;

    /// Get sensor identifier
    fn id(&self) -> &str;

    /// Check if sensor is available
    async fn is_available(&self) -> bool;
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

#[async_trait]
impl TemperatureSensor for HttpSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading> {
        debug!("Reading temperature from HTTP sensor: {}", self.url);

        let response = self
            .client
            .get(&self.url)
            .send()
            .await
            .map_err(|e| SensorError::HttpError(e.to_string()))?;

        let data: HttpTempResponse = response
            .json()
            .await
            .map_err(|e| SensorError::InvalidData(e.to_string()))?;

        let reading = TemperatureReading::from_celsius(data.temp_c);

        if !reading.is_valid() {
            return Err(SensorError::OutOfRange(reading.celsius).into());
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
/// Attempts to read from primary sensor first, then falls back to secondary
pub struct CompositeSensor {
    primary: Box<dyn TemperatureSensor>,
    fallback: Option<Box<dyn TemperatureSensor>>,
}

impl CompositeSensor {
    /// Create a new composite sensor
    ///
    /// # Arguments
    ///
    /// * `primary` - Primary temperature sensor
    /// * `fallback` - Optional fallback sensor
    pub fn new(
        primary: Box<dyn TemperatureSensor>,
        fallback: Option<Box<dyn TemperatureSensor>>,
    ) -> Self {
        Self { primary, fallback }
    }

    /// Create from 1-Wire sensor with optional HTTP fallback
    pub fn from_onewire(sensor_id: &str, http_url: Option<String>) -> Result<Self> {
        let primary = Box::new(OneWireSensor::new(sensor_id)?);

        let fallback = if let Some(url) = http_url {
            Some(Box::new(HttpSensor::new(url)?) as Box<dyn TemperatureSensor>)
        } else {
            None
        };

        Ok(Self { primary, fallback })
    }
}

#[async_trait]
impl TemperatureSensor for CompositeSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading> {
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
                            Err(primary_err) // Return original error
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
        self.primary.is_available().await
            || self.fallback.as_ref().map(|f| f.is_available()).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_reading_conversion() {
        let reading = TemperatureReading::from_celsius(20.0);
        assert_eq!(reading.celsius, 20.0);
        assert_eq!(reading.fahrenheit, 68.0);

        let reading = TemperatureReading::from_celsius(0.0);
        assert_eq!(reading.fahrenheit, 32.0);

        let reading = TemperatureReading::from_celsius(100.0);
        assert_eq!(reading.fahrenheit, 212.0);
    }

    #[test]
    fn test_temperature_validation() {
        let valid = TemperatureReading::from_celsius(20.0);
        assert!(valid.is_valid());

        let too_cold = TemperatureReading::from_celsius(2.0);
        assert!(!too_cold.is_valid());

        let too_hot = TemperatureReading::from_celsius(38.0);
        assert!(!too_hot.is_valid());

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
