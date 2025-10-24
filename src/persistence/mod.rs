//! Redis-based state persistence
//!
//! This module provides persistent storage for thermostat state using Redis.
//! State is stored as JSON to allow for easy inspection and debugging.
//!
//! # State Structure
//!
//! Per-zone state stored in Redis:
//! ```json
//! {
//!     "relay_pin": 23,
//!     "temp_pin": 4,
//!     "temp_id": "00000ab12345",
//!     "target_temp": 21.1,
//!     "target_state": 1,
//!     "current_state": 1,
//!     "last_updated": "2024-01-15T10:30:00Z"
//! }
//! ```
//!
//! # Example
//!
//! ```no_run
//! use thermos_rust::persistence::{StateManager, ThermostatState};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let manager = StateManager::new("redis://localhost").await?;
//!
//!     let mut state = ThermostatState::default();
//!     state.target_temp = 22.0;
//!
//!     manager.save_state("Zone1", &state).await?;
//!     let loaded = manager.load_state("Zone1").await?;
//!
//!     Ok(())
//! }
//! ```

use anyhow::{Context, Result};
use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Persistence errors
#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("Redis connection error: {0}")]
    ConnectionError(String),

    #[error("Serialization error: {0}")]
    SerializationError(String),

    #[error("State not found: {0}")]
    NotFound(String),
}

/// Thermostat state for persistence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermostatState {
    /// GPIO relay pin
    pub relay_pin: u8,

    /// GPIO temperature sensor pin
    pub temp_pin: u8,

    /// Temperature sensor ID
    pub temp_id: String,

    /// Target temperature in Celsius
    pub target_temp: f32,

    /// Target heating state (0=off, 1=heat)
    pub target_state: u8,

    /// Current heating state (0=off, 1=heat)
    pub current_state: u8,

    /// Last update timestamp
    pub last_updated: chrono::DateTime<chrono::Utc>,
}

impl Default for ThermostatState {
    fn default() -> Self {
        Self {
            relay_pin: 0,
            temp_pin: 0,
            temp_id: String::new(),
            target_temp: 21.1, // 70°F
            target_state: 0,   // Off
            current_state: 0,  // Off
            last_updated: chrono::Utc::now(),
        }
    }
}

impl ThermostatState {
    /// Update timestamp to current time
    pub fn touch(&mut self) {
        self.last_updated = chrono::Utc::now();
    }
}

/// Redis-based state manager
pub struct StateManager {
    /// Redis connection manager
    connection: Arc<Mutex<ConnectionManager>>,
    /// Key prefix for all state keys
    key_prefix: String,
}

impl StateManager {
    /// Create a new state manager
    ///
    /// # Arguments
    ///
    /// * `redis_url` - Redis connection URL (e.g., "redis://localhost:6379")
    ///
    /// # Returns
    ///
    /// * `Result<Self>` - New state manager or error
    pub async fn new(redis_url: &str) -> Result<Self> {
        info!("Connecting to Redis at {}", redis_url);

        let client = redis::Client::open(redis_url)
            .context("Failed to create Redis client")?;

        let connection = ConnectionManager::new(client)
            .await
            .context("Failed to connect to Redis")?;

        info!("Successfully connected to Redis");

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            key_prefix: "thermostat:".to_string(),
        })
    }

    /// Create a new state manager with custom key prefix
    ///
    /// # Arguments
    ///
    /// * `redis_url` - Redis connection URL
    /// * `key_prefix` - Custom key prefix (e.g., "myapp:")
    pub async fn new_with_prefix(redis_url: &str, key_prefix: String) -> Result<Self> {
        let mut manager = Self::new(redis_url).await?;
        manager.key_prefix = key_prefix;
        Ok(manager)
    }

    /// Get full Redis key for a zone
    fn zone_key(&self, zone_name: &str) -> String {
        format!("{}{}", self.key_prefix, zone_name)
    }

    /// Save thermostat state
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    /// * `state` - State to save
    pub async fn save_state(&self, zone_name: &str, state: &ThermostatState) -> Result<()> {
        let key = self.zone_key(zone_name);

        let json = serde_json::to_string(state)
            .map_err(|e| PersistenceError::SerializationError(e.to_string()))?;

        let mut conn = self.connection.lock().await;

        conn.set(&key, json)
            .await
            .map_err(|e| PersistenceError::ConnectionError(e.to_string()))?;

        debug!("Saved state for zone: {}", zone_name);

        Ok(())
    }

    /// Load thermostat state
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    ///
    /// # Returns
    ///
    /// * `Result<Option<ThermostatState>>` - Loaded state or None if not found
    pub async fn load_state(&self, zone_name: &str) -> Result<Option<ThermostatState>> {
        let key = self.zone_key(zone_name);

        let mut conn = self.connection.lock().await;

        let json: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| PersistenceError::ConnectionError(e.to_string()))?;

        if let Some(json) = json {
            let state: ThermostatState = serde_json::from_str(&json)
                .map_err(|e| PersistenceError::SerializationError(e.to_string()))?;

            debug!("Loaded state for zone: {}", zone_name);

            Ok(Some(state))
        } else {
            debug!("No saved state found for zone: {}", zone_name);
            Ok(None)
        }
    }

    /// Delete state for a zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    pub async fn delete_state(&self, zone_name: &str) -> Result<()> {
        let key = self.zone_key(zone_name);

        let mut conn = self.connection.lock().await;

        conn.del(&key)
            .await
            .map_err(|e| PersistenceError::ConnectionError(e.to_string()))?;

        debug!("Deleted state for zone: {}", zone_name);

        Ok(())
    }

    /// List all zone names with saved state
    ///
    /// # Returns
    ///
    /// * `Result<Vec<String>>` - List of zone names
    pub async fn list_zones(&self) -> Result<Vec<String>> {
        let pattern = format!("{}*", self.key_prefix);

        let mut conn = self.connection.lock().await;

        let keys: Vec<String> = conn
            .keys(&pattern)
            .await
            .map_err(|e| PersistenceError::ConnectionError(e.to_string()))?;

        let zones: Vec<String> = keys
            .into_iter()
            .map(|k| k.strip_prefix(&self.key_prefix).unwrap_or(&k).to_string())
            .collect();

        Ok(zones)
    }

    /// Check if Redis connection is healthy
    pub async fn ping(&self) -> Result<()> {
        let mut conn = self.connection.lock().await;

        redis::cmd("PING")
            .query_async::<_, String>(&mut *conn)
            .await
            .map_err(|e| PersistenceError::ConnectionError(e.to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let state = ThermostatState::default();
        assert_eq!(state.target_temp, 21.1);
        assert_eq!(state.target_state, 0);
        assert_eq!(state.current_state, 0);
    }

    #[test]
    fn test_state_serialization() {
        let state = ThermostatState {
            relay_pin: 23,
            temp_pin: 4,
            temp_id: "00000ab12345".to_string(),
            target_temp: 22.0,
            target_state: 1,
            current_state: 1,
            last_updated: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: ThermostatState = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.relay_pin, state.relay_pin);
        assert_eq!(deserialized.temp_pin, state.temp_pin);
        assert_eq!(deserialized.temp_id, state.temp_id);
        assert_eq!(deserialized.target_temp, state.target_temp);
    }

    #[test]
    fn test_zone_key() {
        let manager = StateManager {
            connection: Arc::new(Mutex::new(
                // Placeholder - won't be used in this test
                tokio::runtime::Runtime::new()
                    .unwrap()
                    .block_on(async {
                        ConnectionManager::new(redis::Client::open("redis://localhost").unwrap())
                            .await
                            .unwrap()
                    }),
            )),
            key_prefix: "thermostat:".to_string(),
        };

        assert_eq!(manager.zone_key("Zone1"), "thermostat:Zone1");
        assert_eq!(manager.zone_key("Test"), "thermostat:Test");
    }

    #[test]
    fn test_state_touch() {
        let mut state = ThermostatState::default();
        let original_time = state.last_updated;

        std::thread::sleep(std::time::Duration::from_millis(10));
        state.touch();

        assert!(state.last_updated > original_time);
    }

    // Integration tests with actual Redis would go here
    // These require a running Redis instance
}
