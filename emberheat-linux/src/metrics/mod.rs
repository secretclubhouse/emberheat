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
//! - `matter_commissioned` - Matter device commissioned status (0=no, 1=yes)
//! - `matter_initialization_status` - Matter initialization status (0=failed, 1=success)
//!
//! ## Counters (cumulative)
//! - `reset_error_count` - Number of sensor power cycle attempts
//!
//! All metrics include labels: `room` (zone name), `heat_status` (0/1)
//!
//! # HTTP Endpoints
//!
//! - `/metrics` - Prometheus metrics
//! - `/matter/status` - Matter commissioning status (JSON)
//! - `/matter/qr_code` - Matter QR code (text)
//!
//! # Example
//!
//! ```no_run
//! use emberheat::metrics::MetricsServer;
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
    register_counter_vec, register_gauge, register_gauge_vec, CounterVec, Encoder, Gauge, GaugeVec,
    TextEncoder,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
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

    /// Matter commissioned status (0=no, 1=yes)
    pub static ref MATTER_COMMISSIONED: Gauge = register_gauge!(
        "matter_commissioned",
        "Matter device commissioned status (0=no, 1=yes)"
    )
    .unwrap();

    /// Matter initialization status (0=failed, 1=success)
    pub static ref MATTER_INIT_STATUS: Gauge = register_gauge!(
        "matter_initialization_status",
        "Matter initialization status (0=failed, 1=success)"
    )
    .unwrap();
}

/// Matter status information
#[derive(Debug, Clone, serde::Serialize)]
pub struct MatterStatus {
    pub commissioned: bool,
    pub initialized: bool,
    pub qr_code_available: bool,
    pub qr_code_file_path: Option<String>,
}

/// Metrics server for Prometheus export and Matter diagnostics
pub struct MetricsServer {
    port: u16,
    matter_status: Mutex<Option<MatterStatus>>,
    qr_code_data: Mutex<Option<String>>,
}

impl MetricsServer {
    /// Create a new metrics server
    ///
    /// # Arguments
    ///
    /// * `port` - Port to listen on (typically 8080)
    pub fn new(port: u16) -> Result<Self> {
        info!("Metrics server initialized on port {}", port);
        Ok(Self {
            port,
            matter_status: Mutex::new(None),
            qr_code_data: Mutex::new(None),
        })
    }

    /// Update Matter status
    pub async fn set_matter_status(&self, status: MatterStatus) {
        let mut matter_status = self.matter_status.lock().await;
        *matter_status = Some(status.clone());

        // Update Prometheus metrics
        MATTER_COMMISSIONED.set(if status.commissioned { 1.0 } else { 0.0 });
        MATTER_INIT_STATUS.set(if status.initialized { 1.0 } else { 0.0 });

        info!(
            "Matter status updated: initialized={}, commissioned={}",
            status.initialized, status.commissioned
        );
    }

    /// Set QR code data for display
    pub async fn set_qr_code(&self, qr_code: String) {
        let mut qr_data = self.qr_code_data.lock().await;
        *qr_data = Some(qr_code);
        debug!("QR code data updated");
    }

    /// Get Matter status
    async fn get_matter_status(&self) -> Option<MatterStatus> {
        self.matter_status.lock().await.clone()
    }

    /// Get QR code
    async fn get_qr_code(&self) -> Option<String> {
        self.qr_code_data.lock().await.clone()
    }

    /// Run the metrics server
    ///
    /// Serves Prometheus metrics on /metrics endpoint and Matter diagnostics on /matter/*
    pub async fn run(self: Arc<Self>) -> Result<()> {
        let addr = SocketAddr::from(([0, 0, 0, 0], self.port));
        let listener = TcpListener::bind(addr).await?;

        info!("Metrics server listening on http://{}/metrics", addr);
        info!(
            "Matter diagnostics available at http://{}/matter/status",
            addr
        );
        info!("Matter QR code available at http://{}/matter/qr_code", addr);

        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let server = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, server).await {
                            error!("Error handling request: {}", e);
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
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    server: Arc<MetricsServer>,
) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let mut buffer = [0; 2048];
    let _n = stream.read(&mut buffer).await?;

    let request = String::from_utf8_lossy(&buffer);

    // Extract request line
    let request_line = request.lines().next().unwrap_or("");

    // Simple HTTP router
    if request_line.starts_with("GET /metrics") {
        // Prometheus metrics endpoint
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
    } else if request_line.starts_with("GET /matter/status") {
        // Matter status endpoint (JSON)
        let status = server.get_matter_status().await;

        let json_body = if let Some(status) = status {
            serde_json::to_string_pretty(&status)?
        } else {
            serde_json::to_string_pretty(&serde_json::json!({
                "error": "Matter not initialized",
                "commissioned": false,
                "initialized": false
            }))?
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
            json_body.len(),
            json_body
        );

        stream.write_all(response.as_bytes()).await?;
    } else if request_line.starts_with("GET /matter/qr_code") {
        // Matter QR code endpoint (text)
        let qr_code = server.get_qr_code().await;

        let body = if let Some(qr_code) = qr_code {
            qr_code
        } else {
            "QR code not available. Matter may not be initialized yet.\n\n\
             Try checking the QR code file at:\n\
             - /var/lib/emberheat/matter_qr_code.txt\n\
             - ~/.config/emberheat/matter_qr_code.txt\n\
             - ./matter_qr_code.txt\n"
                .to_string()
        };

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );

        stream.write_all(response.as_bytes()).await?;
    } else if request_line.starts_with("GET /") && request_line.contains("HTTP") {
        // Root endpoint - show available endpoints
        let body = "EmberHeat Metrics and Diagnostics Server\n\n\
                    Available endpoints:\n\
                    - GET /metrics - Prometheus metrics\n\
                    - GET /matter/status - Matter commissioning status (JSON)\n\
                    - GET /matter/qr_code - Matter QR code (text)\n\n\
                    For help, see: https://github.com/yourusername/emberheat\n";

        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
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
///
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
