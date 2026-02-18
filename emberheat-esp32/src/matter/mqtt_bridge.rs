//! MQTT Bridge for Matter Mode
//!
//! Provides a bridge to run MQTT alongside Matter using the same WiFi connection.
//!
//! # Architecture
//!
//! The rs-matter-embassy stack creates the embassy-net `Stack` internally during
//! `run_coex()`. The `UserTask` trait provides access to:
//! - `net_stack: EnetStack` - implements UDP only (for Matter)
//! - `netif: EnetNetif` - has public `stack: Stack<'d>` field (from our fork)
//!
//! This module implements a `UserTask` that runs MQTT directly within the Matter
//! stack's task context. This avoids RefCell borrow conflicts that occur when
//! trying to use the Stack from a separate task.
//!
//! # Why Run MQTT Inside UserTask?
//!
//! The `embassy_net::Stack` uses `RefCell` for interior mutability. When multiple
//! tasks try to borrow the Stack concurrently (even on a single-threaded executor),
//! a "RefCell already borrowed" panic occurs. By running MQTT inside the UserTask,
//! we ensure proper interleaving with Matter's networking through the `select3`
//! that runs all tasks together.

use core::fmt::Write as FmtWrite;
use core::future::Future;

use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, Timer};

use heapless::String;
use log::{error, info, warn};
use rust_mqtt::client::client::MqttClient;
use rust_mqtt::client::client_config::ClientConfig;
use rust_mqtt::packet::v5::publish_packet::QualityOfService;
use rust_mqtt::utils::rng_generator::CountingRng;

use rs_matter_embassy::enet::get_global_tcp_stack;
use rs_matter_embassy::matter::dm::clusters::gen_diag::NetifDiag;
use rs_matter_embassy::matter::dm::networks::NetChangeNotif;
use rs_matter_embassy::matter::error::Error;
use rs_matter_embassy::stack::nal::NetStack;
use rs_matter_embassy::stack::UserTask;

use crate::mqtt::{MqttConfig, MqttMessage};

/// MQTT queue size (must match main.rs)
pub const MQTT_QUEUE_SIZE: usize = 16;

/// Maximum topic length
const MAX_TOPIC_LEN: usize = 64;

/// MQTT keep-alive interval (seconds)
const MQTT_KEEPALIVE_SECS: u16 = 60;

/// UserTask implementation that runs MQTT alongside Matter.
///
/// This runs as part of the Matter stack's WiFi task via `select3`. It:
/// 1. Extracts the `embassy_net::Stack` from `EnetNetif`
/// 2. Runs MQTT connection and message publishing loop
/// 3. Receives messages from the channel and publishes them
///
/// # Usage
///
/// Pass `MqttBridge::new(config, channel)` as the user task to `run_coex()`:
/// ```ignore
/// stack.run_coex(
///     EmbassyWifi::new(...),
///     &persist,
///     (NODE, handler),
///     MqttBridge::new(mqtt_config, &MQTT_CHANNEL),
/// )
/// ```
pub struct MqttBridge {
    config: MqttConfig,
    channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
}

impl MqttBridge {
    /// Create a new MQTT bridge with the given configuration and message channel.
    pub fn new(
        config: MqttConfig,
        channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
    ) -> Self {
        Self { config, channel }
    }
}

impl UserTask for MqttBridge {
    fn run<S, N>(&mut self, _net_stack: S, _netif: N) -> impl Future<Output = Result<(), Error>>
    where
        S: NetStack,
        N: NetifDiag + NetChangeNotif,
    {
        // Move config and channel into the async block
        let config = self.config.clone();
        let channel = self.channel;

        async move {
            // Get the Stack from global storage.
            // The rs-matter-embassy fork stores it when EnetNetif is created.
            //
            // Safety: This is called from within UserTask::run(), which executes
            // inside the Matter stack's run() via select3(). The Stack stored
            // globally is valid for the entire duration of run(), so it's safe
            // to use here.
            let stack = match unsafe { get_global_tcp_stack() } {
                Some(s) => s,
                None => {
                    error!("MQTT bridge: TCP stack not available!");
                    return Err(Error::new(
                        rs_matter_embassy::matter::error::ErrorCode::Busy,
                    ));
                }
            };

            info!("MQTT bridge: Stack retrieved, starting MQTT task");

            // Run MQTT with retry loop
            mqtt_task_inner(stack, config, channel).await;

            // If MQTT exits (shouldn't happen), return Ok to keep Matter running
            Ok(())
        }
    }
}

/// Inner MQTT task that runs with the extracted stack.
async fn mqtt_task_inner(
    stack: Stack<'static>,
    config: MqttConfig,
    rx_channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
) {
    info!("MQTT task starting (Matter mode)");
    info!("Broker: {}:{}", config.broker_host, config.broker_port);

    // Wait for network to be ready
    info!("Waiting for network link...");
    loop {
        if stack.is_link_up() {
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }
    info!("Network link up");

    // Wait for IP configuration
    info!("Waiting for IP address...");
    loop {
        if stack.config_v4().is_some() {
            if let Some(config) = stack.config_v4() {
                info!("Got IP: {}", config.address);
            }
            break;
        }
        Timer::after(Duration::from_millis(500)).await;
    }

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
    stack: Stack<'static>,
    config: &MqttConfig,
    rx_channel: &'static Channel<CriticalSectionRawMutex, MqttMessage, MQTT_QUEUE_SIZE>,
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
                // Log periodically to avoid spam
                if message_count % 10 == 1 {
                    info!(
                        "MQTT published #{}: {} -> {}",
                        message_count, message.topic, message.payload
                    );
                }
            }
            Err(e) => {
                error!("Failed to publish MQTT message: {:?}", e);
                return Err("MQTT publish failed");
            }
        }

        // Small yield to allow other tasks to run
        Timer::after(Duration::from_millis(10)).await;
    }
}

#[cfg(test)]
mod tests {
    // Tests would require mocking the Stack which is complex
    // The integration is tested via actual hardware runs
}
