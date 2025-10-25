//! Thermos-Rust - Multi-zone HomeKit thermostat system using Matter protocol
//!
//! This application provides a multi-zone heating control system for hydronic heating,
//! integrating with Apple HomeKit via the Matter protocol. It runs on Raspberry Pi hardware
//! and provides:
//!
//! - Multiple independent heating zones
//! - Matter/Thread protocol for HomeKit integration
//! - Hysteresis-based temperature control
//! - 1-Wire DS18B20 temperature sensor support
//! - HTTP fallback sensor support
//! - Redis state persistence
//! - Prometheus metrics export
//! - Graceful shutdown handling
//!
//! # Architecture
//!
//! The application is structured into several key modules:
//!
//! - `config`: Configuration loading and validation
//! - `devices`: Thermostat device implementation
//! - `gpio`: GPIO abstraction for relay control
//! - `sensors`: Temperature sensor interfaces (1-Wire, HTTP)
//! - `persistence`: Redis-based state management
//! - `metrics`: Prometheus metrics collection
//! - `matter`: Matter protocol integration
//!
//! # Usage
//!
//! ```bash
//! # Run with default config
//! thermos-rust
//!
//! # Run with custom config
//! thermos-rust --config /path/to/config.json
//!
//! # Enable debug logging
//! RUST_LOG=debug thermos-rust
//! ```

// The binary only runs on Linux due to hardware dependencies
// For testing on other platforms, use: cargo test --test mock_integration_tests

#[cfg(target_os = "linux")]
use anyhow::Result;
#[cfg(target_os = "linux")]
use futures::StreamExt;
#[cfg(target_os = "linux")]
use signal_hook::consts::signal::*;
#[cfg(target_os = "linux")]
use signal_hook_tokio::Signals;
#[cfg(target_os = "linux")]
use std::path::PathBuf;
#[cfg(target_os = "linux")]
use tokio::select;
#[cfg(target_os = "linux")]
use tracing::{error, info};
#[cfg(target_os = "linux")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// Allow dead code in modules when not building the full binary (non-Linux platforms)
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod config;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod devices;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod gpio;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod matter;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod metrics;
#[cfg(test)]
mod mocks;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod persistence;
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod sensors;

#[cfg(target_os = "linux")]
use config::Config;
#[cfg(target_os = "linux")]
use devices::ThermostatBridge;
#[cfg(target_os = "linux")]
use matter::MatterServer;
#[cfg(target_os = "linux")]
use metrics::MetricsServer;

/// Default configuration file path
#[cfg(target_os = "linux")]
const DEFAULT_CONFIG_PATH: &str = "./config/config.json";

/// Default Matter port (standard Matter port)
#[cfg(target_os = "linux")]
const DEFAULT_MATTER_PORT: u16 = 5540;

/// Default Prometheus metrics port
#[cfg(target_os = "linux")]
const DEFAULT_METRICS_PORT: u16 = 8080;

#[cfg(target_os = "linux")]
#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing/logging
    init_logging();

    info!("Starting Thermos-Rust multi-zone thermostat system");

    // Load configuration
    let config_path = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_CONFIG_PATH));

    info!("Loading configuration from: {:?}", config_path);
    let config = Config::load(&config_path).await?;
    info!("Loaded configuration for {} zones", config.zones.len());

    // Initialize Prometheus metrics server
    info!(
        "Starting Prometheus metrics server on port {}",
        DEFAULT_METRICS_PORT
    );
    let metrics_server = MetricsServer::new(DEFAULT_METRICS_PORT)?;
    let metrics_handle = tokio::spawn(async move {
        if let Err(e) = metrics_server.run().await {
            error!("Metrics server error: {}", e);
        }
    });

    // Initialize Matter server
    info!("Initializing Matter server on port {}", DEFAULT_MATTER_PORT);
    let mut matter_server = MatterServer::new(DEFAULT_MATTER_PORT).await?;

    // Create thermostat bridge with all zones
    info!("Creating thermostat bridge");
    let bridge = ThermostatBridge::new(config).await?;

    // Register thermostats with Matter server
    for (zone_name, thermostat) in bridge.thermostats() {
        info!("Registering zone: {}", zone_name);
        matter_server.add_thermostat(zone_name, thermostat).await?;
    }

    // Setup signal handlers for graceful shutdown
    let signals = Signals::new(&[SIGTERM, SIGINT])?;
    let signal_handle = signals.handle();

    info!("Thermostat bridge started successfully");
    info!("Matter server listening on port {}", DEFAULT_MATTER_PORT);
    info!(
        "Prometheus metrics available at http://localhost:{}/metrics",
        DEFAULT_METRICS_PORT
    );

    // Main event loop
    let matter_handle = tokio::spawn(async move {
        if let Err(e) = matter_server.run().await {
            error!("Matter server error: {}", e);
        }
    });

    let bridge_handle = tokio::spawn(async move {
        if let Err(e) = bridge.run().await {
            error!("Bridge error: {}", e);
        }
    });

    // Wait for shutdown signal
    let mut signal_stream = signals;
    select! {
        _ = signal_stream.next() => {
            info!("Received shutdown signal, initiating graceful shutdown");
        }
        _ = matter_handle => {
            info!("Matter server exited");
        }
        _ = bridge_handle => {
            info!("Bridge exited");
        }
    }

    // Cleanup
    signal_handle.close();
    metrics_handle.abort();

    info!("Shutdown complete");
    Ok(())
}

/// Initialize tracing subscriber for logging
///
/// Reads RUST_LOG environment variable for log level configuration.
/// Defaults to INFO level if not set.
#[cfg(target_os = "linux")]
fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "thermos_rust=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

// Stub main for non-Linux platforms
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("ERROR: thermos-rust binary requires Linux/Raspberry Pi");
    eprintln!("For cross-platform testing, use: cargo test --test mock_integration_tests");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "linux")]
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn test_default_config_path() {
        assert_eq!(DEFAULT_CONFIG_PATH, "./config/config.json");
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_default_ports() {
        assert_eq!(DEFAULT_MATTER_PORT, 5540);
        assert_eq!(DEFAULT_METRICS_PORT, 8080);
    }
}
