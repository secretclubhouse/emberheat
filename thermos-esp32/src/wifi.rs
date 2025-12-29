//! WiFi manager for ESP32-C6
//!
//! NOTE: WiFi support temporarily disabled due to dependency version conflicts.
//! This will be re-enabled in Week 3 with MQTT integration.
//!
//! For MVP testing, the thermostat can operate without WiFi using local sensors
//! and NVS storage.

use core::str::FromStr;
use log::info;

/// WiFi credentials and configuration (placeholder)
#[derive(Clone)]
pub struct WifiConfig {
    pub ssid: heapless::String<32>,
    pub password: heapless::String<64>,
}

impl WifiConfig {
    /// Create a new WiFi configuration
    pub fn new(ssid: &str, password: &str) -> Result<Self, &'static str> {
        let ssid = heapless::String::from_str(ssid).map_err(|_| "SSID too long (max 32 chars)")?;
        let password =
            heapless::String::from_str(password).map_err(|_| "Password too long (max 64 chars)")?;

        Ok(Self { ssid, password })
    }
}

/// WiFi manager placeholder
pub struct WifiManager {
    config: WifiConfig,
}

impl WifiManager {
    /// Create a new WiFi manager
    pub fn new(config: WifiConfig) -> Self {
        info!("WiFi manager created (WiFi support disabled for MVP)");
        Self { config }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wifi_config_creation() {
        let config = WifiConfig::new("MyNetwork", "password123").unwrap();
        assert_eq!(config.ssid.as_str(), "MyNetwork");
        assert_eq!(config.password.as_str(), "password123");
    }
}
