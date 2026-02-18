//! Configuration management for the thermostat system
//!
//! This module handles loading, parsing, and validating configuration from JSON files.
//! Configuration includes zone definitions with GPIO pins, sensor IDs, optional
//! HTTP fallback sensors, and Redis server settings.
//!
//! # Configuration Format
//!
//! ```json
//! {
//!     "redis_url": "redis://192.168.1.100:6379",
//!     "Zone1": {
//!         "relay_pin": 23,
//!         "temp_pin": 4,
//!         "temp_id": "00000ab12345",
//!         "extra_sensor": "http://example.com/temp"
//!     },
//!     "Zone2": {
//!         "relay_pin": 24,
//!         "temp_pin": 4,
//!         "temp_id": "28-00000ab12346"
//!     }
//! }
//! ```
//!
//! # Fields
//!
//! - `redis_url` (optional): Redis server URL. Defaults to "redis://localhost:6379" if not specified.
//! - Zone definitions: Each zone is defined at the top level with a unique name.
//!   Multiple zones can share the same `temp_pin` (1-wire bus protocol allows multiple sensors on one pin).
//!
//! # Example
//!
//! ```no_run
//! use std::path::Path;
//! use emberheat::config::Config;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let config = Config::load(Path::new("config.json")).await?;
//!     println!("Loaded {} zones", config.zones.len());
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tokio::fs;

// Import ZoneConfig from emberheat-core
pub use emberheat_core::ZoneConfig;

/// Main configuration structure
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    /// Redis server URL (defaults to "redis://localhost:6379" if not specified)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redis_url: Option<String>,

    /// Map of zone names to zone configurations
    pub zones: HashMap<String, ZoneConfig>,
}

// Custom deserializer for Config to handle underscore-prefixed fields (comments)
impl<'de> Deserialize<'de> for Config {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawConfig {
            #[serde(skip_serializing_if = "Option::is_none")]
            redis_url: Option<String>,
            #[serde(flatten)]
            other: HashMap<String, serde_json::Value>,
        }

        let raw = RawConfig::deserialize(deserializer)?;
        let mut zones = HashMap::new();

        // Process non-underscore fields as zones
        for (key, value) in raw.other {
            // Skip fields starting with underscore (these are comments)
            if key.starts_with('_') {
                continue;
            }

            // Parse the value as a ZoneConfig
            let zone_config: ZoneConfig = serde_json::from_value(value).map_err(|e| {
                serde::de::Error::custom(format!("Failed to parse zone '{}': {}", key, e))
            })?;
            zones.insert(key, zone_config);
        }

        Ok(Config {
            redis_url: raw.redis_url,
            zones,
        })
    }
}

// Note: ZoneConfig is now imported from emberheat-core
// Custom deserialization for Config handles underscore-prefixed comment fields

impl Config {
    /// Load configuration from a JSON file
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the configuration file
    ///
    /// # Returns
    ///
    /// * `Result<Config>` - Parsed configuration or error
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - File cannot be read
    /// - JSON parsing fails
    /// - Configuration validation fails
    pub async fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .with_context(|| format!("Failed to read config file: {:?}", path))?;

        let config: Config = serde_json::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {:?}", path))?;

        config.validate()?;

        Ok(config)
    }

    /// Validate configuration
    ///
    /// Ensures that:
    /// - At least one zone is defined
    /// - GPIO pins are valid (0-27 for Raspberry Pi)
    /// - Temperature IDs are non-empty
    /// - No duplicate relay pins across zones
    /// - Temperature pins can be shared (1-wire bus protocol)
    fn validate(&self) -> Result<()> {
        if self.zones.is_empty() {
            anyhow::bail!("Configuration must contain at least one zone");
        }

        let mut used_relay_pins = Vec::new();

        for (zone_name, zone) in &self.zones {
            // Validate pin numbers (Raspberry Pi has GPIO 0-27)
            if zone.relay_pin > 27 {
                anyhow::bail!(
                    "Zone '{}': relay_pin {} is invalid (must be 0-27)",
                    zone_name,
                    zone.relay_pin
                );
            }

            if zone.temp_pin > 27 {
                anyhow::bail!(
                    "Zone '{}': temp_pin {} is invalid (must be 0-27)",
                    zone_name,
                    zone.temp_pin
                );
            }

            // Check for duplicate relay pins
            // Note: temp_pins can be shared since 1-wire is a bus protocol
            if used_relay_pins.contains(&zone.relay_pin) {
                anyhow::bail!(
                    "Zone '{}': relay_pin {} is already in use",
                    zone_name,
                    zone.relay_pin
                );
            }

            // Validate temp_id is non-empty
            if zone.temp_id.is_empty() {
                anyhow::bail!("Zone '{}': temp_id cannot be empty", zone_name);
            }

            // Validate extra_sensor URL if provided
            if let Some(url) = &zone.extra_sensor {
                if !url.starts_with("http://") && !url.starts_with("https://") {
                    anyhow::bail!(
                        "Zone '{}': extra_sensor must be a valid HTTP/HTTPS URL",
                        zone_name
                    );
                }
            }

            used_relay_pins.push(zone.relay_pin);
        }

        Ok(())
    }

    /// Get configuration for a specific zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    ///
    /// # Returns
    ///
    /// * `Option<&ZoneConfig>` - Zone configuration if it exists
    #[allow(dead_code)]
    pub fn get_zone(&self, zone_name: &str) -> Option<&ZoneConfig> {
        self.zones.get(zone_name)
    }

    /// Get list of all zone names
    #[allow(dead_code)]
    pub fn zone_names(&self) -> Vec<String> {
        self.zones.keys().cloned().collect()
    }

    /// Get Redis URL with fallback to default
    ///
    /// # Returns
    ///
    /// * `&str` - Redis URL from config or default "redis://localhost:6379"
    pub fn redis_url(&self) -> &str {
        self.redis_url
            .as_deref()
            .unwrap_or("redis://localhost:6379")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_valid_config() {
        let config_json = r#"
        {
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "00000ab12345"
            },
            "Zone2": {
                "relay_pin": 24,
                "temp_pin": 5,
                "temp_id": "00000ab12346",
                "extra_sensor": "http://example.com/temp"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let config = Config::load(temp_file.path()).await.unwrap();

        assert_eq!(config.zones.len(), 2);
        assert!(config.zones.contains_key("Zone1"));
        assert!(config.zones.contains_key("Zone2"));

        let zone1 = config.get_zone("Zone1").unwrap();
        assert_eq!(zone1.relay_pin, 23);
        assert_eq!(zone1.temp_pin, 4);
        assert_eq!(zone1.temp_id, "00000ab12345");
        assert!(zone1.extra_sensor.is_none());

        let zone2 = config.get_zone("Zone2").unwrap();
        assert_eq!(
            zone2.extra_sensor.as_deref(),
            Some("http://example.com/temp")
        );
    }

    #[tokio::test]
    async fn test_empty_config_fails() {
        let config_json = "{}";

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_pin_fails() {
        let config_json = r#"
        {
            "Zone1": {
                "relay_pin": 99,
                "temp_pin": 4,
                "temp_id": "00000ab12345"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_duplicate_pins_fails() {
        let config_json = r#"
        {
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "00000ab12345"
            },
            "Zone2": {
                "relay_pin": 23,
                "temp_pin": 5,
                "temp_id": "00000ab12346"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_invalid_url_fails() {
        let config_json = r#"
        {
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "00000ab12345",
                "extra_sensor": "not-a-url"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_shared_temp_pin_allowed() {
        // Multiple zones can share the same temp_pin (1-wire bus)
        let config_json = r#"
        {
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "28-00000ab12345"
            },
            "Zone2": {
                "relay_pin": 24,
                "temp_pin": 4,
                "temp_id": "28-00000ab12346"
            },
            "Zone3": {
                "relay_pin": 25,
                "temp_pin": 4,
                "temp_id": "28-00000ab12347"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(
            result.is_ok(),
            "Shared temp_pin should be allowed for 1-wire bus"
        );

        let config = result.unwrap();
        assert_eq!(config.zones.len(), 3);

        // Verify all zones share the same temp_pin
        assert_eq!(config.zones.get("Zone1").unwrap().temp_pin, 4);
        assert_eq!(config.zones.get("Zone2").unwrap().temp_pin, 4);
        assert_eq!(config.zones.get("Zone3").unwrap().temp_pin, 4);
    }

    #[test]
    fn test_zone_names() {
        let config = Config {
            redis_url: None,
            zones: vec![
                (
                    "Zone1".to_string(),
                    ZoneConfig {
                        relay_pin: 23,
                        temp_pin: 4,
                        temp_id: "test1".to_string(),
                        extra_sensor: None,
                    },
                ),
                (
                    "Zone2".to_string(),
                    ZoneConfig {
                        relay_pin: 24,
                        temp_pin: 5,
                        temp_id: "test2".to_string(),
                        extra_sensor: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        let names = config.zone_names();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"Zone1".to_string()));
        assert!(names.contains(&"Zone2".to_string()));
    }

    #[test]
    fn test_redis_url_default() {
        let config = Config {
            redis_url: None,
            zones: HashMap::new(),
        };

        assert_eq!(config.redis_url(), "redis://localhost:6379");
    }

    #[test]
    fn test_redis_url_custom() {
        let config = Config {
            redis_url: Some("redis://192.168.1.100:6379".to_string()),
            zones: HashMap::new(),
        };

        assert_eq!(config.redis_url(), "redis://192.168.1.100:6379");
    }

    #[tokio::test]
    async fn test_load_config_with_redis_url() {
        let config_json = r#"
        {
            "redis_url": "redis://myserver:6379",
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "00000ab12345"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(result.is_ok());

        let config = result.unwrap();
        assert_eq!(config.redis_url(), "redis://myserver:6379");
        assert_eq!(config.zones.len(), 1);
    }

    #[tokio::test]
    async fn test_load_config_without_redis_url() {
        let config_json = r#"
        {
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "00000ab12345"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(result.is_ok());

        let config = result.unwrap();
        // Should use default when not specified
        assert_eq!(config.redis_url(), "redis://localhost:6379");
        assert_eq!(config.zones.len(), 1);
    }

    #[tokio::test]
    async fn test_load_config_with_comments() {
        // Test that underscore-prefixed fields (comments) are properly ignored
        let config_json = r#"
        {
            "_comment": "This is a comment field that should be ignored",
            "redis_url": "redis://localhost:6379",
            "_note": "Another comment at top level",
            "Zone1": {
                "relay_pin": 23,
                "temp_pin": 4,
                "temp_id": "00000ab12345",
                "_description": "Living room zone"
            },
            "Zone2": {
                "relay_pin": 24,
                "temp_pin": 4,
                "temp_id": "00000ab12346",
                "_note": "Bedroom zone"
            }
        }
        "#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(config_json.as_bytes()).unwrap();

        let result = Config::load(temp_file.path()).await;
        assert!(
            result.is_ok(),
            "Config with underscore fields should load successfully"
        );

        let config = result.unwrap();
        assert_eq!(config.redis_url(), "redis://localhost:6379");
        assert_eq!(config.zones.len(), 2);
        assert!(config.zones.contains_key("Zone1"));
        assert!(config.zones.contains_key("Zone2"));

        // Make sure the underscore fields are NOT parsed as zones
        assert!(!config.zones.contains_key("_comment"));
        assert!(!config.zones.contains_key("_note"));
    }
}
