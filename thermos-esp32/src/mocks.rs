//! Mock implementations for testing ESP32 code without hardware
//!
//! This module provides mock implementations that can be used in tests
//! to verify logic without requiring actual ESP32 hardware.

#![cfg(test)]

extern crate std;
use std::sync::{Arc, Mutex};

use thermos_core::{
    GpioError, OutputPin, PersistenceError, SensorError, StateStorage, TemperatureReading,
    TemperatureSensor, ThermostatState,
};

/// Mock GPIO pin for testing
///
/// Tracks the pin state (high/low) and provides inspection methods
#[derive(Debug, Clone)]
pub struct MockGpioPin {
    state: Arc<Mutex<bool>>, // true = high, false = low
    set_count: Arc<Mutex<usize>>,
}

impl MockGpioPin {
    pub fn new(initial_high: bool) -> Self {
        Self {
            state: Arc::new(Mutex::new(initial_high)),
            set_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn get_state(&self) -> bool {
        *self.state.lock().unwrap()
    }

    pub fn get_set_count(&self) -> usize {
        *self.set_count.lock().unwrap()
    }

    pub fn reset_count(&self) {
        *self.set_count.lock().unwrap() = 0;
    }
}

impl OutputPin for MockGpioPin {
    fn set_high(&mut self) -> Result<(), GpioError> {
        *self.state.lock().unwrap() = true;
        *self.set_count.lock().unwrap() += 1;
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), GpioError> {
        *self.state.lock().unwrap() = false;
        *self.set_count.lock().unwrap() += 1;
        Ok(())
    }

    fn is_set_low(&self) -> Result<bool, GpioError> {
        Ok(!*self.state.lock().unwrap())
    }
}

/// Mock temperature sensor for testing
///
/// Returns configurable temperature values and can simulate errors
#[derive(Debug, Clone)]
pub struct MockTempSensor {
    sensor_id: std::string::String,
    temperature: Arc<Mutex<f32>>,
    should_error: Arc<Mutex<bool>>,
    read_count: Arc<Mutex<usize>>,
}

impl MockTempSensor {
    pub fn new(sensor_id: &str, initial_temp: f32) -> Self {
        Self {
            sensor_id: sensor_id.to_string(),
            temperature: Arc::new(Mutex::new(initial_temp)),
            should_error: Arc::new(Mutex::new(false)),
            read_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn set_temperature(&self, temp: f32) {
        *self.temperature.lock().unwrap() = temp;
    }

    pub fn set_error(&self, should_error: bool) {
        *self.should_error.lock().unwrap() = should_error;
    }

    pub fn get_read_count(&self) -> usize {
        *self.read_count.lock().unwrap()
    }

    pub fn reset_read_count(&self) {
        *self.read_count.lock().unwrap() = 0;
    }
}

impl TemperatureSensor for MockTempSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        *self.read_count.lock().unwrap() += 1;

        if *self.should_error.lock().unwrap() {
            use core::fmt::Write;
            let mut msg = heapless::String::<128>::new();
            let _ = write!(&mut msg, "Simulated sensor error");
            return Err(SensorError::Unavailable(msg));
        }

        let temp_c = *self.temperature.lock().unwrap();
        Ok(TemperatureReading::from_celsius(temp_c))
    }

    fn id(&self) -> &str {
        &self.sensor_id
    }
}

/// Mock state storage for testing
///
/// Stores state in memory instead of NVS flash
#[derive(Debug, Clone)]
pub struct MockStorage {
    data: Arc<Mutex<std::collections::HashMap<std::string::String, ThermostatState>>>,
    should_error: Arc<Mutex<bool>>,
}

impl MockStorage {
    pub fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(std::collections::HashMap::new())),
            should_error: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_error(&self, should_error: bool) {
        *self.should_error.lock().unwrap() = should_error;
    }

    pub fn get_stored_count(&self) -> usize {
        self.data.lock().unwrap().len()
    }

    pub fn clear(&self) {
        self.data.lock().unwrap().clear();
    }
}

impl Default for MockStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStorage for MockStorage {
    async fn save_state(
        &mut self,
        zone_name: &str,
        state: &ThermostatState,
    ) -> Result<(), PersistenceError> {
        if *self.should_error.lock().unwrap() {
            use core::fmt::Write;
            let mut msg = heapless::String::<128>::new();
            let _ = write!(&mut msg, "Simulated storage error");
            return Err(PersistenceError::StorageError(msg));
        }

        self.data
            .lock()
            .unwrap()
            .insert(zone_name.to_string(), state.clone());
        Ok(())
    }

    async fn load_state(
        &mut self,
        zone_name: &str,
    ) -> Result<Option<ThermostatState>, PersistenceError> {
        if *self.should_error.lock().unwrap() {
            use core::fmt::Write;
            let mut msg = heapless::String::<128>::new();
            let _ = write!(&mut msg, "Simulated storage error");
            return Err(PersistenceError::StorageError(msg));
        }

        Ok(self.data.lock().unwrap().get(zone_name).cloned())
    }

    async fn delete_state(&mut self, zone_name: &str) -> Result<(), PersistenceError> {
        if *self.should_error.lock().unwrap() {
            use core::fmt::Write;
            let mut msg = heapless::String::<128>::new();
            let _ = write!(&mut msg, "Simulated storage error");
            return Err(PersistenceError::StorageError(msg));
        }

        self.data.lock().unwrap().remove(zone_name);
        Ok(())
    }

    async fn ping(&mut self) -> Result<(), PersistenceError> {
        if *self.should_error.lock().unwrap() {
            use core::fmt::Write;
            let mut msg = heapless::String::<128>::new();
            let _ = write!(&mut msg, "Simulated storage error");
            return Err(PersistenceError::StorageError(msg));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_gpio_basic() {
        let mut pin = MockGpioPin::new(true);

        assert_eq!(pin.get_state(), true);
        assert_eq!(pin.is_set_low().unwrap(), false);

        pin.set_low().unwrap();
        assert_eq!(pin.get_state(), false);
        assert_eq!(pin.is_set_low().unwrap(), true);
        assert_eq!(pin.get_set_count(), 1);

        pin.set_high().unwrap();
        assert_eq!(pin.get_state(), true);
        assert_eq!(pin.get_set_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_sensor_basic() {
        let sensor = MockTempSensor::new("test-sensor", 20.0);

        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 20.0);
        assert_eq!(sensor.get_read_count(), 1);

        sensor.set_temperature(25.0);
        let reading = sensor.read_temperature().await.unwrap();
        assert_eq!(reading.celsius, 25.0);
        assert_eq!(sensor.get_read_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_sensor_error() {
        let sensor = MockTempSensor::new("test-sensor", 20.0);

        sensor.set_error(true);
        let result = sensor.read_temperature().await;
        assert!(result.is_err());

        sensor.set_error(false);
        let result = sensor.read_temperature().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_storage_save_load() {
        let mut storage = MockStorage::new();
        let state = ThermostatState::default();

        storage.save_state("Zone1", &state).await.unwrap();
        assert_eq!(storage.get_stored_count(), 1);

        let loaded = storage.load_state("Zone1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().target_temp, state.target_temp);
    }

    #[tokio::test]
    async fn test_mock_storage_delete() {
        let mut storage = MockStorage::new();
        let state = ThermostatState::default();

        storage.save_state("Zone1", &state).await.unwrap();
        assert_eq!(storage.get_stored_count(), 1);

        storage.delete_state("Zone1").await.unwrap();
        assert_eq!(storage.get_stored_count(), 0);

        let loaded = storage.load_state("Zone1").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_mock_storage_error() {
        let mut storage = MockStorage::new();
        let state = ThermostatState::default();

        storage.set_error(true);
        let result = storage.save_state("Zone1", &state).await;
        assert!(result.is_err());

        storage.set_error(false);
        let result = storage.save_state("Zone1", &state).await;
        assert!(result.is_ok());
    }
}
