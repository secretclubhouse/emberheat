//! Integration tests for ESP32 thermostat implementation
//!
//! These tests verify the integration between different ESP32 components
//! using mock implementations that can run on the host.

#![cfg(test)]

use emberheat_core::{
    apply_hysteresis_control, HeatingState, HysteresisConfig, OutputPin, RelayState, StateStorage,
    TemperatureReading, TemperatureSensor, ThermostatState,
};

// Mock implementations for testing
mod mocks {
    use super::*;
    use emberheat_core::{GpioError, PersistenceError, SensorError};
    use std::sync::{Arc, Mutex};

    #[derive(Debug, Clone)]
    pub struct MockGpio {
        state: Arc<Mutex<bool>>,
    }

    impl MockGpio {
        pub fn new() -> Self {
            Self {
                state: Arc::new(Mutex::new(true)), // Start high (OFF)
            }
        }

        pub fn is_heating(&self) -> bool {
            !*self.state.lock().unwrap() // Active low
        }
    }

    impl OutputPin for MockGpio {
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
    pub struct MockSensor {
        temperature: Arc<Mutex<f32>>,
    }

    impl MockSensor {
        pub fn new(temp: f32) -> Self {
            Self {
                temperature: Arc::new(Mutex::new(temp)),
            }
        }

        pub fn set_temp(&self, temp: f32) {
            *self.temperature.lock().unwrap() = temp;
        }
    }

    impl TemperatureSensor for MockSensor {
        async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
            let temp = *self.temperature.lock().unwrap();
            Ok(TemperatureReading::from_celsius(temp))
        }

        fn id(&self) -> &str {
            "mock-sensor"
        }
    }

    #[derive(Debug, Clone)]
    pub struct MockStorage {
        data: Arc<Mutex<Option<ThermostatState>>>,
    }

    impl MockStorage {
        pub fn new() -> Self {
            Self {
                data: Arc::new(Mutex::new(None)),
            }
        }

        pub fn get_saved_state(&self) -> Option<ThermostatState> {
            self.data.lock().unwrap().clone()
        }
    }

    impl StateStorage for MockStorage {
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
}

use mocks::*;

#[tokio::test]
async fn test_hysteresis_control_turn_on() {
    let mut gpio = MockGpio::new();
    let sensor = MockSensor::new(20.0); // Below target
    let config = HysteresisConfig::default();
    let target_temp = 21.1;

    let reading = sensor.read_temperature().await.unwrap();
    let new_state =
        apply_hysteresis_control(reading.celsius, target_temp, HeatingState::Off, &config);

    assert_eq!(new_state, HeatingState::On);

    // Apply the state to GPIO
    match new_state {
        HeatingState::On => gpio.set_low().unwrap(),
        HeatingState::Off => gpio.set_high().unwrap(),
    }

    assert!(gpio.is_heating());
}

#[tokio::test]
async fn test_hysteresis_control_turn_off() {
    let mut gpio = MockGpio::new();
    let sensor = MockSensor::new(22.0); // Above target
    let config = HysteresisConfig::default();
    let target_temp = 21.1;

    let reading = sensor.read_temperature().await.unwrap();
    let new_state =
        apply_hysteresis_control(reading.celsius, target_temp, HeatingState::On, &config);

    assert_eq!(new_state, HeatingState::Off);

    // Apply the state to GPIO
    match new_state {
        HeatingState::On => gpio.set_low().unwrap(),
        HeatingState::Off => gpio.set_high().unwrap(),
    }

    assert!(!gpio.is_heating());
}

#[tokio::test]
async fn test_hysteresis_maintains_state_in_dead_band() {
    let mut gpio = MockGpio::new();
    let sensor = MockSensor::new(21.1); // Exactly at target
    let config = HysteresisConfig::default();
    let target_temp = 21.1;

    // Should maintain OFF state
    let reading = sensor.read_temperature().await.unwrap();
    let new_state =
        apply_hysteresis_control(reading.celsius, target_temp, HeatingState::Off, &config);
    assert_eq!(new_state, HeatingState::Off);

    // Should maintain ON state
    let new_state =
        apply_hysteresis_control(reading.celsius, target_temp, HeatingState::On, &config);
    assert_eq!(new_state, HeatingState::On);
}

#[tokio::test]
async fn test_temperature_control_cycle() {
    let mut gpio = MockGpio::new();
    let sensor = MockSensor::new(19.0); // Start cold
    let config = HysteresisConfig::default();
    let target_temp = 21.1;
    let mut current_state = HeatingState::Off;

    // Temperature rises from 19°C to 22°C, should turn on then off
    let temps = [19.0, 20.0, 20.5, 21.0, 21.3, 21.6, 22.0];
    let expected_states = [
        HeatingState::On,  // 19°C < 20.6°C (target - 0.5)
        HeatingState::On,  // 20°C < 20.6°C
        HeatingState::On,  // 20.5°C < 20.6°C
        HeatingState::On,  // 21.0°C < 20.6°C? No, >= 20.6, maintain ON
        HeatingState::On,  // 21.3°C, still in dead band
        HeatingState::Off, // 21.6°C > 21.6°C (target + 0.5)
        HeatingState::Off, // 22.0°C > 21.6°C
    ];

    for (temp, expected) in temps.iter().zip(expected_states.iter()) {
        sensor.set_temp(*temp);
        let reading = sensor.read_temperature().await.unwrap();
        current_state =
            apply_hysteresis_control(reading.celsius, target_temp, current_state, &config);

        assert_eq!(current_state, *expected, "Failed at temp {}", temp);
    }
}

#[tokio::test]
async fn test_state_persistence() {
    let mut storage = MockStorage::new();
    let state = ThermostatState {
        relay_pin: 4,
        temp_pin: 6,
        #[cfg(feature = "std")]
        temp_id: "test-sensor".to_string(),
        #[cfg(not(feature = "std"))]
        temp_id: {
            let mut s = heapless::String::new();
            use core::fmt::Write;
            write!(&mut s, "test-sensor").unwrap();
            s
        },
        target_temp: 21.5,
        target_state: RelayState::On as u8,
        current_state: RelayState::Off as u8,
    };

    // Save state
    storage.save_state("Zone1", &state).await.unwrap();

    // Load state
    let loaded = storage.load_state("Zone1").await.unwrap();
    assert!(loaded.is_some());
    let loaded = loaded.unwrap();
    assert_eq!(loaded.target_temp, 21.5);
    assert_eq!(loaded.relay_pin, 4);

    // Verify in-memory storage
    let saved = storage.get_saved_state();
    assert!(saved.is_some());
    assert_eq!(saved.unwrap().target_temp, 21.5);
}

#[tokio::test]
async fn test_gpio_relay_control() {
    let mut gpio = MockGpio::new();

    // Initially OFF (high)
    assert!(!gpio.is_heating());

    // Turn ON (low)
    gpio.set_low().unwrap();
    assert!(gpio.is_heating());
    assert!(gpio.is_set_low().unwrap());

    // Turn OFF (high)
    gpio.set_high().unwrap();
    assert!(!gpio.is_heating());
    assert!(!gpio.is_set_low().unwrap());
}

#[tokio::test]
async fn test_sensor_temperature_reading() {
    let sensor = MockSensor::new(21.5);

    let reading = sensor.read_temperature().await.unwrap();
    assert_eq!(reading.celsius, 21.5);
    assert_eq!(reading.fahrenheit, 70.7);
    assert!(reading.is_valid());

    // Change temperature
    sensor.set_temp(25.0);
    let reading = sensor.read_temperature().await.unwrap();
    assert_eq!(reading.celsius, 25.0);
}

#[tokio::test]
async fn test_full_control_loop_simulation() {
    // Simulate a complete control loop iteration
    let mut gpio = MockGpio::new();
    let sensor = MockSensor::new(20.0); // Start below target
    let mut storage = MockStorage::new();
    let config = HysteresisConfig::default();
    let target_temp = 21.1;

    // Initial state: OFF, cold
    let mut current_state = HeatingState::Off;

    // First iteration: should turn ON
    let reading = sensor.read_temperature().await.unwrap();
    current_state = apply_hysteresis_control(reading.celsius, target_temp, current_state, &config);
    assert_eq!(current_state, HeatingState::On);
    gpio.set_low().unwrap(); // Apply state
    assert!(gpio.is_heating());

    // Save state
    let state = ThermostatState {
        relay_pin: 4,
        temp_pin: 6,
        #[cfg(feature = "std")]
        temp_id: sensor.id().to_string(),
        #[cfg(not(feature = "std"))]
        temp_id: {
            let mut s = heapless::String::new();
            use core::fmt::Write;
            write!(&mut s, "{}", sensor.id()).unwrap();
            s
        },
        target_temp,
        target_state: RelayState::On as u8,
        current_state: current_state as u8,
    };
    storage.save_state("Zone1", &state).await.unwrap();

    // Simulate heating: temperature rises
    sensor.set_temp(22.0); // Above target + threshold

    // Second iteration: should turn OFF
    let reading = sensor.read_temperature().await.unwrap();
    current_state = apply_hysteresis_control(reading.celsius, target_temp, current_state, &config);
    assert_eq!(current_state, HeatingState::Off);
    gpio.set_high().unwrap(); // Apply state
    assert!(!gpio.is_heating());

    // Verify state was saved
    let loaded = storage.load_state("Zone1").await.unwrap();
    assert!(loaded.is_some());
}
