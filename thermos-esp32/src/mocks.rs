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

/// Mock MQTT publisher for testing
///
/// Captures published messages for verification without real MQTT broker
#[derive(Debug, Clone)]
pub struct MockMqttPublisher {
    messages: Arc<Mutex<Vec<MockMqttMessage>>>,
    should_error: Arc<Mutex<bool>>,
}

#[derive(Debug, Clone)]
pub struct MockMqttMessage {
    pub topic: std::string::String,
    pub payload: std::string::String,
}

impl MockMqttPublisher {
    pub fn new() -> Self {
        Self {
            messages: Arc::new(Mutex::new(Vec::new())),
            should_error: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_error(&self, should_error: bool) {
        *self.should_error.lock().unwrap() = should_error;
    }

    pub fn get_messages(&self) -> Vec<MockMqttMessage> {
        self.messages.lock().unwrap().clone()
    }

    pub fn get_message_count(&self) -> usize {
        self.messages.lock().unwrap().len()
    }

    pub fn clear_messages(&self) {
        self.messages.lock().unwrap().clear();
    }

    pub fn find_messages_by_topic(&self, topic_pattern: &str) -> Vec<MockMqttMessage> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter(|msg| msg.topic.contains(topic_pattern))
            .cloned()
            .collect()
    }

    /// Simulate publishing a temperature reading
    pub async fn publish_temperature(
        &self,
        zone: &str,
        temp_c: f32,
        target_c: f32,
        heating: bool,
    ) -> Result<(), &'static str> {
        if *self.should_error.lock().unwrap() {
            return Err("MQTT publish failed");
        }

        let topic = format!("thermostat/device/temp/{}", zone);
        let payload = format!(
            r#"{{"zone":"{}","temp_c":{:.1},"target_c":{:.1},"heating":{}}}"#,
            zone, temp_c, target_c, heating
        );

        self.messages
            .lock()
            .unwrap()
            .push(MockMqttMessage { topic, payload });

        Ok(())
    }

    /// Simulate publishing a state change event
    pub async fn publish_event(
        &self,
        zone: &str,
        event_type: &str,
        message: &str,
    ) -> Result<(), &'static str> {
        if *self.should_error.lock().unwrap() {
            return Err("MQTT publish failed");
        }

        let topic = format!("thermostat/device/events/{}", zone);
        let payload = format!(
            r#"{{"zone":"{}","event":"{}","message":"{}"}}"#,
            zone, event_type, message
        );

        self.messages
            .lock()
            .unwrap()
            .push(MockMqttMessage { topic, payload });

        Ok(())
    }
}

impl Default for MockMqttPublisher {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock WiFi connection for testing
///
/// Simulates WiFi connection state without real hardware
#[derive(Debug, Clone)]
pub struct MockWifiConnection {
    connected: Arc<Mutex<bool>>,
    ip_address: Arc<Mutex<Option<std::string::String>>>,
    connection_count: Arc<Mutex<usize>>,
    should_fail: Arc<Mutex<bool>>,
}

impl MockWifiConnection {
    pub fn new() -> Self {
        Self {
            connected: Arc::new(Mutex::new(false)),
            ip_address: Arc::new(Mutex::new(None)),
            connection_count: Arc::new(Mutex::new(0)),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock().unwrap()
    }

    pub fn get_ip_address(&self) -> Option<std::string::String> {
        self.ip_address.lock().unwrap().clone()
    }

    pub fn get_connection_count(&self) -> usize {
        *self.connection_count.lock().unwrap()
    }

    pub fn set_should_fail(&self, should_fail: bool) {
        *self.should_fail.lock().unwrap() = should_fail;
    }

    pub async fn connect(&self, _ssid: &str, _password: &str) -> Result<(), &'static str> {
        if *self.should_fail.lock().unwrap() {
            return Err("WiFi connection failed");
        }

        *self.connected.lock().unwrap() = true;
        *self.ip_address.lock().unwrap() = Some("192.168.1.100".to_string());
        *self.connection_count.lock().unwrap() += 1;

        Ok(())
    }

    pub fn disconnect(&self) {
        *self.connected.lock().unwrap() = false;
        *self.ip_address.lock().unwrap() = None;
    }

    pub fn reset(&self) {
        self.disconnect();
        *self.connection_count.lock().unwrap() = 0;
        *self.should_fail.lock().unwrap() = false;
    }
}

impl Default for MockWifiConnection {
    fn default() -> Self {
        Self::new()
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

    #[tokio::test]
    async fn test_mock_mqtt_publisher_temperature() {
        let publisher = MockMqttPublisher::new();

        publisher
            .publish_temperature("Zone1", 21.5, 21.1, true)
            .await
            .unwrap();

        assert_eq!(publisher.get_message_count(), 1);

        let messages = publisher.get_messages();
        assert_eq!(messages[0].topic, "thermostat/device/temp/Zone1");
        assert!(messages[0].payload.contains("\"temp_c\":21.5"));
        assert!(messages[0].payload.contains("\"heating\":true"));
    }

    #[tokio::test]
    async fn test_mock_mqtt_publisher_event() {
        let publisher = MockMqttPublisher::new();

        publisher
            .publish_event("Zone1", "state_change", "Heating ON")
            .await
            .unwrap();

        assert_eq!(publisher.get_message_count(), 1);

        let messages = publisher.get_messages();
        assert_eq!(messages[0].topic, "thermostat/device/events/Zone1");
        assert!(messages[0].payload.contains("\"event\":\"state_change\""));
        assert!(messages[0].payload.contains("\"message\":\"Heating ON\""));
    }

    #[tokio::test]
    async fn test_mock_mqtt_publisher_error() {
        let publisher = MockMqttPublisher::new();

        publisher.set_error(true);
        let result = publisher
            .publish_temperature("Zone1", 21.0, 21.0, false)
            .await;
        assert!(result.is_err());

        publisher.set_error(false);
        let result = publisher
            .publish_temperature("Zone1", 21.0, 21.0, false)
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_mock_mqtt_publisher_find_by_topic() {
        let publisher = MockMqttPublisher::new();

        publisher
            .publish_temperature("Zone1", 21.0, 21.0, false)
            .await
            .unwrap();
        publisher
            .publish_event("Zone1", "test", "message")
            .await
            .unwrap();
        publisher
            .publish_temperature("Zone2", 22.0, 22.0, true)
            .await
            .unwrap();

        let temp_messages = publisher.find_messages_by_topic("/temp/");
        assert_eq!(temp_messages.len(), 2);

        let event_messages = publisher.find_messages_by_topic("/events/");
        assert_eq!(event_messages.len(), 1);

        let zone1_messages = publisher.find_messages_by_topic("Zone1");
        assert_eq!(zone1_messages.len(), 2);
    }

    #[tokio::test]
    async fn test_mock_mqtt_publisher_clear() {
        let publisher = MockMqttPublisher::new();

        publisher
            .publish_temperature("Zone1", 21.0, 21.0, false)
            .await
            .unwrap();
        publisher
            .publish_event("Zone1", "test", "message")
            .await
            .unwrap();
        assert_eq!(publisher.get_message_count(), 2);

        publisher.clear_messages();
        assert_eq!(publisher.get_message_count(), 0);
    }

    #[tokio::test]
    async fn test_mock_wifi_connection_basic() {
        let wifi = MockWifiConnection::new();

        assert!(!wifi.is_connected());
        assert_eq!(wifi.get_ip_address(), None);

        wifi.connect("TestSSID", "password").await.unwrap();

        assert!(wifi.is_connected());
        assert_eq!(wifi.get_ip_address(), Some("192.168.1.100".to_string()));
        assert_eq!(wifi.get_connection_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_wifi_connection_disconnect() {
        let wifi = MockWifiConnection::new();

        wifi.connect("TestSSID", "password").await.unwrap();
        assert!(wifi.is_connected());

        wifi.disconnect();
        assert!(!wifi.is_connected());
        assert_eq!(wifi.get_ip_address(), None);
    }

    #[tokio::test]
    async fn test_mock_wifi_connection_error() {
        let wifi = MockWifiConnection::new();

        wifi.set_should_fail(true);
        let result = wifi.connect("TestSSID", "password").await;
        assert!(result.is_err());
        assert!(!wifi.is_connected());

        wifi.set_should_fail(false);
        let result = wifi.connect("TestSSID", "password").await;
        assert!(result.is_ok());
        assert!(wifi.is_connected());
    }

    #[tokio::test]
    async fn test_mock_wifi_connection_reconnect() {
        let wifi = MockWifiConnection::new();

        wifi.connect("SSID1", "pass1").await.unwrap();
        assert_eq!(wifi.get_connection_count(), 1);

        wifi.disconnect();

        wifi.connect("SSID2", "pass2").await.unwrap();
        assert_eq!(wifi.get_connection_count(), 2);
    }

    #[tokio::test]
    async fn test_mock_wifi_connection_reset() {
        let wifi = MockWifiConnection::new();

        wifi.connect("TestSSID", "password").await.unwrap();
        wifi.set_should_fail(true);
        assert!(wifi.is_connected());
        assert_eq!(wifi.get_connection_count(), 1);

        wifi.reset();

        assert!(!wifi.is_connected());
        assert_eq!(wifi.get_connection_count(), 0);
        assert_eq!(wifi.get_ip_address(), None);

        // Should be able to connect again after reset
        let result = wifi.connect("TestSSID", "password").await;
        assert!(result.is_ok());
    }
}
