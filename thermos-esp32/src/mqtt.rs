//! MQTT temperature history logger for ESP32-C6
//!
//! Publishes temperature readings and state changes to an MQTT broker for
//! historical tracking and monitoring. Uses rust-mqtt for no_std compatibility.

use core::fmt::Write as FmtWrite;
use core::str::FromStr;
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::{Duration, Timer};
use embedded_io_async::Write;
use heapless::String;
use log::{error, info, warn};
use rust_mqtt::client::client::MqttClient;
use rust_mqtt::client::client_config::ClientConfig;
use rust_mqtt::packet::v5::publish_packet::QualityOfService;
use rust_mqtt::utils::rng_generator::CountingRng;

/// MQTT message queue depth (buffered messages before blocking)
const MQTT_QUEUE_SIZE: usize = 16;

/// MQTT keep-alive interval (seconds)
const MQTT_KEEPALIVE_SECS: u16 = 60;

/// Maximum topic length (thermostat/{device_id}/events/{zone})
const MAX_TOPIC_LEN: usize = 64;

/// Maximum payload length (JSON with temperature data)
const MAX_PAYLOAD_LEN: usize = 256;

/// MQTT configuration
#[derive(Clone)]
pub struct MqttConfig {
    /// MQTT broker hostname or IP address
    pub broker_host: String<64>,
    /// MQTT broker port (default: 1883)
    pub broker_port: u16,
    /// Device identifier for topic hierarchy
    pub device_id: String<32>,
    /// Client ID for MQTT connection
    pub client_id: String<32>,
    /// Optional username for authentication
    pub username: Option<String<32>>,
    /// Optional password for authentication
    pub password: Option<String<64>>,
}

impl MqttConfig {
    /// Create a new MQTT configuration without authentication
    pub fn new(broker_host: &str, broker_port: u16, device_id: &str) -> Result<Self, &'static str> {
        Self::with_auth(broker_host, broker_port, device_id, None, None)
    }

    /// Create a new MQTT configuration with optional authentication
    pub fn with_auth(
        broker_host: &str,
        broker_port: u16,
        device_id: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<Self, &'static str> {
        let broker_host =
            String::from_str(broker_host).map_err(|_| "Broker host too long (max 64 chars)")?;
        let device_id =
            String::from_str(device_id).map_err(|_| "Device ID too long (max 32 chars)")?;

        // Generate client ID from device ID
        let mut client_id: String<32> = String::new();
        let _ = write!(client_id, "thermos-{}", device_id);

        // Parse optional credentials
        let username = match username {
            Some(u) => Some(String::from_str(u).map_err(|_| "Username too long (max 32 chars)")?),
            None => None,
        };
        let password = match password {
            Some(p) => Some(String::from_str(p).map_err(|_| "Password too long (max 64 chars)")?),
            None => None,
        };

        Ok(Self {
            broker_host,
            broker_port,
            device_id,
            client_id,
            username,
            password,
        })
    }
}

/// MQTT message to be published
#[derive(Clone)]
pub struct MqttMessage {
    pub topic: String<MAX_TOPIC_LEN>,
    pub payload: String<MAX_PAYLOAD_LEN>,
}

/// MQTT publisher with async channel for message queueing
pub struct MqttPublisher {
    sender: Sender<'static, CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
    config: MqttConfig,
}

impl MqttPublisher {
    /// Create a new MQTT publisher
    pub fn new(
        sender: Sender<'static, CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
        config: MqttConfig,
    ) -> Self {
        Self { sender, config }
    }

    /// Publish a temperature reading
    pub async fn publish_temperature(
        &self,
        zone: &str,
        temp_c: f32,
        target_c: f32,
        heating: bool,
    ) -> Result<(), &'static str> {
        // Build topic: thermostat/{device_id}/temp/{zone}
        let mut topic: String<MAX_TOPIC_LEN> = String::new();
        write!(topic, "thermostat/{}/temp/{}", self.config.device_id, zone)
            .map_err(|_| "Topic too long")?;

        // Build JSON payload with temperature data
        let mut payload: String<MAX_PAYLOAD_LEN> = String::new();
        write!(
            payload,
            r#"{{"zone":"{}","temp_c":{:.1},"temp_f":{:.1},"target_c":{:.1},"heating":{}}}"#,
            zone,
            temp_c,
            celsius_to_fahrenheit(temp_c),
            target_c,
            heating
        )
        .map_err(|_| "Payload too long")?;

        let message = MqttMessage { topic, payload };

        // Send to background task (non-blocking)
        self.sender.send(message).await;
        Ok(())
    }

    /// Publish a state change event
    pub async fn publish_event(
        &self,
        zone: &str,
        event_type: &str,
        message: &str,
    ) -> Result<(), &'static str> {
        // Build topic: thermostat/{device_id}/events/{zone}
        let mut topic: String<MAX_TOPIC_LEN> = String::new();
        write!(
            topic,
            "thermostat/{}/events/{}",
            self.config.device_id, zone
        )
        .map_err(|_| "Topic too long")?;

        // Build JSON payload with event data
        let mut payload: String<MAX_PAYLOAD_LEN> = String::new();
        write!(
            payload,
            r#"{{"zone":"{}","event":"{}","message":"{}"}}"#,
            zone, event_type, message
        )
        .map_err(|_| "Payload too long")?;

        let msg = MqttMessage { topic, payload };

        // Send to background task (non-blocking)
        self.sender.send(msg).await;
        Ok(())
    }

    /// Publish device status (health check)
    pub async fn publish_status(&self, status: &str) -> Result<(), &'static str> {
        // Build topic: thermostat/{device_id}/status
        let mut topic: String<MAX_TOPIC_LEN> = String::new();
        write!(topic, "thermostat/{}/status", self.config.device_id)
            .map_err(|_| "Topic too long")?;

        // Build JSON payload with status
        let mut payload: String<MAX_PAYLOAD_LEN> = String::new();
        write!(payload, r#"{{"status":"{}"}}"#, status).map_err(|_| "Payload too long")?;

        let message = MqttMessage { topic, payload };

        // Send to background task (non-blocking)
        self.sender.send(message).await;
        Ok(())
    }
}

/// Embassy task for MQTT connection and publishing
pub async fn mqtt_task(
    stack: &'static Stack<'static>,
    config: MqttConfig,
    rx_channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
) {
    info!("MQTT task starting");
    info!("Broker: {}:{}", config.broker_host, config.broker_port);

    // Retry connection loop with exponential backoff
    let mut backoff_secs = 1u64;
    const MAX_BACKOFF_SECS: u64 = 60;

    loop {
        info!("Connecting to MQTT broker...");

        match mqtt_connect_and_publish(stack, &config, rx_channel).await {
            Ok(_) => {
                info!("MQTT connection closed gracefully");
                backoff_secs = 1; // Reset backoff on clean disconnect
            }
            Err(e) => {
                error!("MQTT connection failed: {}", e);
            }
        }

        // Exponential backoff
        warn!("Reconnecting in {} seconds...", backoff_secs);
        Timer::after(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}

/// Connect to MQTT broker and publish messages from queue
async fn mqtt_connect_and_publish(
    stack: &'static Stack<'static>,
    config: &MqttConfig,
    rx_channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
) -> Result<(), &'static str> {
    // Create TCP socket for MQTT connection
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut socket = TcpSocket::new(*stack, &mut rx_buffer, &mut tx_buffer);
    socket.set_timeout(Some(Duration::from_secs(10)));

    // Resolve broker hostname (DNS lookup)
    info!("Resolving DNS for {}...", config.broker_host);
    let broker_addr = match stack
        .dns_query(
            config.broker_host.as_str(),
            embassy_net::dns::DnsQueryType::A,
        )
        .await
    {
        Ok(addrs) => {
            if addrs.is_empty() {
                return Err("DNS returned no addresses");
            }
            addrs[0]
        }
        Err(_) => return Err("DNS lookup failed"),
    };

    info!("Connecting to {}:{}...", broker_addr, config.broker_port);
    socket
        .connect((broker_addr, config.broker_port))
        .await
        .map_err(|_| "TCP connect failed")?;

    info!("TCP connected, initializing MQTT client");

    // Create MQTT client configuration
    let mut mqtt_config = ClientConfig::new(
        rust_mqtt::client::client_config::MqttVersion::MQTTv5,
        CountingRng(20000),
    );
    mqtt_config.add_max_subscribe_qos(QualityOfService::QoS0);
    mqtt_config.add_client_id(config.client_id.as_str());
    mqtt_config.max_packet_size = 1024;
    mqtt_config.keep_alive = MQTT_KEEPALIVE_SECS;

    // Add authentication if configured
    if let Some(ref username) = config.username {
        mqtt_config.add_username(username.as_str());
        info!("MQTT auth: using username '{}'", username);
    }
    if let Some(ref password) = config.password {
        mqtt_config.add_password(password.as_str());
    }

    // Create MQTT client
    let mut recv_buffer = [0; 1024];
    let mut write_buffer = [0; 1024];

    let mut client = MqttClient::<_, 5, _>::new(
        socket,
        &mut write_buffer,
        1024,
        &mut recv_buffer,
        1024,
        mqtt_config,
    );

    // Connect to MQTT broker
    info!("Sending MQTT CONNECT");
    client
        .connect_to_broker()
        .await
        .map_err(|_| "MQTT connect failed")?;

    info!("MQTT connected successfully");

    // Publish device online status
    let mut status_topic: String<MAX_TOPIC_LEN> = String::new();
    write!(status_topic, "thermostat/{}/status", config.device_id)
        .map_err(|_| "Status topic too long")?;

    client
        .send_message(
            status_topic.as_str(),
            b"{\"status\":\"online\"}",
            QualityOfService::QoS0,
            false,
        )
        .await
        .map_err(|_| "Failed to publish online status")?;

    info!("Published online status, entering message loop");

    // Message publishing loop
    let mut message_count: u32 = 0;
    loop {
        // Wait for message from queue
        let message = rx_channel.receive().await;

        // Publish to MQTT broker
        match client
            .send_message(
                message.topic.as_str(),
                message.payload.as_bytes(),
                QualityOfService::QoS0,
                false,
            )
            .await
        {
            Ok(_) => {
                message_count += 1;
                // Log every message for debugging (can reduce later)
                info!(
                    "MQTT published #{}: {} -> {}",
                    message_count, message.topic, message.payload
                );
            }
            Err(e) => {
                error!("Failed to publish MQTT message: {:?}", e);
                return Err("MQTT publish failed");
            }
        }

        // Small yield to allow other tasks to run and prevent overwhelming the network
        Timer::after(Duration::from_millis(10)).await;
    }
}

/// Convert Celsius to Fahrenheit
fn celsius_to_fahrenheit(celsius: f32) -> f32 {
    celsius * 9.0 / 5.0 + 32.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mqtt_config_creation() {
        let config = MqttConfig::new("192.168.1.100", 1883, "esp32-001").unwrap();
        assert_eq!(config.broker_host.as_str(), "192.168.1.100");
        assert_eq!(config.broker_port, 1883);
        assert_eq!(config.device_id.as_str(), "esp32-001");
        assert!(config.client_id.starts_with("thermos-"));
        assert!(config.username.is_none());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_mqtt_config_with_auth() {
        let config = MqttConfig::with_auth(
            "192.168.1.100",
            1883,
            "esp32-001",
            Some("mqtt_user"),
            Some("mqtt_pass"),
        )
        .unwrap();
        assert_eq!(config.broker_host.as_str(), "192.168.1.100");
        assert_eq!(config.username.as_ref().unwrap().as_str(), "mqtt_user");
        assert_eq!(config.password.as_ref().unwrap().as_str(), "mqtt_pass");
    }

    #[test]
    fn test_mqtt_config_partial_auth() {
        // Username only (no password)
        let config =
            MqttConfig::with_auth("192.168.1.100", 1883, "esp32-001", Some("mqtt_user"), None)
                .unwrap();
        assert!(config.username.is_some());
        assert!(config.password.is_none());
    }

    #[test]
    fn test_celsius_to_fahrenheit() {
        assert_eq!(celsius_to_fahrenheit(0.0), 32.0);
        assert_eq!(celsius_to_fahrenheit(100.0), 212.0);
        assert!((celsius_to_fahrenheit(21.1) - 70.0).abs() < 0.1);
    }
}
