//! NVS (Non-Volatile Storage) stub implementation for ESP32-C6
//!
//! TODO: This is a stub implementation for MVP compilation.
//! For production, implement proper NVS storage using esp-storage or sequential-storage.
//! The challenge is that FlashStorage in esp-storage 0.8+ requires a Flash peripheral
//! parameter which complicates the async storage trait implementation.
//!
//! Possible solutions:
//! 1. Use unsafe static Flash access
//! 2. Pass Flash peripheral through the storage struct with proper lifetimes
//! 3. Use a different storage abstraction like sequential-storage with proper setup

extern crate alloc;
use alloc::boxed::Box;

use core::fmt::Write as FmtWrite;
use heapless::String as HeaplessString;
use thermos_core::{PersistenceError, StateStorage, ThermostatState};

/// Stub NVS storage implementation
///
/// This is a no-op implementation for MVP. State is not persisted across reboots.
/// TODO: Implement actual NVS storage for production use.
pub struct NvsStorage {}

impl NvsStorage {
    /// Create a new NVS storage instance
    pub fn new() -> Self {
        Self {}
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
        _zone_name: &str,
        _state: &ThermostatState,
    ) -> Result<(), PersistenceError> {
        // TODO: Implement actual flash storage
        // For MVP, we just return Ok without persisting
        Ok(())
    }

    async fn load_state(
        &mut self,
        _zone_name: &str,
    ) -> Result<Option<ThermostatState>, PersistenceError> {
        // TODO: Implement actual flash storage
        // For MVP, always return None (no saved state)
        Ok(None)
    }

    async fn delete_state(&mut self, _zone_name: &str) -> Result<(), PersistenceError> {
        // TODO: Implement actual flash storage
        Ok(())
    }

    async fn ping(&mut self) -> Result<(), PersistenceError> {
        // Flash is always available on ESP32
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stub_storage_creation() {
        let _storage = NvsStorage::new();
        // Just verify it compiles and creates
    }

    #[test]
    fn test_stub_storage_default() {
        let _storage = NvsStorage::default();
        // Verify Default trait works
    }

    #[tokio::test]
    async fn test_stub_storage_save() {
        let mut storage = NvsStorage::new();
        let state = ThermostatState::default();

        // Stub implementation always succeeds
        let result = storage.save_state("Zone1", &state).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stub_storage_load() {
        let mut storage = NvsStorage::new();

        // Stub implementation always returns None
        let result = storage.load_state("Zone1").await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_stub_storage_delete() {
        let mut storage = NvsStorage::new();

        // Stub implementation always succeeds
        let result = storage.delete_state("Zone1").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stub_storage_ping() {
        let mut storage = NvsStorage::new();

        // Stub implementation always succeeds (flash is always available)
        let result = storage.ping().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_stub_storage_trait_implementation() {
        // Verify StateStorage trait is properly implemented
        let mut storage: Box<dyn StateStorage> = Box::new(NvsStorage::new());
        let state = ThermostatState::default();

        storage.save_state("test", &state).await.unwrap();
        let loaded = storage.load_state("test").await.unwrap();
        assert!(loaded.is_none()); // Stub always returns None

        storage.ping().await.unwrap();
    }
}
