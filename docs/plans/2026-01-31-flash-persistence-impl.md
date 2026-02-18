# Flash Persistence Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Persist Matter commissioning and zone setpoints across power cycles using flash storage.

**Architecture:** Single `FlashKvBlobStore` implementation backs both Matter (rs-matter) persistence and thermostat zone setpoints. Uses `sequential-storage` crate for wear-leveled key-value storage on a 64KB flash partition. Write-on-change-only pattern ensures flash longevity.

**Tech Stack:** `sequential-storage` v3.0, `esp-storage`, `embedded-storage-async`, Embassy tasks

**Design Document:** `docs/plans/2026-01-31-flash-persistence-design.md`

---

## Task 1: Add Dirty Flag to ZoneThermostatState

**Files:**
- Modify: `emberheat-matter/src/state.rs`

**Step 1: Add AtomicBool import and dirty field**

In `emberheat-matter/src/state.rs`, add `AtomicBool` to the imports at line 12:

```rust
use core::sync::atomic::{AtomicBool, AtomicI16, AtomicU16, AtomicU8, Ordering};
```

**Step 2: Add dirty field to struct**

Add the `dirty` field to `ZoneThermostatState` struct (after `running_state`):

```rust
pub struct ZoneThermostatState {
    /// Current temperature in Matter centi-degrees (0.01°C)
    local_temperature: AtomicI16,

    /// Target heating setpoint in Matter centi-degrees
    occupied_heating_setpoint: AtomicI16,

    /// Target cooling setpoint in Matter centi-degrees
    occupied_cooling_setpoint: AtomicI16,

    /// Current system mode (Off/Heat/Cool)
    system_mode: AtomicU8,

    /// Running state bitmap
    running_state: AtomicU16,

    /// Dirty flag for persistence - set when user-controlled state changes
    dirty: AtomicBool,
}
```

**Step 3: Initialize dirty in new()**

Update the `new()` const fn to initialize `dirty`:

```rust
pub const fn new() -> Self {
    Self {
        local_temperature: AtomicI16::new(temperature::NULL_TEMPERATURE),
        occupied_heating_setpoint: AtomicI16::new(defaults::OCCUPIED_HEATING_SETPOINT),
        occupied_cooling_setpoint: AtomicI16::new(defaults::OCCUPIED_COOLING_SETPOINT),
        system_mode: AtomicU8::new(SystemMode::Heat as u8),
        running_state: AtomicU16::new(0),
        dirty: AtomicBool::new(false),
    }
}
```

**Step 4: Add dirty flag methods**

Add these methods to `impl ZoneThermostatState` (after the running state methods, before the `Default` impl):

```rust
// ---- Persistence dirty flag ----

/// Check if state has changed since last persist
///
/// Returns true if any user-controlled value (setpoints, mode) has changed.
pub fn is_dirty(&self) -> bool {
    self.dirty.load(Ordering::Relaxed)
}

/// Clear dirty flag after successful persist
pub fn clear_dirty(&self) {
    self.dirty.store(false, Ordering::Relaxed);
}

/// Mark state as dirty (needs persistence)
fn mark_dirty(&self) {
    self.dirty.store(true, Ordering::Relaxed);
}
```

**Step 5: Update setters to mark dirty**

Modify `set_occupied_heating_setpoint` to mark dirty on success:

```rust
pub fn set_occupied_heating_setpoint(
    &self,
    setpoint_centi_c: i16,
) -> Result<(), SetpointError> {
    if setpoint_centi_c < defaults::MIN_HEAT_SETPOINT {
        return Err(SetpointError::BelowMinimum);
    }
    if setpoint_centi_c > defaults::MAX_HEAT_SETPOINT {
        return Err(SetpointError::AboveMaximum);
    }
    self.occupied_heating_setpoint
        .store(setpoint_centi_c, Ordering::Relaxed);
    self.mark_dirty();
    Ok(())
}
```

Modify `set_occupied_cooling_setpoint` similarly:

```rust
pub fn set_occupied_cooling_setpoint(
    &self,
    setpoint_centi_c: i16,
) -> Result<(), SetpointError> {
    if setpoint_centi_c < defaults::MIN_COOL_SETPOINT {
        return Err(SetpointError::BelowMinimum);
    }
    if setpoint_centi_c > defaults::MAX_COOL_SETPOINT {
        return Err(SetpointError::AboveMaximum);
    }
    self.occupied_cooling_setpoint
        .store(setpoint_centi_c, Ordering::Relaxed);
    self.mark_dirty();
    Ok(())
}
```

Modify `set_system_mode`:

```rust
/// Set the system mode
pub fn set_system_mode(&self, mode: SystemMode) {
    self.system_mode.store(mode as u8, Ordering::Relaxed);
    self.mark_dirty();
}
```

**Step 6: Add tests for dirty flag**

Add these tests to the `#[cfg(test)] mod tests` section:

```rust
#[test]
fn test_dirty_flag_initial_state() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());
}

#[test]
fn test_dirty_flag_set_on_heating_setpoint_change() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());

    state.set_occupied_heating_setpoint(2200).unwrap();
    assert!(state.is_dirty());
}

#[test]
fn test_dirty_flag_set_on_cooling_setpoint_change() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());

    state.set_occupied_cooling_setpoint(2600).unwrap();
    assert!(state.is_dirty());
}

#[test]
fn test_dirty_flag_set_on_mode_change() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());

    state.set_system_mode(SystemMode::Off);
    assert!(state.is_dirty());
}

#[test]
fn test_dirty_flag_not_set_on_invalid_setpoint() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());

    // Try to set invalid setpoint
    let result = state.set_occupied_heating_setpoint(500); // Below minimum
    assert!(result.is_err());
    assert!(!state.is_dirty()); // Should still be clean
}

#[test]
fn test_dirty_flag_clear() {
    let state = ZoneThermostatState::new();
    state.set_occupied_heating_setpoint(2200).unwrap();
    assert!(state.is_dirty());

    state.clear_dirty();
    assert!(!state.is_dirty());
}

#[test]
fn test_dirty_flag_not_set_on_temperature_update() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());

    // Temperature updates should NOT set dirty (not user-controlled)
    state.set_local_temperature_celsius(21.5);
    assert!(!state.is_dirty());
}

#[test]
fn test_dirty_flag_not_set_on_heating_state_update() {
    let state = ZoneThermostatState::new();
    assert!(!state.is_dirty());

    // Running state updates should NOT set dirty (not user-controlled)
    state.set_heating_active(true);
    assert!(!state.is_dirty());
}
```

**Step 7: Run tests**

Run: `cargo test -p emberheat-matter`

Expected: All tests pass, including new dirty flag tests.

**Step 8: Commit**

```bash
git add emberheat-matter/src/state.rs
git commit -m "feat(matter): add dirty flag to ZoneThermostatState for persistence"
```

---

## Task 2: Add sequential-storage Dependency

**Files:**
- Modify: `emberheat-esp32/Cargo.toml`

**Step 1: Add sequential-storage dependency**

Add to `[dependencies]` section in `emberheat-esp32/Cargo.toml` (after `embedded-storage`):

```toml
# Sequential flash storage with wear leveling
sequential-storage = { version = "3.0", default-features = false }
embedded-storage-async = "0.4"
```

**Step 2: Verify it compiles**

Run: `cargo check -p emberheat-esp32`

Expected: Compiles without errors.

**Step 3: Commit**

```bash
git add emberheat-esp32/Cargo.toml
git commit -m "chore(esp32): add sequential-storage dependency for flash persistence"
```

---

## Task 3: Create Persistence Module Structure

**Files:**
- Create: `emberheat-esp32/src/persistence/mod.rs`
- Create: `emberheat-esp32/src/persistence/types.rs`
- Modify: `emberheat-esp32/src/lib.rs`

**Step 1: Create persistence module directory**

Run: `mkdir -p /Users/josh/Code/emberheat/emberheat-esp32/src/persistence`

**Step 2: Create types.rs with PersistedZoneState**

Create `emberheat-esp32/src/persistence/types.rs`:

```rust
//! Persistence data types
//!
//! Defines the data structures for persisted state.

use emberheat_matter::types::SystemMode;

/// Key base for zone data in KvBlobStore
/// Keys 0x0000-0x0FFF are reserved for rs-matter
pub const ZONE_KEY_BASE: u16 = 0x1000;

/// Calculate storage key for a zone index
pub const fn zone_key(zone_index: usize) -> u16 {
    ZONE_KEY_BASE + zone_index as u16
}

/// Persisted zone state (5 bytes)
///
/// Only stores user-controlled values that should survive power cycles.
/// Current temperature and relay state are NOT persisted (restored by control loop).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct PersistedZoneState {
    /// Heating setpoint in Matter centi-degrees (i16)
    pub heating_setpoint: i16,
    /// Cooling setpoint in Matter centi-degrees (i16)
    pub cooling_setpoint: i16,
    /// System mode: 0=Off, 1=Heat, 2=Cool
    pub system_mode: u8,
}

impl PersistedZoneState {
    /// Size of persisted data in bytes
    pub const SIZE: usize = 5;

    /// Create from ZoneThermostatState
    pub fn from_zone_state(state: &emberheat_matter::state::ZoneThermostatState) -> Self {
        Self {
            heating_setpoint: state.get_occupied_heating_setpoint(),
            cooling_setpoint: state.get_occupied_cooling_setpoint(),
            system_mode: state.get_system_mode() as u8,
        }
    }

    /// Apply to ZoneThermostatState
    ///
    /// Silently ignores validation errors (uses defaults for invalid values).
    pub fn apply_to_zone_state(&self, state: &emberheat_matter::state::ZoneThermostatState) {
        let _ = state.set_occupied_heating_setpoint(self.heating_setpoint);
        let _ = state.set_occupied_cooling_setpoint(self.cooling_setpoint);
        if let Some(mode) = SystemMode::from_u8(self.system_mode) {
            state.set_system_mode(mode);
        }
        // Clear dirty since we just loaded
        state.clear_dirty();
    }

    /// Serialize to bytes
    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        let mut buf = [0u8; Self::SIZE];
        buf[0..2].copy_from_slice(&self.heating_setpoint.to_le_bytes());
        buf[2..4].copy_from_slice(&self.cooling_setpoint.to_le_bytes());
        buf[4] = self.system_mode;
        buf
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }
        Some(Self {
            heating_setpoint: i16::from_le_bytes([data[0], data[1]]),
            cooling_setpoint: i16::from_le_bytes([data[2], data[3]]),
            system_mode: data[4],
        })
    }
}

impl Default for PersistedZoneState {
    fn default() -> Self {
        Self {
            heating_setpoint: emberheat_matter::types::defaults::OCCUPIED_HEATING_SETPOINT,
            cooling_setpoint: emberheat_matter::types::defaults::OCCUPIED_COOLING_SETPOINT,
            system_mode: SystemMode::Heat as u8,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_key_calculation() {
        assert_eq!(zone_key(0), 0x1000);
        assert_eq!(zone_key(1), 0x1001);
        assert_eq!(zone_key(6), 0x1006);
        assert_eq!(zone_key(15), 0x100F);
    }

    #[test]
    fn test_persisted_zone_state_size() {
        assert_eq!(PersistedZoneState::SIZE, 5);
    }

    #[test]
    fn test_persisted_zone_state_serialization() {
        let state = PersistedZoneState {
            heating_setpoint: 2100, // 21°C
            cooling_setpoint: 2400, // 24°C
            system_mode: 1,         // Heat
        };

        let bytes = state.to_bytes();
        let restored = PersistedZoneState::from_bytes(&bytes).unwrap();

        assert_eq!(state, restored);
    }

    #[test]
    fn test_persisted_zone_state_from_bytes_too_short() {
        let short_data = [0u8; 4];
        assert!(PersistedZoneState::from_bytes(&short_data).is_none());
    }

    #[test]
    fn test_persisted_zone_state_default() {
        let state = PersistedZoneState::default();
        assert_eq!(state.heating_setpoint, 2000); // 20°C
        assert_eq!(state.cooling_setpoint, 2400); // 24°C
        assert_eq!(state.system_mode, 1);         // Heat
    }
}
```

**Step 3: Create mod.rs**

Create `emberheat-esp32/src/persistence/mod.rs`:

```rust
//! Flash persistence for Matter and thermostat state
//!
//! This module provides flash-backed storage for:
//! - Matter commissioning data (fabrics, networks)
//! - Zone thermostat setpoints and modes
//!
//! Uses `sequential-storage` crate for wear-leveled key-value storage.

pub mod types;

pub use types::{zone_key, PersistedZoneState, ZONE_KEY_BASE};
```

**Step 4: Add persistence module to lib.rs**

In `emberheat-esp32/src/lib.rs`, add the module declaration after `pub mod persistence_nvs;`:

```rust
pub mod persistence;
```

And add re-export at the bottom with other re-exports:

```rust
pub use persistence::{PersistedZoneState, ZONE_KEY_BASE};
```

**Step 5: Run tests**

Run: `cargo test -p emberheat-esp32 --lib`

Expected: Tests pass.

**Step 6: Commit**

```bash
git add emberheat-esp32/src/persistence/
git add emberheat-esp32/src/lib.rs
git commit -m "feat(esp32): add persistence module with PersistedZoneState type"
```

---

## Task 4: Implement FlashKvBlobStore

**Files:**
- Create: `emberheat-esp32/src/persistence/flash_store.rs`
- Modify: `emberheat-esp32/src/persistence/mod.rs`

**Step 1: Create flash_store.rs**

Create `emberheat-esp32/src/persistence/flash_store.rs`:

```rust
//! Flash-backed KvBlobStore implementation
//!
//! Implements the `KvBlobStore` trait from rs-matter-stack using
//! `sequential-storage` for wear-leveled flash storage.

use core::ops::Range;

use embedded_storage_async::nor_flash::NorFlash;
use rs_matter::error::{Error, ErrorCode};
use rs_matter_embassy::stack::persist::KvBlobStore;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{fetch_item, remove_item, store_item, SerializationError};

use super::types::{zone_key, PersistedZoneState, ZONE_KEY_BASE};

/// Flash storage key wrapper for sequential-storage
///
/// Implements the Key trait required by sequential-storage map operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StorageKey(pub u16);

impl sequential_storage::map::Key for StorageKey {
    fn serialize_into(&self, buffer: &mut [u8]) -> Result<usize, SerializationError> {
        if buffer.len() < 2 {
            return Err(SerializationError::BufferTooSmall);
        }
        buffer[0..2].copy_from_slice(&self.0.to_le_bytes());
        Ok(2)
    }

    fn deserialize_from(buffer: &[u8]) -> Result<(Self, usize), SerializationError> {
        if buffer.len() < 2 {
            return Err(SerializationError::BufferTooSmall);
        }
        let key = u16::from_le_bytes([buffer[0], buffer[1]]);
        Ok((Self(key), 2))
    }
}

/// Flash-backed key-value blob store
///
/// Provides persistence for both Matter commissioning data (keys 0-2)
/// and thermostat zone state (keys 0x1000+).
pub struct FlashKvBlobStore<F> {
    flash: F,
    flash_range: Range<u32>,
}

impl<F> FlashKvBlobStore<F>
where
    F: NorFlash,
{
    /// Create a new flash store
    ///
    /// # Arguments
    /// * `flash` - Flash storage peripheral
    /// * `flash_range` - Address range for the persistence partition
    pub fn new(flash: F, flash_range: Range<u32>) -> Self {
        Self { flash, flash_range }
    }

    /// Get mutable access to the flash peripheral
    pub fn flash_mut(&mut self) -> &mut F {
        &mut self.flash
    }

    /// Get the flash range
    pub fn flash_range(&self) -> Range<u32> {
        self.flash_range.clone()
    }

    /// Load a zone's persisted state
    pub async fn load_zone(&mut self, zone_index: usize) -> Result<Option<PersistedZoneState>, Error> {
        let key = StorageKey(zone_key(zone_index));
        let mut buf = [0u8; PersistedZoneState::SIZE + 16]; // Extra space for sequential-storage overhead
        let mut data_buf = [0u8; PersistedZoneState::SIZE];

        match fetch_item::<StorageKey, _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &key,
        )
        .await
        {
            Ok(Some(data)) => {
                let len = data.len().min(data_buf.len());
                data_buf[..len].copy_from_slice(&data[..len]);
                Ok(PersistedZoneState::from_bytes(&data_buf))
            }
            Ok(None) => Ok(None),
            Err(e) => {
                log::error!("Flash load error for zone {}: {:?}", zone_index, e);
                Err(ErrorCode::StdIoError.into())
            }
        }
    }

    /// Store a zone's state
    pub async fn store_zone(
        &mut self,
        zone_index: usize,
        state: &PersistedZoneState,
    ) -> Result<(), Error> {
        let key = StorageKey(zone_key(zone_index));
        let data = state.to_bytes();
        let mut buf = [0u8; 64]; // Scratch buffer for sequential-storage

        store_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &key,
            &data[..],
        )
        .await
        .map_err(|e| {
            log::error!("Flash store error for zone {}: {:?}", zone_index, e);
            Error::new(ErrorCode::StdIoError)
        })
    }

    /// Erase all data (factory reset)
    pub async fn erase_all(&mut self) -> Result<(), Error> {
        sequential_storage::erase_all(&mut self.flash, self.flash_range.clone())
            .await
            .map_err(|e| {
                log::error!("Flash erase error: {:?}", e);
                Error::new(ErrorCode::StdIoError)
            })
    }
}

impl<F> KvBlobStore for FlashKvBlobStore<F>
where
    F: NorFlash,
{
    async fn load<CB>(&mut self, key: u16, buf: &mut [u8], cb: CB) -> Result<(), Error>
    where
        CB: FnOnce(Option<&[u8]>) -> Result<(), Error>,
    {
        let storage_key = StorageKey(key);
        let mut scratch = [0u8; 128];

        match fetch_item::<StorageKey, _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut scratch,
            &storage_key,
        )
        .await
        {
            Ok(Some(data)) => {
                let len = data.len().min(buf.len());
                buf[..len].copy_from_slice(&data[..len]);
                cb(Some(&buf[..len]))
            }
            Ok(None) => cb(None),
            Err(e) => {
                log::error!("KvBlobStore load error for key {}: {:?}", key, e);
                cb(None) // Treat errors as missing data
            }
        }
    }

    async fn store<CB>(&mut self, key: u16, buf: &mut [u8], cb: CB) -> Result<(), Error>
    where
        CB: FnOnce(&mut [u8]) -> Result<usize, Error>,
    {
        // Call callback to fill buffer with data to store
        let len = cb(buf)?;
        let data = &buf[..len];

        let storage_key = StorageKey(key);
        let mut scratch = [0u8; 128];

        store_item(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut scratch,
            &storage_key,
            data,
        )
        .await
        .map_err(|e| {
            log::error!("KvBlobStore store error for key {}: {:?}", key, e);
            Error::new(ErrorCode::StdIoError)
        })
    }

    async fn remove(&mut self, key: u16, _buf: &mut [u8]) -> Result<(), Error> {
        let storage_key = StorageKey(key);
        let mut scratch = [0u8; 64];

        // remove_item returns Ok(true) if removed, Ok(false) if not found
        match remove_item::<StorageKey>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut scratch,
            &storage_key,
        )
        .await
        {
            Ok(_) => Ok(()),
            Err(e) => {
                log::error!("KvBlobStore remove error for key {}: {:?}", key, e);
                Err(ErrorCode::StdIoError.into())
            }
        }
    }
}
```

**Step 2: Update mod.rs to export flash_store**

Update `emberheat-esp32/src/persistence/mod.rs`:

```rust
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
```

**Step 3: Verify it compiles**

Run: `cargo check -p emberheat-esp32`

Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add emberheat-esp32/src/persistence/
git commit -m "feat(esp32): implement FlashKvBlobStore with KvBlobStore trait"
```

---

## Task 5: Add Persistence Constants to Config

**Files:**
- Modify: `emberheat-esp32/src/config.rs`

**Step 1: Add persistence constants**

Add these constants near the top of `emberheat-esp32/src/config.rs` (after `ONEWIRE_PIN`):

```rust
// ============================================================================
// Flash Persistence Configuration
// ============================================================================

/// Start address of persistence partition in flash
/// Must not overlap with app or other partitions
pub const PERSIST_PARTITION_START: u32 = 0x310000;

/// Size of persistence partition (64KB)
/// Provides ~25x headroom for sequential-storage wear leveling
pub const PERSIST_PARTITION_SIZE: u32 = 0x10000;

/// Flash range for persistence partition
pub const PERSIST_FLASH_RANGE: core::ops::Range<u32> =
    PERSIST_PARTITION_START..(PERSIST_PARTITION_START + PERSIST_PARTITION_SIZE);

/// Interval between persistence checks (seconds)
pub const PERSIST_CHECK_INTERVAL_SECS: u64 = 5;

// ============================================================================
// Factory Reset Configuration
// ============================================================================

/// GPIO pin for factory reset button (directly to GND when pressed)
/// Hold during boot to trigger factory reset
pub const FACTORY_RESET_PIN: u8 = 9;

/// How long reset button must be held to confirm factory reset (seconds)
pub const FACTORY_RESET_HOLD_SECS: u64 = 5;
```

**Step 2: Verify it compiles**

Run: `cargo check -p emberheat-esp32`

Expected: Compiles without errors.

**Step 3: Commit**

```bash
git add emberheat-esp32/src/config.rs
git commit -m "feat(esp32): add persistence and factory reset configuration constants"
```

---

## Task 6: Implement Background Persist Task

**Files:**
- Create: `emberheat-esp32/src/persistence/task.rs`
- Modify: `emberheat-esp32/src/persistence/mod.rs`

**Step 1: Create task.rs**

Create `emberheat-esp32/src/persistence/task.rs`:

```rust
//! Background persistence task
//!
//! Periodically checks for dirty zone states and persists them to flash.

use embassy_time::{Duration, Timer};
use embedded_storage_async::nor_flash::NorFlash;

use crate::config::{NUM_ZONES, PERSIST_CHECK_INTERVAL_SECS};
use crate::matter::zones::ZONE_STATES;

use super::flash_store::FlashKvBlobStore;
use super::types::PersistedZoneState;

/// Run the background persistence task
///
/// This task periodically checks all zones for dirty flags and persists
/// changed state to flash. It runs indefinitely.
///
/// # Arguments
/// * `store` - Mutable reference to the flash store
///
/// # Write Behavior
/// - Only writes zones that have changed (dirty flag set)
/// - Clears dirty flag after successful write
/// - Retries on next cycle if write fails (dirty flag stays set)
pub async fn run_persist_task<F>(store: &mut FlashKvBlobStore<F>) -> !
where
    F: NorFlash,
{
    log::info!(
        "Persistence task started (interval: {}s, zones: {})",
        PERSIST_CHECK_INTERVAL_SECS,
        NUM_ZONES
    );

    loop {
        Timer::after(Duration::from_secs(PERSIST_CHECK_INTERVAL_SECS)).await;

        for zone_idx in 0..NUM_ZONES {
            let state = &ZONE_STATES[zone_idx];

            if state.is_dirty() {
                let persisted = PersistedZoneState::from_zone_state(state);

                match store.store_zone(zone_idx, &persisted).await {
                    Ok(()) => {
                        state.clear_dirty();
                        log::info!(
                            "Persisted zone {} (heat: {}°C, cool: {}°C, mode: {})",
                            zone_idx,
                            persisted.heating_setpoint as f32 / 100.0,
                            persisted.cooling_setpoint as f32 / 100.0,
                            persisted.system_mode
                        );
                    }
                    Err(e) => {
                        // Don't clear dirty - retry next cycle
                        log::error!("Failed to persist zone {}: {:?}", zone_idx, e);
                    }
                }
            }
        }
    }
}

/// Load persisted state for all zones on startup
///
/// # Arguments
/// * `store` - Mutable reference to the flash store
///
/// # Behavior
/// - Loads each zone's persisted state and applies to ZONE_STATES
/// - Logs success/failure for each zone
/// - Continues on errors (zones keep defaults)
/// - Clears dirty flags after loading
pub async fn load_all_zones<F>(store: &mut FlashKvBlobStore<F>)
where
    F: NorFlash,
{
    log::info!("Loading persisted zone states...");

    for zone_idx in 0..NUM_ZONES {
        match store.load_zone(zone_idx).await {
            Ok(Some(persisted)) => {
                let state = &ZONE_STATES[zone_idx];
                persisted.apply_to_zone_state(state);
                log::info!(
                    "Zone {}: restored (heat: {}°C, cool: {}°C, mode: {})",
                    zone_idx,
                    persisted.heating_setpoint as f32 / 100.0,
                    persisted.cooling_setpoint as f32 / 100.0,
                    persisted.system_mode
                );
            }
            Ok(None) => {
                log::info!("Zone {}: no persisted state, using defaults", zone_idx);
            }
            Err(e) => {
                log::warn!(
                    "Zone {}: failed to load ({:?}), using defaults",
                    zone_idx,
                    e
                );
            }
        }
    }
}
```

**Step 2: Update mod.rs**

Update `emberheat-esp32/src/persistence/mod.rs`:

```rust
//! Flash persistence for Matter and thermostat state
//!
//! This module provides flash-backed storage for:
//! - Matter commissioning data (fabrics, networks)
//! - Zone thermostat setpoints and modes
//!
//! Uses `sequential-storage` crate for wear-leveled key-value storage.

pub mod flash_store;
pub mod task;
pub mod types;

pub use flash_store::{FlashKvBlobStore, StorageKey};
pub use task::{load_all_zones, run_persist_task};
pub use types::{zone_key, PersistedZoneState, ZONE_KEY_BASE};
```

**Step 3: Verify it compiles**

Run: `cargo check -p emberheat-esp32`

Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add emberheat-esp32/src/persistence/
git commit -m "feat(esp32): add background persist task and zone loading"
```

---

## Task 7: Implement Factory Reset

**Files:**
- Create: `emberheat-esp32/src/persistence/reset.rs`
- Modify: `emberheat-esp32/src/persistence/mod.rs`

**Step 1: Create reset.rs**

Create `emberheat-esp32/src/persistence/reset.rs`:

```rust
//! Factory reset functionality
//!
//! Provides functions to erase all persisted data and reset to defaults.

use embedded_storage_async::nor_flash::NorFlash;
use rs_matter::error::Error;
use rs_matter::Matter;

use crate::config::NUM_ZONES;
use crate::matter::zones::ZONE_STATES;
use emberheat_matter::types::{defaults, SystemMode};

use super::flash_store::FlashKvBlobStore;

/// Perform factory reset
///
/// This erases all persisted data and resets in-memory state to defaults.
///
/// # Arguments
/// * `store` - Flash store to erase
/// * `matter` - Matter stack to reset (optional, pass None if not initialized)
///
/// # What Gets Reset
/// - All flash persistence data (Matter fabrics, networks, zone setpoints)
/// - In-memory zone states (setpoints, modes)
/// - Matter stack state (fabrics, transport) if provided
///
/// # Post-Reset State
/// - Device will need to be re-commissioned with HomeKit
/// - All zones reset to Heat mode, 20°C heating / 24°C cooling setpoints
pub async fn factory_reset<F>(
    store: &mut FlashKvBlobStore<F>,
    matter: Option<&Matter<'_>>,
) -> Result<(), Error>
where
    F: NorFlash,
{
    log::warn!("=== FACTORY RESET INITIATED ===");

    // 1. Erase entire persistence partition
    log::info!("Erasing flash persistence partition...");
    store.erase_all().await?;
    log::info!("Flash erased successfully");

    // 2. Reset Matter stack if provided
    if let Some(m) = matter {
        log::info!("Resetting Matter stack...");
        m.reset_persist(false);
        m.reset_transport()?;
        log::info!("Matter stack reset");
    }

    // 3. Reset all zone states to defaults
    log::info!("Resetting zone states to defaults...");
    for (idx, state) in ZONE_STATES.iter().enumerate() {
        let _ = state.set_occupied_heating_setpoint(defaults::OCCUPIED_HEATING_SETPOINT);
        let _ = state.set_occupied_cooling_setpoint(defaults::OCCUPIED_COOLING_SETPOINT);
        state.set_system_mode(SystemMode::Heat);
        state.clear_dirty(); // Don't persist defaults
        log::debug!("Zone {} reset to defaults", idx);
    }

    log::warn!("=== FACTORY RESET COMPLETE ===");
    log::info!("Device will need to be re-commissioned");

    Ok(())
}

/// Reset only zone states (not Matter commissioning)
///
/// Useful for resetting thermostat settings without losing HomeKit pairing.
pub async fn reset_zone_states<F>(store: &mut FlashKvBlobStore<F>) -> Result<(), Error>
where
    F: NorFlash,
{
    log::info!("Resetting zone states...");

    // Remove zone keys from flash
    for zone_idx in 0..NUM_ZONES {
        let key = super::types::zone_key(zone_idx);
        let mut buf = [0u8; 8];
        // Ignore errors - key might not exist
        let _ = store.remove(key, &mut buf).await;
    }

    // Reset in-memory state
    for state in ZONE_STATES.iter() {
        let _ = state.set_occupied_heating_setpoint(defaults::OCCUPIED_HEATING_SETPOINT);
        let _ = state.set_occupied_cooling_setpoint(defaults::OCCUPIED_COOLING_SETPOINT);
        state.set_system_mode(SystemMode::Heat);
        state.clear_dirty();
    }

    log::info!("Zone states reset to defaults");
    Ok(())
}
```

**Step 2: Update mod.rs**

Update `emberheat-esp32/src/persistence/mod.rs`:

```rust
//! Flash persistence for Matter and thermostat state
//!
//! This module provides flash-backed storage for:
//! - Matter commissioning data (fabrics, networks)
//! - Zone thermostat setpoints and modes
//!
//! Uses `sequential-storage` crate for wear-leveled key-value storage.

pub mod flash_store;
pub mod reset;
pub mod task;
pub mod types;

pub use flash_store::{FlashKvBlobStore, StorageKey};
pub use reset::{factory_reset, reset_zone_states};
pub use task::{load_all_zones, run_persist_task};
pub use types::{zone_key, PersistedZoneState, ZONE_KEY_BASE};
```

**Step 3: Verify it compiles**

Run: `cargo check -p emberheat-esp32`

Expected: Compiles without errors.

**Step 4: Commit**

```bash
git add emberheat-esp32/src/persistence/
git commit -m "feat(esp32): add factory reset and zone reset functions"
```

---

## Task 8: Update lib.rs Exports

**Files:**
- Modify: `emberheat-esp32/src/lib.rs`

**Step 1: Update lib.rs with full persistence exports**

Update the persistence-related lines in `emberheat-esp32/src/lib.rs`:

Find the line:
```rust
pub use persistence::{PersistedZoneState, ZONE_KEY_BASE};
```

Replace with:
```rust
pub use persistence::{
    factory_reset, load_all_zones, run_persist_task, reset_zone_states,
    FlashKvBlobStore, PersistedZoneState, StorageKey, ZONE_KEY_BASE,
};
```

**Step 2: Verify it compiles**

Run: `cargo check -p emberheat-esp32`

Expected: Compiles without errors.

**Step 3: Commit**

```bash
git add emberheat-esp32/src/lib.rs
git commit -m "chore(esp32): export all persistence module public items"
```

---

## Task 9: Integration - Replace DummyKvBlobStore in stack.rs

**Files:**
- Modify: `emberheat-esp32/src/matter/stack.rs`

This task requires careful integration with the existing Matter stack code. The changes depend on the current structure of `stack.rs`.

**Step 1: Locate DummyKvBlobStore usage**

Search for `DummyKvBlobStore` in `emberheat-esp32/src/matter/stack.rs` and note the context.

**Step 2: Add import for FlashKvBlobStore**

Add import near the top of the file with other imports:

```rust
use crate::persistence::FlashKvBlobStore;
use crate::config::PERSIST_FLASH_RANGE;
```

**Step 3: Replace DummyKvBlobStore**

This step requires examining the actual code structure. Look for patterns like:

```rust
// Before:
let persist = stack.create_persist_with_comm_window(DummyKvBlobStore)
```

The replacement will involve:
1. Creating the FlashKvBlobStore with the flash peripheral
2. Passing it to `create_persist_with_comm_window`

**Note:** This task may need adjustment based on how the flash peripheral is currently managed in main.rs. The flash store may need to be created in main.rs and passed to the stack initialization.

**Step 4: Commit placeholder**

If full integration isn't possible without main.rs changes:

```bash
git commit --allow-empty -m "wip(esp32): placeholder for stack.rs persistence integration"
```

---

## Task 10: Integration - Update main.rs Startup Sequence

**Files:**
- Modify: `emberheat-esp32/src/main.rs`

This task integrates persistence into the main startup sequence. The specific changes depend on the current main.rs structure.

**Key Integration Points:**

1. **Early boot: Check factory reset button**
```rust
// After GPIO init, before loading any state
let reset_pin = Input::new(io.pins.gpioN, Pull::Up);
if check_factory_reset_button(&reset_pin).await {
    factory_reset(&mut store, None).await?;
    // Reboot
}
```

2. **Initialize flash store**
```rust
let flash = FlashStorage::new();
let mut store = FlashKvBlobStore::new(flash, PERSIST_FLASH_RANGE);
```

3. **Load zone states before control loops**
```rust
load_all_zones(&mut store).await;
```

4. **Pass store to Matter stack**
```rust
// Replace DummyKvBlobStore with store reference
```

5. **Spawn persist task**
```rust
// This requires the store to be in a static or passed appropriately
```

**Note:** The exact implementation depends on the current main.rs structure and how static resources are managed. Embassy tasks have specific requirements for static lifetimes.

---

## Task 11: Add Partition Table Entry

**Files:**
- Modify or create: `partitions.csv` (if using custom partition table)

**Step 1: Check current partition setup**

Run: `ls -la /Users/josh/Code/emberheat/emberheat-esp32/` to check for existing partition files.

**Step 2: Document partition requirements**

If using ESP-IDF bootloader defaults, document that the persist partition at 0x310000 needs to be added to the partition table.

The partition table entry should be:
```csv
persist,  data, nvs,     0x310000, 0x10000,
```

**Note:** Partition table configuration depends on the build system and bootloader setup. This may require additional research into the esp-bootloader-esp-idf crate configuration.

---

## Summary

After completing all tasks:

1. **emberheat-matter** has dirty flag support in `ZoneThermostatState`
2. **emberheat-esp32** has:
   - `persistence/types.rs` - Data types
   - `persistence/flash_store.rs` - `FlashKvBlobStore` implementing `KvBlobStore`
   - `persistence/task.rs` - Background persist task and zone loading
   - `persistence/reset.rs` - Factory reset functions
   - Config constants for partition and factory reset
3. **Integration** connects persistence to Matter stack and startup

**Testing:**
- Unit tests for dirty flag and serialization
- Integration testing requires hardware (flash storage)
- Manual testing: change setpoint via HomeKit, reboot, verify restored

**Hardware Testing Checklist:**
- [ ] Setpoint changes persist across reboot
- [ ] Mode changes persist across reboot
- [ ] Matter commissioning persists across reboot
- [ ] Factory reset button clears all data
- [ ] Factory reset via Matter command works
- [ ] Flash doesn't wear out with normal usage patterns
