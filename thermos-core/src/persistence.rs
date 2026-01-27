//! State persistence trait
//!
//! This module provides the storage abstraction for persisting thermostat state.
//! Different backends can implement this trait (Redis for Linux, NVS for ESP32, etc.).

// Allow async fn in traits - this crate is used internally and Send bounds are handled
// at the implementation level by each platform
#![allow(async_fn_in_trait)]

use crate::types::ThermostatState;
use core::fmt;

/// Persistence errors
#[derive(Debug)]
pub enum PersistenceError {
    ConnectionError(heapless::String<128>),
    SerializationError(heapless::String<128>),
    DeserializationError(heapless::String<128>),
    NotFound(heapless::String<128>),
    StorageError(heapless::String<128>),
    InvalidKey(heapless::String<128>),
}

impl fmt::Display for PersistenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistenceError::ConnectionError(msg) => write!(f, "Connection error: {}", msg),
            PersistenceError::SerializationError(msg) => write!(f, "Serialization error: {}", msg),
            PersistenceError::DeserializationError(msg) => {
                write!(f, "Deserialization error: {}", msg)
            }
            PersistenceError::NotFound(msg) => write!(f, "State not found: {}", msg),
            PersistenceError::StorageError(msg) => write!(f, "Storage error: {}", msg),
            PersistenceError::InvalidKey(msg) => write!(f, "Invalid key: {}", msg),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for PersistenceError {}

/// State storage trait
///
/// This trait defines the interface for persisting and retrieving thermostat state.
/// Implementations can use different storage backends (Redis, NVS flash, files, etc.).
///
/// Note: Uses native async fn in trait (requires Rust 1.75+)
pub trait StateStorage: Send + Sync {
    /// Save thermostat state for a zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    /// * `state` - State to save
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success or error
    async fn save_state(
        &mut self,
        zone_name: &str,
        state: &ThermostatState,
    ) -> Result<(), PersistenceError>;

    /// Load thermostat state for a zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    ///
    /// # Returns
    ///
    /// * `Result<Option<ThermostatState>>` - Loaded state, None if not found, or error
    async fn load_state(
        &mut self,
        zone_name: &str,
    ) -> Result<Option<ThermostatState>, PersistenceError>;

    /// Delete state for a zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Name of the zone
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success or error
    async fn delete_state(&mut self, zone_name: &str) -> Result<(), PersistenceError>;

    /// List all zone names with saved state
    ///
    /// # Returns
    ///
    /// * `Result<Vec<String>>` - List of zone names or error (std only)
    #[cfg(feature = "std")]
    async fn list_zones(&mut self) -> Result<Vec<String>, PersistenceError>;

    /// Check if storage connection is healthy
    ///
    /// # Returns
    ///
    /// * `Result<()>` - Success if healthy, error otherwise
    async fn ping(&mut self) -> Result<(), PersistenceError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    // Mock storage implementation for testing
    struct MockStorage {
        data: Arc<Mutex<HashMap<String, ThermostatState>>>,
    }

    impl MockStorage {
        fn new() -> Self {
            Self {
                data: Arc::new(Mutex::new(HashMap::new())),
            }
        }
    }

    impl StateStorage for MockStorage {
        async fn save_state(
            &mut self,
            zone_name: &str,
            state: &ThermostatState,
        ) -> Result<(), PersistenceError> {
            let mut data = self.data.lock().await;
            data.insert(zone_name.to_string(), state.clone());
            Ok(())
        }

        async fn load_state(
            &mut self,
            zone_name: &str,
        ) -> Result<Option<ThermostatState>, PersistenceError> {
            let data = self.data.lock().await;
            Ok(data.get(zone_name).cloned())
        }

        async fn delete_state(&mut self, zone_name: &str) -> Result<(), PersistenceError> {
            let mut data = self.data.lock().await;
            data.remove(zone_name);
            Ok(())
        }

        async fn list_zones(&mut self) -> Result<Vec<String>, PersistenceError> {
            let data = self.data.lock().await;
            Ok(data.keys().cloned().collect())
        }

        async fn ping(&mut self) -> Result<(), PersistenceError> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_mock_storage_save_and_load() {
        let mut storage = MockStorage::new();
        let state = ThermostatState::default();

        storage.save_state("Zone1", &state).await.unwrap();
        let loaded = storage.load_state("Zone1").await.unwrap();

        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.target_temp, state.target_temp);
    }

    #[tokio::test]
    async fn test_mock_storage_load_nonexistent() {
        let mut storage = MockStorage::new();
        let loaded = storage.load_state("NonExistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_mock_storage_delete() {
        let mut storage = MockStorage::new();
        let state = ThermostatState::default();

        storage.save_state("Zone1", &state).await.unwrap();
        storage.delete_state("Zone1").await.unwrap();

        let loaded = storage.load_state("Zone1").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_mock_storage_list_zones() {
        let mut storage = MockStorage::new();
        let state = ThermostatState::default();

        storage.save_state("Zone1", &state).await.unwrap();
        storage.save_state("Zone2", &state).await.unwrap();

        let zones = storage.list_zones().await.unwrap();
        assert_eq!(zones.len(), 2);
        assert!(zones.contains(&"Zone1".to_string()));
        assert!(zones.contains(&"Zone2".to_string()));
    }

    #[tokio::test]
    async fn test_mock_storage_ping() {
        let mut storage = MockStorage::new();
        assert!(storage.ping().await.is_ok());
    }
}
