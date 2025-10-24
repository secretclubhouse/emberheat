//! Prometheus metrics collection and export
//!
//! This module provides Prometheus metrics for monitoring the thermostat system.
//! Metrics include temperature readings, heating status, sensor response times,
//! and error counts.
//!
//! # Exported Metrics
//!
//! ## Gauges (current values)
//! - `current_temperature` - Current room temperature in Fahrenheit
//! - `target_temperature` - Target setpoint temperature in Fahrenheit
//! - `heat_status` - Relay state (0=off, 1=on)
//! - `response_time` - Sensor read latency in seconds
//!
//! ## Counters (cumulative)
//! - `reset_error_count` - Number of sensor power cycle attempts
//!
//! All metrics include labels: `room` (zone name), `heat_status` (0/1)
//!
//! # Example
//!
//! ```no_run
//! use thermos_rust::metrics::MetricsServer;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let server = MetricsServer::new(8080)?;
//!     server.run().await?;
//!     Ok(())
//! }
//! ```

use anyhow::Result;
use lazy_static::lazy_static;
use prometheus::{
    register_counter_vec, register_gauge_vec, CounterVec, Encoder, GaugeVec, TextEncoder,
};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::{debug, error, info};

lazy_static! {
    /// Current temperature in Fahrenheit
    pub static ref CURRENT_TEMP: GaugeVec = register_gauge_vec!(
        "current_temperature",
        "Current room temperature in Fahrenheit",
        &["room", "heat_status"]
    )
    .unwrap();

    /// Target temperature in Fahrenheit
    pub static ref TARGET_TEMP: GaugeVec = register_gauge_vec!(
        "target_temperature",
        "Target setpoint temperature in Fahrenheit",
        &["room", "heat_status"]
    )
    .unwrap();

    /// Heat status (0=off, 1=on)
    pub static ref HEAT_STATUS: GaugeVec = register_gauge_vec!(
        "heat_status",
        "Relay state (0=off, 1=on)",
        &["room"]
    )
    .unwrap();

    /// Sensor response time in seconds
    pub static ref RESPONSE_TIME: GaugeVec = register_gauge_vec!(
        "response_time",
        "Sensor read latency in seconds",
        &["room"]
    )
    .unwrap();

    /// Reset error count
    pub static ref RESET_ERROR_COUNT: CounterVec = register_counter_vec!(
        "reset_error_count",
        "Number of sensor power cycle attempts due to errors",
        &["room"]
    )
    .unwrap();
}

/// Metrics server for Prometheus export
pub struct MetricsServer {
    port: u16,
}

impl MetricsServer {
    /// Create a new metrics server
    ///
    /// # Arguments
    ///
    /// * `port` - Port to listen on (typically 8080)
    pub fn new(port: u16) -> Result<Self> {
        info!("Metrics server initialized on port {}", port);
        Ok(Self { port })
    }

    /// Run the metrics server
    ///
    /// Serves Prometheus metrics on /metrics endpoint
    pub async fn run(&self) -> Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Metrics server listening on http://{}/metrics", addr);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream).await {
                            error!("Error handling metrics request: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
        }
    }
}

/// Handle incoming HTTP connection
async fn handle_connection(mut stream: tokio::net::TcpStream) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = [0; 1024];
    stream.read(&mut buffer).await?;

    let request = String::from_utf8_lossy(&buffer);

    // Simple HTTP router - only serve /metrics
    if request.starts_with("GET /metrics") {
        let encoder = TextEncoder::new();
        let metric_families = prometheus::gather();
        let mut buffer = Vec::new();

        encoder.encode(&metric_families, &mut buffer)?;

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\n\r\n{}",
            buffer.len(),
            String::from_utf8_lossy(&buffer)
        );

        stream.write_all(response.as_bytes()).await?;
    } else {
        // 404 for other paths
        let response = "HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\n\r\nNot Found";
        stream.write_all(response.as_bytes()).await?;
    }

    stream.flush().await?;
    Ok(())
}

/// Helper functions for updating metrics

/// Update current temperature metric
///
/// # Arguments
///
/// * `zone` - Zone name
/// * `temp_f` - Temperature in Fahrenheit
/// * `heat_on` - Whether heating is active
pub fn update_current_temp(zone: &str, temp_f: f32, heat_on: bool) {
    let heat_status = if heat_on { "1" } else { "0" };
    CURRENT_TEMP
        .with_label_values(&[zone, heat_status])
        .set(temp_f as f64);

    debug!("Updated current_temp metric for {}: {}°F", zone, temp_f);
}

/// Update target temperature metric
///
/// # Arguments
///
/// * `zone` - Zone name
/// * `temp_f` - Temperature in Fahrenheit
/// * `heat_on` - Whether heating is active
pub fn update_target_temp(zone: &str, temp_f: f32, heat_on: bool) {
    let heat_status = if heat_on { "1" } else { "0" };
    TARGET_TEMP
        .with_label_values(&[zone, heat_status])
        .set(temp_f as f64);

    debug!("Updated target_temp metric for {}: {}°F", zone, temp_f);
}

/// Update heat status metric
///
/// # Arguments
///
/// * `zone` - Zone name
/// * `heat_on` - Whether heating is active
pub fn update_heat_status(zone: &str, heat_on: bool) {
    let value = if heat_on { 1.0 } else { 0.0 };
    HEAT_STATUS.with_label_values(&[zone]).set(value);

    debug!("Updated heat_status metric for {}: {}", zone, value);
}

/// Update response time metric
///
/// # Arguments
///
/// * `zone` - Zone name
/// * `duration` - Duration of sensor read
pub fn update_response_time(zone: &str, duration: std::time::Duration) {
    RESPONSE_TIME
        .with_label_values(&[zone])
        .set(duration.as_secs_f64());

    debug!(
        "Updated response_time metric for {}: {:.3}s",
        zone,
        duration.as_secs_f64()
    );
}

/// Increment reset error count
///
/// # Arguments
///
/// * `zone` - Zone name
pub fn increment_reset_error_count(zone: &str) {
    RESET_ERROR_COUNT.with_label_values(&[zone]).inc();

    debug!("Incremented reset_error_count for {}", zone);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_initialization() {
        // Just verify metrics can be accessed
        CURRENT_TEMP.with_label_values(&["test", "0"]).set(70.0);
        TARGET_TEMP.with_label_values(&["test", "0"]).set(72.0);
        HEAT_STATUS.with_label_values(&["test"]).set(1.0);
        RESPONSE_TIME.with_label_values(&["test"]).set(0.5);
        RESET_ERROR_COUNT.with_label_values(&["test"]).inc();
    }

    #[test]
    fn test_update_current_temp() {
        update_current_temp("Zone1", 68.5, true);
        update_current_temp("Zone2", 72.0, false);
    }

    #[test]
    fn test_update_target_temp() {
        update_target_temp("Zone1", 70.0, true);
        update_target_temp("Zone2", 68.0, false);
    }

    #[test]
    fn test_update_heat_status() {
        update_heat_status("Zone1", true);
        update_heat_status("Zone2", false);
    }

    #[test]
    fn test_update_response_time() {
        let duration = std::time::Duration::from_millis(500);
        update_response_time("Zone1", duration);
    }

    #[test]
    fn test_increment_reset_error_count() {
        increment_reset_error_count("Zone1");
        increment_reset_error_count("Zone1");
    }
}
