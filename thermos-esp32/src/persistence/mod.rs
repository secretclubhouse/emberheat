//! Flash persistence for Matter and thermostat state
//!
//! This module provides flash-backed storage for:
//! - Matter commissioning data (fabrics, networks)
//! - Zone thermostat setpoints and modes
//!
//! Uses `sequential-storage` crate for wear-leveled key-value storage.

pub mod flash_store;
pub mod types;

pub use flash_store::{FlashKvBlobStore, StorageKey};
pub use types::{zone_key, PersistedZoneState, ZONE_KEY_BASE};
