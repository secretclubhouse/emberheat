//! Host-runnable mock tests for ESP32 thermostat logic
//!
//! These tests use only the mock implementations from the mocks module
//! and can run on any platform (including development hosts).

#![cfg(test)]
#![cfg(not(target_arch = "riscv32"))]

use emberheat_core::{
    apply_hysteresis_control, GpioError, HeatingState, HysteresisConfig, OutputPin,
    PersistenceError, RelayState, SensorError, StateStorage, TemperatureReading, TemperatureSensor,
    ThermostatState,
};
use std::sync::{Arc, Mutex};

// Simplified mocks that only depend on std
#[derive(Debug, Clone)]
struct TestGpio {
    state: Arc<Mutex<bool>>,
}

impl TestGpio {
    fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(true)), // Start high (OFF)
        }
    }

    fn is_heating(&self) -> bool {
        !*self.state.lock().unwrap() // Active low
    }
}

impl OutputPin for TestGpio {
    fn set_high(&mut self) -> Result<(), GpioError> {
        *self.state.lock().unwrap() = true;
        Ok(())
    }

    fn set_low(&mut self) -> Result<(), GpioError> {
        *self.state.lock().unwrap() = false;
        Ok(())
    }

    fn is_set_low(&self) -> Result<bool, GpioError> {
        Ok(!*self.state.lock().unwrap())
    }
}

#[derive(Debug, Clone)]
struct TestSensor {
    temperature: Arc<Mutex<f32>>,
}

impl TestSensor {
    fn new(temp: f32) -> Self {
        Self {
            temperature: Arc::new(Mutex::new(temp)),
        }
    }

    fn set_temp(&self, temp: f32) {
        *self.temperature.lock().unwrap() = temp;
    }
}

impl TemperatureSensor for TestSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        let temp = *self.temperature.lock().unwrap();
        Ok(TemperatureReading::from_celsius(temp))
    }

    fn id(&self) -> &str {
        "test-sensor"
    }
}

#[derive(Debug, Clone)]
struct TestStorage {
    data: Arc<Mutex<Option<ThermostatState>>>,
}

impl TestStorage {
    fn new() -> Self {
        Self {
            data: Arc::new(Mutex::new(None)),
        }
    }
}

impl StateStorage for TestStorage {
    async fn save_state(
        &mut self,
        _zone_name: &str,
        state: &ThermostatState,
    ) -> Result<(), PersistenceError> {
        *self.data.lock().unwrap() = Some(state.clone());
        Ok(())
    }

    async fn load_state(
        &mut self,
        _zone_name: &str,
    ) -> Result<Option<ThermostatState>, PersistenceError> {
        Ok(self.data.lock().unwrap().clone())
    }

    async fn delete_state(&mut self, _zone_name: &str) -> Result<(), PersistenceError> {
        *self.data.lock().unwrap() = None;
        Ok(())
    }

    async fn ping(&mut self) -> Result<(), PersistenceError> {
        Ok(())
    }
}

#[tokio::test]
async fn test_gpio_control() {
    let mut gpio = TestGpio::new();

    // Start OFF
    assert!(!gpio.is_heating());

    // Turn ON
    gpio.set_low().unwrap();
    assert!(gpio.is_heating());

    // Turn OFF
    gpio.set_high().unwrap();
    assert!(!gpio.is_heating());
}

#[tokio::test]
async fn test_sensor_reading() {
    let sensor = TestSensor::new(21.5);

    let reading = sensor.read_temperature().await.unwrap();
    assert_eq!(reading.celsius, 21.5);

    sensor.set_temp(25.0);
    let reading = sensor.read_temperature().await.unwrap();
    assert_eq!(reading.celsius, 25.0);
}

#[tokio::test]
async fn test_storage() {
    let mut storage = TestStorage::new();
    let state = ThermostatState::default();

    storage.save_state("Zone1", &state).await.unwrap();
    let loaded = storage.load_state("Zone1").await.unwrap();
    assert!(loaded.is_some());

    storage.delete_state("Zone1").await.unwrap();
    let loaded = storage.load_state("Zone1").await.unwrap();
    assert!(loaded.is_none());
}

#[tokio::test]
async fn test_hysteresis_turn_on() {
    let sensor = TestSensor::new(20.0); // Below target
    let config = HysteresisConfig::default();
    let target = 21.1;

    let reading = sensor.read_temperature().await.unwrap();
    let state = apply_hysteresis_control(reading.celsius, target, HeatingState::Off, &config);

    assert_eq!(state, HeatingState::On);
}

#[tokio::test]
async fn test_hysteresis_turn_off() {
    let sensor = TestSensor::new(22.0); // Above target
    let config = HysteresisConfig::default();
    let target = 21.1;

    let reading = sensor.read_temperature().await.unwrap();
    let state = apply_hysteresis_control(reading.celsius, target, HeatingState::On, &config);

    assert_eq!(state, HeatingState::Off);
}

#[tokio::test]
async fn test_control_loop_cycle() {
    let mut gpio = TestGpio::new();
    let sensor = TestSensor::new(19.0);
    let mut storage = TestStorage::new();
    let config = HysteresisConfig::default();
    let target = 21.1;
    let mut current_state = HeatingState::Off;

    // Temperature rises
    for temp in [19.0, 20.0, 21.0, 21.6, 22.0] {
        sensor.set_temp(temp);
        let reading = sensor.read_temperature().await.unwrap();
        current_state = apply_hysteresis_control(reading.celsius, target, current_state, &config);

        // Apply to GPIO
        match current_state {
            HeatingState::On => gpio.set_low().unwrap(),
            HeatingState::Off => gpio.set_high().unwrap(),
        }

        // Save state
        let state = ThermostatState {
            relay_pin: 4,
            temp_pin: 6,
            temp_id: sensor.id().to_string(),
            target_temp: target,
            target_state: RelayState::On as u8,
            current_state: current_state as u8,
        };
        storage.save_state("Zone1", &state).await.unwrap();
    }

    // Should be OFF at end (temp > target + 0.5)
    assert_eq!(current_state, HeatingState::Off);
    assert!(!gpio.is_heating());
}
