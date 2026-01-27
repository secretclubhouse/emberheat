//! NVS (Non-Volatile Storage) implementation for ESP32-C6
//!
//! This module provides in-memory state storage for zone persistence.
//! The data is stored in a heapless map and persists across the lifetime
//! of the application (but not across power cycles - that requires actual
//! flash storage which can be added later).
//!
//! For production flash persistence, consider using esp-storage or
//! sequential-storage crates with proper Flash peripheral handling.

extern crate alloc;
use alloc::boxed::Box;

use core::fmt::Write as FmtWrite;
use heapless::{FnvIndexMap, String as HeaplessString};
use thermos_core::{PersistenceError, StateStorage, ThermostatState};

/// Maximum key length for zone storage keys
const MAX_KEY_LEN: usize = 32;

/// Maximum number of zones that can be stored
const MAX_STORED_ZONES: usize = 8;

/// Simplified state for storage
///
/// Only stores the essential fields needed to restore zone state
/// after a restart. Other fields can be reconstructed from config.
#[derive(Debug, Clone, Copy)]
struct StoredState {
    /// Target temperature in Celsius
    target_temp: f32,
    /// Target heating state (0=off, 1=heat)
    target_state: u8,
}

impl StoredState {
    /// Create a new stored state from a ThermostatState
    fn from_thermostat_state(state: &ThermostatState) -> Self {
        Self {
            target_temp: state.target_temp,
            target_state: state.target_state,
        }
    }

    /// Convert back to a full ThermostatState with defaults for non-persisted fields
    fn to_thermostat_state(&self) -> ThermostatState {
        ThermostatState {
            relay_pin: 0,
            temp_pin: 0,
            temp_id: HeaplessString::new(),
            target_temp: self.target_temp,
            target_state: self.target_state,
            current_state: 0, // Default to off on load
            last_updated: 0,
        }
    }
}

/// In-memory NVS storage implementation
///
/// Stores zone state in a heapless map for the duration of the application.
/// This provides state persistence across control loops but not across
/// power cycles. For full persistence, flash storage must be implemented.
pub struct NvsStorage {
    /// In-memory storage map: zone key -> stored state
    storage: FnvIndexMap<HeaplessString<MAX_KEY_LEN>, StoredState, MAX_STORED_ZONES>,
}

impl NvsStorage {
    /// Create a new NVS storage instance
    pub fn new() -> Self {
        log::info!(
            "NvsStorage: initialized in-memory storage (max {} zones)",
            MAX_STORED_ZONES
        );
        Self {
            storage: FnvIndexMap::new(),
        }
    }

    /// Generate a storage key for a zone name
    ///
    /// Creates a key in the format "zone_{name}" to namespace zone data.
    fn make_key(zone_name: &str) -> Result<HeaplessString<MAX_KEY_LEN>, PersistenceError> {
        let mut key = HeaplessString::new();

        // Write "zone_" prefix
        if write!(key, "zone_{}", zone_name).is_err() {
            let mut err_msg = HeaplessString::<128>::new();
            let _ = write!(err_msg, "Zone name too long: {}", zone_name);
            return Err(PersistenceError::InvalidKey(err_msg));
        }

        Ok(key)
    }
}

impl Default for NvsStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStorage for NvsStorage {
    async fn save_state(
        &mut self,
        zone_name: &str,
        state: &ThermostatState,
    ) -> Result<(), PersistenceError> {
        let key = Self::make_key(zone_name)?;
        let stored = StoredState::from_thermostat_state(state);

        // Insert or update the state in the map
        if self.storage.contains_key(&key) {
            // Update existing entry
            if let Some(entry) = self.storage.get_mut(&key) {
                *entry = stored;
            }
            log::debug!(
                "NvsStorage: updated state for '{}' (target={:.1}C, state={})",
                zone_name,
                stored.target_temp,
                stored.target_state
            );
        } else {
            // Insert new entry
            if self.storage.insert(key.clone(), stored).is_err() {
                let mut err_msg = HeaplessString::<128>::new();
                let _ = write!(err_msg, "Storage full, cannot add zone: {}", zone_name);
                return Err(PersistenceError::StorageError(err_msg));
            }
            log::info!(
                "NvsStorage: saved new state for '{}' (target={:.1}C, state={})",
                zone_name,
                stored.target_temp,
                stored.target_state
            );
        }

        Ok(())
    }

    async fn load_state(
        &mut self,
        zone_name: &str,
    ) -> Result<Option<ThermostatState>, PersistenceError> {
        let key = Self::make_key(zone_name)?;

        match self.storage.get(&key) {
            Some(stored) => {
                let state = stored.to_thermostat_state();
                log::debug!(
                    "NvsStorage: loaded state for '{}' (target={:.1}C, state={})",
                    zone_name,
                    state.target_temp,
                    state.target_state
                );
                Ok(Some(state))
            }
            None => {
                log::debug!("NvsStorage: no saved state for '{}'", zone_name);
                Ok(None)
            }
        }
    }

    async fn delete_state(&mut self, zone_name: &str) -> Result<(), PersistenceError> {
        let key = Self::make_key(zone_name)?;

        if self.storage.remove(&key).is_some() {
            log::info!("NvsStorage: deleted state for '{}'", zone_name);
        } else {
            log::debug!("NvsStorage: no state to delete for '{}'", zone_name);
        }

        Ok(())
    }

    async fn ping(&mut self) -> Result<(), PersistenceError> {
        // In-memory storage is always available
        log::debug!("NvsStorage: ping OK ({} zones stored)", self.storage.len());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_creation() {
        let storage = NvsStorage::new();
        assert_eq!(storage.storage.len(), 0);
    }

    #[test]
    fn test_storage_default() {
        let storage = NvsStorage::default();
        assert_eq!(storage.storage.len(), 0);
    }

    #[test]
    fn test_make_key() {
        let key = NvsStorage::make_key("Zone1").unwrap();
        assert_eq!(key.as_str(), "zone_Zone1");
    }

    #[test]
    fn test_make_key_long_name() {
        // A name that would make the key exceed MAX_KEY_LEN
        let long_name = "ThisIsAVeryLongZoneNameThatExceeds";
        let result = NvsStorage::make_key(long_name);
        assert!(result.is_err());
    }

    #[test]
    fn test_stored_state_conversion() {
        let original = ThermostatState {
            relay_pin: 4,
            temp_pin: 5,
            temp_id: HeaplessString::new(),
            target_temp: 22.5,
            target_state: 1,
            current_state: 1,
            last_updated: 12345,
        };

        let stored = StoredState::from_thermostat_state(&original);
        assert_eq!(stored.target_temp, 22.5);
        assert_eq!(stored.target_state, 1);

        let restored = stored.to_thermostat_state();
        assert_eq!(restored.target_temp, 22.5);
        assert_eq!(restored.target_state, 1);
        // Non-persisted fields should be defaults
        assert_eq!(restored.relay_pin, 0);
        assert_eq!(restored.current_state, 0);
    }

    #[tokio::test]
    async fn test_save_and_load() {
        let mut storage = NvsStorage::new();
        let state = ThermostatState {
            relay_pin: 4,
            temp_pin: 5,
            temp_id: HeaplessString::new(),
            target_temp: 23.0,
            target_state: 1,
            current_state: 1,
            last_updated: 0,
        };

        // Save state
        storage.save_state("Zone1", &state).await.unwrap();

        // Load it back
        let loaded = storage.load_state("Zone1").await.unwrap();
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.target_temp, 23.0);
        assert_eq!(loaded.target_state, 1);
    }

    #[tokio::test]
    async fn test_load_nonexistent() {
        let mut storage = NvsStorage::new();
        let loaded = storage.load_state("NonExistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_update_existing() {
        let mut storage = NvsStorage::new();

        // Save initial state
        let state1 = ThermostatState {
            target_temp: 20.0,
            target_state: 0,
            ..Default::default()
        };
        storage.save_state("Zone1", &state1).await.unwrap();

        // Update with new state
        let state2 = ThermostatState {
            target_temp: 25.0,
            target_state: 1,
            ..Default::default()
        };
        storage.save_state("Zone1", &state2).await.unwrap();

        // Verify updated values
        let loaded = storage.load_state("Zone1").await.unwrap().unwrap();
        assert_eq!(loaded.target_temp, 25.0);
        assert_eq!(loaded.target_state, 1);

        // Should still be just one entry
        assert_eq!(storage.storage.len(), 1);
    }

    #[tokio::test]
    async fn test_delete() {
        let mut storage = NvsStorage::new();
        let state = ThermostatState::default();

        storage.save_state("Zone1", &state).await.unwrap();
        assert_eq!(storage.storage.len(), 1);

        storage.delete_state("Zone1").await.unwrap();
        assert_eq!(storage.storage.len(), 0);

        let loaded = storage.load_state("Zone1").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_delete_nonexistent() {
        let mut storage = NvsStorage::new();
        // Should not error when deleting non-existent zone
        let result = storage.delete_state("NonExistent").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ping() {
        let mut storage = NvsStorage::new();
        let result = storage.ping().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_multiple_zones() {
        let mut storage = NvsStorage::new();

        // Save multiple zones
        for i in 1..=5 {
            let state = ThermostatState {
                target_temp: 20.0 + i as f32,
                target_state: (i % 2) as u8,
                ..Default::default()
            };
            let zone_name = alloc::format!("Zone{}", i);
            storage.save_state(&zone_name, &state).await.unwrap();
        }

        assert_eq!(storage.storage.len(), 5);

        // Verify each zone
        for i in 1..=5 {
            let zone_name = alloc::format!("Zone{}", i);
            let loaded = storage.load_state(&zone_name).await.unwrap().unwrap();
            assert_eq!(loaded.target_temp, 20.0 + i as f32);
            assert_eq!(loaded.target_state, (i % 2) as u8);
        }
    }

    #[tokio::test]
    async fn test_trait_object() {
        // Verify StateStorage trait is properly implemented
        let mut storage: Box<dyn StateStorage> = Box::new(NvsStorage::new());
        let state = ThermostatState::default();

        storage.save_state("test", &state).await.unwrap();
        let loaded = storage.load_state("test").await.unwrap();
        assert!(loaded.is_some());

        storage.delete_state("test").await.unwrap();
        let loaded = storage.load_state("test").await.unwrap();
        assert!(loaded.is_none());

        storage.ping().await.unwrap();
    }
}
