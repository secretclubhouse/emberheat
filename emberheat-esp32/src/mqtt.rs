//! MQTT temperature history logger for ESP32-C6
//!
//! Publishes temperature readings and state changes to an MQTT broker for
//! historical tracking and monitoring, and receives remote sensor readings on
//! `thermostat/<device_id>/zone/+/sensor/+`. Uses rust-mqtt for no_std
//! compatibility.

use core::fmt::Write as FmtWrite;
use core::num::NonZero;
use core::str::FromStr;
use embassy_futures::select::{select3, Either3};
use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Sender};
use embassy_time::{with_timeout, Duration, Instant, Timer};
use emberheat_core::remote::parse_sensor_topic;
use heapless::String;
use log::{debug, error, info, warn};
use rust_mqtt::buffer::BumpBuffer;
use rust_mqtt::client::event::{Event, Publish};
use rust_mqtt::client::options::{
    ConnectOptions, PublicationOptions, RetainHandling, SubscriptionOptions, TopicReference,
};
use rust_mqtt::client::Client;
use rust_mqtt::config::KeepAlive;
use rust_mqtt::types::{MqttBinary, MqttString, TopicFilter, TopicName};
use rust_mqtt::Bytes;

use crate::sensor_remote::record_remote_reading;

/// MQTT message queue depth (buffered messages before blocking)
const MQTT_QUEUE_SIZE: usize = 16;

/// MQTT keep-alive interval (seconds)
const MQTT_KEEPALIVE_SECS: u16 = 60;

/// Interval between PINGREQs (seconds), well inside the keep-alive
const MQTT_PING_INTERVAL_SECS: u64 = 30;

/// Largest packet the broker may send us; keeps incoming data well inside the
/// 1024-byte bump buffer
const MQTT_MAX_PACKET_SIZE: u32 = 512;

/// How long to wait for the broker to acknowledge the sensor subscription
const SUBACK_TIMEOUT_SECS: u64 = 10;

/// How long to wait for a packet body once its header has arrived
const POLL_BODY_TIMEOUT_SECS: u64 = 10;

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
        let _ = write!(client_id, "emberheat-{}", device_id);

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

        let mut subscribed = false;
        match mqtt_connect_and_publish(*stack, &config, rx_channel, &mut subscribed).await {
            Ok(_) => {
                info!("MQTT connection closed gracefully");
                backoff_secs = 1; // Reset backoff on clean disconnect
            }
            Err(e) => {
                error!("MQTT connection failed: {}", e);
            }
        }
        // A session that got as far as SUBACK was healthy; retry it promptly.
        if subscribed {
            backoff_secs = 1;
        }

        // Exponential backoff
        warn!("Reconnecting in {} seconds...", backoff_secs);
        Timer::after(Duration::from_secs(backoff_secs)).await;
        backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
    }
}

/// Connect to MQTT broker, subscribe to remote sensors, and publish messages
/// from the queue until the connection fails
///
/// Sets `subscribed` once the broker has acknowledged the sensor
/// subscription, so callers can tell a working session that later dropped
/// (reset the reconnect backoff) from a failure to connect at all.
pub(crate) async fn mqtt_connect_and_publish(
    stack: Stack<'static>,
    config: &MqttConfig,
    rx_channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
    subscribed: &mut bool,
) -> Result<(), &'static str> {
    // Create TCP socket for MQTT connection
    let mut rx_buffer = [0; 4096];
    let mut tx_buffer = [0; 4096];
    let mut socket = TcpSocket::new(stack, &mut rx_buffer, &mut tx_buffer);
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

    // Scratch space the client uses for data it receives from the broker
    let mut mqtt_buffer = [0u8; 1024];
    let mut mqtt_buffer = BumpBuffer::new(&mut mqtt_buffer);

    // One subscription (remote sensors), QoS 0 only, no topic aliases
    let mut client = Client::<'_, '_, _, _, 1, 1, 1, 1, 0, 0, 0>::new(&mut mqtt_buffer);

    // The packet size cap tells the broker not to send us anything that could
    // overflow the 1024-byte bump buffer; it drops oversized messages instead.
    let mut connect_options = ConnectOptions::new()
        .clean_start()
        .keep_alive(KeepAlive::Seconds(
            NonZero::new(MQTT_KEEPALIVE_SECS).ok_or("Keep-alive must be non-zero")?,
        ))
        .maximum_packet_size(
            NonZero::new(MQTT_MAX_PACKET_SIZE).ok_or("Max packet size must be non-zero")?,
        );

    // Add authentication if configured
    if let Some(ref username) = config.username {
        connect_options = connect_options.user_name(
            MqttString::try_from(username.as_str()).map_err(|_| "Invalid MQTT username")?,
        );
        info!("MQTT auth: using username '{}'", username);
    }
    if let Some(ref password) = config.password {
        connect_options = connect_options.password(
            MqttBinary::try_from(password.as_str()).map_err(|_| "Invalid MQTT password")?,
        );
    }

    // Connect to MQTT broker
    info!("Sending MQTT CONNECT");
    let client_id =
        MqttString::try_from(config.client_id.as_str()).map_err(|_| "Invalid MQTT client id")?;
    client
        .connect(socket, &connect_options, Some(client_id))
        .await
        .map_err(|_| "MQTT connect failed")?;

    // The CONNACK data is no longer needed; release the scratch space
    unsafe {
        client.buffer_mut().reset();
    }

    info!("MQTT connected successfully");

    // Subscribe to remote sensor readings before announcing we are online, so
    // a sensor reacting to the status message cannot publish into a gap.
    let mut sensor_filter: String<MAX_TOPIC_LEN> = String::new();
    write!(
        sensor_filter,
        "thermostat/{}/zone/+/sensor/+",
        config.device_id
    )
    .map_err(|_| "Sensor topic filter too long")?;
    let sensor_filter =
        MqttString::try_from(sensor_filter.as_str()).map_err(|_| "Invalid sensor topic filter")?;
    let sensor_filter = TopicFilter::new(sensor_filter).ok_or("Invalid sensor topic filter")?;
    // Retained readings may be arbitrarily old; never have the broker replay them.
    let sub_options = SubscriptionOptions::new().retain_handling(RetainHandling::NeverSend);
    let sub_pid = client
        .subscribe(sensor_filter, &sub_options)
        .await
        .map_err(|e| {
            error!("MQTT subscribe error: {:?}", e);
            "MQTT subscribe failed"
        })?;

    with_timeout(Duration::from_secs(SUBACK_TIMEOUT_SECS), async {
        loop {
            // Only the outcome leaves this block, so nothing borrows the
            // buffer when it is reset below.
            let suback = match client.poll().await {
                Ok(Event::Suback(suback)) if suback.packet_identifier == sub_pid => {
                    Some(suback.reason_code.is_success())
                }
                Ok(_) => None,
                Err(e) => {
                    error!("MQTT poll error while waiting for SUBACK: {:?}", e);
                    return Err("MQTT poll failed");
                }
            };
            unsafe {
                client.buffer_mut().reset();
            }
            match suback {
                Some(true) => return Ok(()),
                Some(false) => return Err("Subscribe rejected"),
                None => continue,
            }
        }
    })
    .await
    .map_err(|_| "No SUBACK")??;

    info!("Subscribed to remote sensors");
    *subscribed = true;

    // Publish device online status
    let mut status_topic: String<MAX_TOPIC_LEN> = String::new();
    write!(status_topic, "thermostat/{}/status", config.device_id)
        .map_err(|_| "Status topic too long")?;

    publish_qos0(
        &mut client,
        status_topic.as_str(),
        b"{\"status\":\"online\"}",
    )
    .await
    .map_err(|_| "Failed to publish online status")?;

    info!("Published online status, entering message loop");

    // Publish queued messages, receive sensor readings, and ping the broker.
    // Only `poll_header` (cancel-safe) is ever dropped by the select; the
    // non-cancel-safe `poll_body`, `publish` and `ping` run to completion
    // inside their arms.
    let mut message_count: u32 = 0;
    // Pings run on a fixed 30 s cadence, independent of other traffic, so
    // frequent incoming messages cannot starve the keep-alive.
    let mut next_ping = Instant::now() + Duration::from_secs(MQTT_PING_INTERVAL_SECS);
    loop {
        match select3(
            rx_channel.receive(),
            client.poll_header(),
            Timer::at(next_ping),
        )
        .await
        {
            Either3::First(message) => {
                match publish_qos0(
                    &mut client,
                    message.topic.as_str(),
                    message.payload.as_bytes(),
                )
                .await
                {
                    Ok(()) => {
                        message_count += 1;
                        // Log periodically to avoid spam
                        if message_count % 10 == 1 {
                            info!(
                                "MQTT published #{}: {} -> {}",
                                message_count, message.topic, message.payload
                            );
                        }
                    }
                    Err(()) => {
                        error!("Failed to publish MQTT message to {}", message.topic);
                        return Err("MQTT publish failed");
                    }
                }

                // Pace outgoing publishes so they don't overwhelm the network
                Timer::after(Duration::from_millis(10)).await;
            }
            Either3::Second(header) => {
                let header = header.map_err(|e| {
                    error!("MQTT poll header error: {:?}", e);
                    "MQTT poll failed"
                })?;
                // The event borrows the bump buffer; it is consumed inside this
                // block so nothing references the buffer when it is reset.
                //
                // The body can stall forever if the link dies between header
                // and body, so it gets a deadline. Dropping a half-read
                // `poll_body` is safe only because the error return below
                // discards this client and socket; the reconnect loop builds
                // fresh ones.
                {
                    let event = with_timeout(
                        Duration::from_secs(POLL_BODY_TIMEOUT_SECS),
                        client.poll_body(header),
                    )
                    .await
                    .map_err(|_| {
                        error!("MQTT poll body timed out");
                        "MQTT poll body timed out"
                    })?
                    .map_err(|e| {
                        error!("MQTT poll body error: {:?}", e);
                        "MQTT poll failed"
                    })?;
                    if let Event::Publish(publish) = event {
                        handle_sensor_publish(config, &publish);
                    }
                }
                unsafe {
                    client.buffer_mut().reset();
                }
            }
            Either3::Third(()) => {
                client.ping().await.map_err(|e| {
                    error!("MQTT ping error: {:?}", e);
                    "MQTT ping failed"
                })?;
                next_ping = Instant::now() + Duration::from_secs(MQTT_PING_INTERVAL_SECS);
            }
        }
    }
}

/// Feeds an incoming PUBLISH on the sensor subscription into the zone store.
///
/// Retained messages are dropped: their age is unknown, so they could make a
/// long-dead sensor look fresh.
fn handle_sensor_publish(config: &MqttConfig, publish: &Publish<'_, 1, 0>) {
    let Some(topic) = publish.topic.name() else {
        // Unreachable with no inbound topic aliases allowed, but harmless.
        debug!("MQTT publish without topic name dropped");
        return;
    };
    let topic = AsRef::<MqttString>::as_ref(topic).as_str();
    // This only guards subscribe-time replays of retained messages (also
    // refused via RetainHandling::NeverSend). Under MQTT v5 the broker clears
    // the retain flag when forwarding a live publish to an existing
    // subscription, so a reading published with -r while we are connected
    // arrives with retain = false and is accepted, as it should be.
    if publish.retain {
        debug!("Retained MQTT message on {} dropped", topic);
        return;
    }
    match parse_sensor_topic(topic, config.device_id.as_str()) {
        Some((zone, sensor_id)) => record_remote_reading(zone, sensor_id, &publish.message),
        None => debug!("MQTT message on unexpected topic {} dropped", topic),
    }
}

/// The MQTT client type used by this firmware (one subscription, QoS 0)
type MqttClient<'a, 'c, 's> = Client<'a, 'c, TcpSocket<'s>, BumpBuffer<'c>, 1, 1, 1, 1, 0, 0, 0>;

/// Publish one QoS 0 (fire-and-forget) message
async fn publish_qos0(
    client: &mut MqttClient<'_, '_, '_>,
    topic: &str,
    payload: &[u8],
) -> Result<(), ()> {
    let topic = MqttString::try_from(topic).map_err(|_| ())?;
    let topic = TopicName::new(topic).ok_or(())?;
    let options = PublicationOptions::new(TopicReference::Name(topic));

    client
        .publish(&options, Bytes::from(payload))
        .await
        .map(|_| ())
        .map_err(|e| error!("MQTT publish error: {:?}", e))
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
        assert!(config.client_id.starts_with("emberheat-"));
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
