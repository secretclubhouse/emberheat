//! Mock state manager for testing

use crate::persistence::{PersistenceError, ThermostatState};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Mock state manager that uses in-memory storage
///
/// This mock allows tests to run without requiring a Redis instance.
/// State is stored in a HashMap and persists for the lifetime of the mock.
#[derive(Clone)]
pub struct MockStateManager {
    /// In-memory state storage
    states: Arc<Mutex<HashMap<String, ThermostatState>>>,
    /// Track save operations
    save_count: Arc<Mutex<usize>>,
    /// Track load operations
    load_count: Arc<Mutex<usize>>,
    /// Simulate connection failures
    should_fail: Arc<Mutex<bool>>,
}

impl MockStateManager {
    /// Create a new mock state manager
    pub fn new() -> Self {
        Self {
            states: Arc::new(Mutex::new(HashMap::new())),
            save_count: Arc::new(Mutex::new(0)),
            load_count: Arc::new(Mutex::new(0)),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    /// Create a new mock with pre-populated states
    pub fn with_states(states: HashMap<String, ThermostatState>) -> Self {
        Self {
            states: Arc::new(Mutex::new(states)),
            save_count: Arc::new(Mutex::new(0)),
            load_count: Arc::new(Mutex::new(0)),
            should_fail: Arc::new(Mutex::new(false)),
        }
    }

    /// Save thermostat state
    pub async fn save_state(&self, zone_name: &str, state: &ThermostatState) -> Result<()> {
        // Increment save count
        {
            let mut count = self.save_count.lock().unwrap();
            *count += 1;
        }

        // Check if should fail
        if *self.should_fail.lock().unwrap() {
            return Err(PersistenceError::ConnectionError("Mock failure".to_string()).into());
        }

        let mut states = self.states.lock().unwrap();
        states.insert(zone_name.to_string(), state.clone());

        Ok(())
    }

    /// Load thermostat state
    pub async fn load_state(&self, zone_name: &str) -> Result<Option<ThermostatState>> {
        // Increment load count
        {
            let mut count = self.load_count.lock().unwrap();
            *count += 1;
        }

        // Check if should fail
        if *self.should_fail.lock().unwrap() {
            return Err(PersistenceError::ConnectionError("Mock failure".to_string()).into());
        }

        let states = self.states.lock().unwrap();
        Ok(states.get(zone_name).cloned())
    }

    /// Delete state for a zone
    pub async fn delete_state(&self, zone_name: &str) -> Result<()> {
        // Check if should fail
        if *self.should_fail.lock().unwrap() {
            return Err(PersistenceError::ConnectionError("Mock failure".to_string()).into());
        }

        let mut states = self.states.lock().unwrap();
        states.remove(zone_name);

        Ok(())
    }

    /// List all zone names
    pub async fn list_zones(&self) -> Result<Vec<String>> {
        // Check if should fail
        if *self.should_fail.lock().unwrap() {
            return Err(PersistenceError::ConnectionError("Mock failure".to_string()).into());
        }

        let states = self.states.lock().unwrap();
        Ok(states.keys().cloned().collect())
    }

    /// Check connection health
    pub async fn ping(&self) -> Result<()> {
        if *self.should_fail.lock().unwrap() {
            return Err(PersistenceError::ConnectionError("Mock failure".to_string()).into());
        }

        Ok(())
    }

    /// Get the number of save operations
    pub fn get_save_count(&self) -> usize {
        *self.save_count.lock().unwrap()
    }

    /// Get the number of load operations
    pub fn get_load_count(&self) -> usize {
        *self.load_count.lock().unwrap()
    }

    /// Reset operation counters
    #[allow(dead_code)]
    pub fn reset_counters(&self) {
        let mut save = self.save_count.lock().unwrap();
        *save = 0;

        let mut load = self.load_count.lock().unwrap();
        *load = 0;
    }

    /// Set whether operations should fail
    pub fn set_should_fail(&self, should_fail: bool) {
        let mut fail = self.should_fail.lock().unwrap();
        *fail = should_fail;
    }

    /// Clear all stored states
    #[allow(dead_code)]
    pub fn clear(&self) {
        let mut states = self.states.lock().unwrap();
        states.clear();
    }

    /// Get all stored states (for testing)
    #[allow(dead_code)]
    pub fn get_all_states(&self) -> HashMap<String, ThermostatState> {
        self.states.lock().unwrap().clone()
    }
}

impl Default for MockStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_state_save_and_load() {
        let manager = MockStateManager::new();

        let state = ThermostatState {
            relay_pin: 23,
            temp_pin: 4,
            temp_id: "test-sensor".to_string(),
            target_temp: 22.0,
            target_state: 1,
            current_state: 0,
            last_updated: chrono::Utc::now(),
        };

        // Save state
        manager.save_state("Zone1", &state).await.unwrap();
        assert_eq!(manager.get_save_count(), 1);

        // Load state
        let loaded = manager.load_state("Zone1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().target_temp, 22.0);
        assert_eq!(manager.get_load_count(), 1);
    }

    #[tokio::test]
    async fn test_mock_state_not_found() {
        let manager = MockStateManager::new();

        let loaded = manager.load_state("NonExistent").await.unwrap();
        assert!(loaded.is_none());
    }

    #[tokio::test]
    async fn test_mock_state_delete() {
        let manager = MockStateManager::new();

        let state = ThermostatState::default();

        // Save and verify
        manager.save_state("Zone1", &state).await.unwrap();
        assert!(manager.load_state("Zone1").await.unwrap().is_some());

        // Delete and verify
        manager.delete_state("Zone1").await.unwrap();
        assert!(manager.load_state("Zone1").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_mock_state_list_zones() {
        let manager = MockStateManager::new();

        let state = ThermostatState::default();

        manager.save_state("Zone1", &state).await.unwrap();
        manager.save_state("Zone2", &state).await.unwrap();

        let zones = manager.list_zones().await.unwrap();
        assert_eq!(zones.len(), 2);
        assert!(zones.contains(&"Zone1".to_string()));
        assert!(zones.contains(&"Zone2".to_string()));
    }

    #[tokio::test]
    async fn test_mock_state_ping() {
        let manager = MockStateManager::new();
        assert!(manager.ping().await.is_ok());
    }

    #[tokio::test]
    async fn test_mock_state_failure_simulation() {
        let manager = MockStateManager::new();

        // Enable failure mode
        manager.set_should_fail(true);

        let state = ThermostatState::default();

        // All operations should fail
        assert!(manager.save_state("Zone1", &state).await.is_err());
        assert!(manager.load_state("Zone1").await.is_err());
        assert!(manager.ping().await.is_err());

        // Disable failure mode
        manager.set_should_fail(false);

        // Operations should succeed again
        assert!(manager.save_state("Zone1", &state).await.is_ok());
    }

    #[tokio::test]
    async fn test_mock_state_with_initial_data() {
        let mut initial_states = HashMap::new();
        initial_states.insert(
            "Zone1".to_string(),
            ThermostatState {
                relay_pin: 23,
                temp_pin: 4,
                temp_id: "test".to_string(),
                target_temp: 21.0,
                target_state: 1,
                current_state: 1,
                last_updated: chrono::Utc::now(),
            },
        );

        let manager = MockStateManager::with_states(initial_states);

        let loaded = manager.load_state("Zone1").await.unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().target_temp, 21.0);
    }
}
