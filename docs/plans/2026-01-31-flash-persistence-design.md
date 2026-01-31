# Flash Persistence Design

**Date:** 2026-01-31
**Status:** Approved

## Overview

Persist both Matter commissioning data and thermostat zone setpoints across power cycles using a single flash-backed storage implementation.

### Goals

- Matter commissioning survives reboots (no re-pairing with HomeKit)
- Zone setpoints survive reboots (user preferences restored)
- Flash-safe write patterns (no wear-out risk)
- Minimal code changes to existing architecture

### Non-Goals

- Persisting current temperature readings (changes every 6 seconds)
- Persisting relay state (control loop restores within 6 seconds)

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                      Application Layer                       │
├─────────────────────┬───────────────────────────────────────┤
│   Matter Stack      │         Thermostat Control            │
│   (rs-matter)       │         (thermos-esp32)               │
│                     │                                       │
│   - Fabrics         │   - Zone setpoints                    │
│   - BasicInfo       │   - System mode                       │
│   - Networks        │                                       │
└─────────┬───────────┴───────────────┬───────────────────────┘
          │                           │
          │    KvBlobStore trait      │
          └───────────┬───────────────┘
                      ▼
          ┌───────────────────────┐
          │   FlashKvBlobStore    │  (new, in thermos-esp32)
          │   - load/store/remove │
          └───────────┬───────────┘
                      │
                      ▼
          ┌───────────────────────┐
          │  sequential-storage   │  (crate)
          │  - wear leveling      │
          │  - append-only log    │
          └───────────┬───────────┘
                      │
                      ▼
          ┌───────────────────────┐
          │     esp-storage       │  (flash driver)
          │     Flash partition   │
          └───────────────────────┘
```

## Key Allocation

| Key Range | Purpose | Owner |
|-----------|---------|-------|
| 0x0000 | Matter Fabrics | rs-matter |
| 0x0001 | Matter BasicInfo | rs-matter |
| 0x0002 | Matter Networks | rs-matter |
| 0x0003 - 0x0FFF | Reserved for future rs-matter use | - |
| 0x1000 - 0x100F | Zone 0-15 setpoints | thermos-esp32 |

### Zone Key Calculation

```rust
const ZONE_KEY_BASE: u16 = 0x1000;

fn zone_key(zone_index: usize) -> u16 {
    ZONE_KEY_BASE + zone_index as u16
}
```

## Data Format

### Zone State (5 bytes per zone)

```rust
#[derive(Clone, Copy)]
#[repr(C, packed)]
struct PersistedZoneState {
    /// Heating setpoint in Matter centi-degrees (i16)
    heating_setpoint: i16,    // 2 bytes
    /// Cooling setpoint in Matter centi-degrees (i16)
    cooling_setpoint: i16,    // 2 bytes
    /// System mode: 0=Off, 1=Heat, 2=Cool
    system_mode: u8,          // 1 byte
}
```

Total for 7 zones: 35 bytes

## Implementation Details

### 1. FlashKvBlobStore

**Location:** `thermos-esp32/src/persistence/flash_store.rs`

Implements `rs_matter_stack::persist::KvBlobStore` trait using `sequential-storage::map`.

```rust
use sequential_storage::map::Map;
use esp_storage::FlashStorage;

pub struct FlashKvBlobStore {
    flash_range: Range<u32>,
}

impl KvBlobStore for FlashKvBlobStore {
    async fn load<F>(&mut self, key: u16, buf: &mut [u8], cb: F) -> Result<(), Error>
    where F: FnOnce(Option<&[u8]>) -> Result<(), Error>;

    async fn store<F>(&mut self, key: u16, buf: &mut [u8], cb: F) -> Result<(), Error>
    where F: FnOnce(&mut [u8]) -> Result<usize, Error>;

    async fn remove(&mut self, key: u16, buf: &mut [u8]) -> Result<(), Error>;
}
```

### 2. Dirty Flag Mechanism

**Location:** `thermos-matter/src/state.rs`

Add to `ZoneThermostatState`:

```rust
pub struct ZoneThermostatState {
    // ... existing fields ...
    dirty: AtomicBool,  // NEW
}

impl ZoneThermostatState {
    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Relaxed)
    }

    pub fn clear_dirty(&self) {
        self.dirty.store(false, Ordering::Relaxed)
    }
}
```

Modify setters to set dirty flag:

```rust
pub fn set_occupied_heating_setpoint(&self, value: i16) -> Result<(), SetpointError> {
    // ... validation ...
    self.occupied_heating_setpoint.store(value, Ordering::Relaxed);
    self.dirty.store(true, Ordering::Relaxed);  // NEW
    Ok(())
}
```

### 3. Background Persist Task

**Location:** `thermos-esp32/src/persistence/task.rs`

```rust
const PERSIST_CHECK_INTERVAL_SECS: u64 = 5;

#[embassy_executor::task]
pub async fn persist_task(store: &'static FlashKvBlobStore) -> ! {
    loop {
        Timer::after(Duration::from_secs(PERSIST_CHECK_INTERVAL_SECS)).await;

        for zone_idx in 0..NUM_ZONES {
            let state = &ZONE_STATES[zone_idx];

            if state.is_dirty() {
                match store.store_zone(zone_idx, &persisted).await {
                    Ok(()) => state.clear_dirty(),
                    Err(e) => log::error!("Persist failed: {:?}", e),
                }
            }
        }
    }
}
```

### 4. Startup Loading

**Location:** `thermos-esp32/src/main.rs`

```rust
async fn main(spawner: Spawner) {
    // 1. Hardware init
    // 2. Initialize flash storage
    let store = FlashKvBlobStore::new(PERSIST_PARTITION_RANGE);

    // 3. Load zone states
    for zone_idx in 0..NUM_ZONES {
        if let Ok(Some(persisted)) = store.load_zone(zone_idx).await {
            let state = &ZONE_STATES[zone_idx];
            let _ = state.set_occupied_heating_setpoint(persisted.heating_setpoint);
            let _ = state.set_occupied_cooling_setpoint(persisted.cooling_setpoint);
            state.set_system_mode(SystemMode::from_u8(persisted.system_mode)
                .unwrap_or(SystemMode::Heat));
            state.clear_dirty();
        }
    }

    // 4. Initialize Matter with flash store (replaces DummyKvBlobStore)
    let persist = stack.create_persist_with_comm_window(store);
    persist.load().await?;

    // 5. Spawn persist task
    spawner.spawn(persist_task(store)).unwrap();

    // 6. Start control loops
}
```

## Flash Partition Layout

**Partition Table Addition** (`partitions.csv`):

```csv
# Name,   Type, SubType, Offset,   Size,    Flags
persist,  data, nvs,     0x310000, 0x10000,   # 64KB for persistence
```

**Constants:**

```rust
const PERSIST_PARTITION_START: u32 = 0x310000;
const PERSIST_PARTITION_SIZE: u32 = 0x10000;  // 64KB
```

### Size Justification

| Data | Size |
|------|------|
| Matter Fabrics | ~2KB |
| Matter BasicInfo | ~100B |
| Matter Networks | ~200B |
| Zone states (7 zones) | 35B |
| **Total active** | **~2.5KB** |

64KB provides ~25x headroom for `sequential-storage` wear leveling.

## Files to Modify

| File | Change |
|------|--------|
| `thermos-matter/src/state.rs` | Add `dirty: AtomicBool` field and methods |
| `thermos-esp32/src/matter/stack.rs` | Replace `DummyKvBlobStore` with `FlashKvBlobStore` |
| `thermos-esp32/src/main.rs` | Add storage init, zone loading, spawn persist task |
| `thermos-esp32/Cargo.toml` | Ensure `sequential-storage` features enabled |
| `partitions.csv` | Add persist partition |

## New Files

| File | Purpose |
|------|---------|
| `thermos-esp32/src/persistence/mod.rs` | Module exports |
| `thermos-esp32/src/persistence/flash_store.rs` | `FlashKvBlobStore` implementation |
| `thermos-esp32/src/persistence/task.rs` | Background persist task |
| `thermos-esp32/src/persistence/types.rs` | `PersistedZoneState` struct |

## Write Frequency Analysis

| Event | Frequency | Flash Impact |
|-------|-----------|--------------|
| User changes setpoint | Few times/day | Negligible |
| User changes mode | Few times/day | Negligible |
| Matter commissioning | Once | Negligible |
| Control loop (temp/relay) | Every 6 sec | **Not persisted** |

ESP32 flash endurance: ~100,000 erase cycles
With `sequential-storage` wear leveling and write-on-change-only pattern: **decades of safe operation**

## Restart Behavior

1. Device powers on
2. Loads persisted setpoints and mode → zones have correct targets
3. Loads Matter fabrics → device stays commissioned with HomeKit
4. Control loop starts, reads temperature
5. Within 6 seconds: hysteresis logic turns heating ON if needed
6. User sees correct setpoint in HomeKit immediately

**Edge case:** If temperature is in dead band (±0.5°C of setpoint) at restart, relay stays OFF until temperature drifts. For hydronic heating with thermal mass, this is negligible.
