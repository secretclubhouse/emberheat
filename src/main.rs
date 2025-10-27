//! Thermos-Rust - Multi-zone HomeKit thermostat system using Matter protocol
//!
//! This application provides a multi-zone heating control system for hydronic heating,
//! integrating with Apple HomeKit via the Matter protocol.
//!
//! **Platform Requirements:** Linux (Raspberry Pi recommended)
//!
//! Features:
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

use anyhow::Result;
use futures::StreamExt;
use signal_hook::consts::signal::*;
use signal_hook_tokio::Signals;
use std::path::PathBuf;
use tokio::select;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod devices;
mod gpio;
mod matter;
mod metrics;
#[cfg(test)]
mod mocks;
mod persistence;
mod sensors;

use config::Config;
use devices::ThermostatBridge;
use matter::MatterServer;
use metrics::MetricsServer;

/// Configuration file search paths (in order of priority)
/// 1. Command line argument
/// 2. /etc/thermos-rust/config.json (system-wide, for binary installations)
/// 3. ~/.config/thermos-rust/config.json (user-specific)
/// 4. ./config/config.json (development/local directory)
const CONFIG_SEARCH_PATHS: &[&str] = &[
    "/etc/thermos-rust/config.json",
    "~/.config/thermos-rust/config.json",
    "./config/config.json",
];

/// Default Matter port (standard Matter port)
const DEFAULT_MATTER_PORT: u16 = 5540;

/// Default Prometheus metrics port
const DEFAULT_METRICS_PORT: u16 = 8080;

/// Find configuration file by searching standard locations
fn find_config_file() -> Option<PathBuf> {
    for path_str in CONFIG_SEARCH_PATHS {
        // Expand ~ to home directory
        let expanded = if let Some(stripped) = path_str.strip_prefix("~/") {
            if let Some(home) = std::env::var_os("HOME") {
                PathBuf::from(home).join(stripped)
            } else {
                continue;
            }
        } else {
            PathBuf::from(path_str)
        };

        if expanded.exists() {
            return Some(expanded);
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing/logging
    init_logging();

    info!("Starting Thermos-Rust multi-zone thermostat system");

    // Load configuration
    let config_path = if let Some(arg_path) = std::env::args().nth(1) {
        // Use command line argument if provided
        PathBuf::from(arg_path)
    } else if let Some(found_path) = find_config_file() {
        // Search standard locations
        info!("Found configuration at: {:?}", found_path);
        found_path
    } else {
        anyhow::bail!(
            "Configuration file not found. Searched:\n{}\n\nPlease create a config file or specify the path as an argument.",
            CONFIG_SEARCH_PATHS.join("\n")
        );
    };

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
        matter_server
            .add_thermostat(zone_name, thermostat.clone())
            .await?;
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
fn init_logging() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "thermos_rust=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_search_paths() {
        assert_eq!(CONFIG_SEARCH_PATHS.len(), 3);
        assert_eq!(CONFIG_SEARCH_PATHS[0], "/etc/thermos-rust/config.json");
        assert_eq!(CONFIG_SEARCH_PATHS[1], "~/.config/thermos-rust/config.json");
        assert_eq!(CONFIG_SEARCH_PATHS[2], "./config/config.json");
    }

    #[test]
    fn test_default_ports() {
        assert_eq!(DEFAULT_MATTER_PORT, 5540);
        assert_eq!(DEFAULT_METRICS_PORT, 8080);
    }
}
