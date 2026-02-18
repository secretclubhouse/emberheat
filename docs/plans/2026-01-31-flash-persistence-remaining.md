# Flash Persistence - Remaining Work

**Date:** 2026-01-31
**Status:** Partially Complete

## What's Done

All core implementation is complete and builds successfully:

- [x] Dirty flag in `ZoneThermostatState` (emberheat-matter)
- [x] `PersistedZoneState` type with serialization
- [x] `FlashKvBlobStore` implementing `KvBlobStore` trait
- [x] `load_all_zones()` function - loads zone state at startup
- [x] `run_persist_task()` function - background persistence loop
- [x] `factory_reset()` and `reset_zone_states()` functions
- [x] Config constants for partition layout
- [x] `matter_task()` accepts generic `KvBlobStore`
- [x] Zone states loaded before Matter stack starts
- [x] Matter commissioning uses `FlashKvBlobStore`

## What Remains

### 1. Background Persist Task Not Spawned

**Problem:** The `run_persist_task()` is implemented but not actually running. The Matter stack takes ownership of the `FlashKvBlobStore`, so we can't also give it to the persist task.

**Location:** `emberheat-esp32/src/main.rs` (matter_main_task functions)

**Options:**

A. **Shared store with mutex** - Wrap store in `Mutex<RefCell<FlashKvBlobStore>>` and share between Matter and persist task. Requires careful lifetime management.

B. **Persist inside Matter user task** - Instead of a separate task, run persistence checks inside the Matter `user_task` closure. This is simpler but couples persistence to Matter.

C. **Separate flash instances** - Create two `FlashStorage` instances (one for Matter keys 0x0000-0x0FFF, one for zone keys 0x1000+). Sequential-storage should handle this safely since they use different key ranges, but needs verification.

D. **Event-driven persistence** - Instead of periodic polling, persist immediately when dirty flag is set. Would require a channel/signal from the setpoint setter to trigger persistence.

**Recommendation:** Option B is simplest. The Matter user task already has access to zone states and runs continuously.

**Code sketch for Option B:**
```rust
// In matter_main_task, replace `()` user_task with:
let user_task = async {
    loop {
        Timer::after(Duration::from_secs(5)).await;
        for zone_idx in 0..NUM_ZONES {
            let state = &ZONE_STATES[zone_idx];
            if state.is_dirty() {
                // Need store access here - this is the challenge
            }
        }
    }
};
```

### 2. Factory Reset Button Not Wired Up

**Problem:** The GPIO button check at boot is documented but not implemented.

**Location:** `emberheat-esp32/src/main.rs` (early in startup, before loading state)

**Implementation:**
```rust
use esp_hal::gpio::{Input, Pull};
use embassy_time::{Duration, Timer};
use crate::config::{FACTORY_RESET_PIN, FACTORY_RESET_HOLD_SECS};

async fn check_factory_reset_button(pin: &Input<'_>) -> bool {
    if pin.is_low() {
        log::warn!("Factory reset button pressed - hold for {}s", FACTORY_RESET_HOLD_SECS);
        for _ in 0..FACTORY_RESET_HOLD_SECS {
            Timer::after(Duration::from_secs(1)).await;
            if pin.is_high() {
                log::info!("Factory reset cancelled");
                return false;
            }
        }
        log::warn!("Factory reset confirmed!");
        return true;
    }
    false
}
```

**Integrate in main:**
```rust
// After GPIO init, before loading any state
let reset_button = Input::new(io.pins.gpio9, Pull::Up);
if check_factory_reset_button(&reset_button).await {
    factory_reset(&mut store, None).await.ok();
    esp_hal::reset::software_reset();
}
```

**Note:** GPIO 9 is specified in config but may need adjustment based on actual hardware wiring.

### 3. Matter FactoryReset Command Handler

**Problem:** When HomeKit sends a factory reset command, we should clear our persistence data too.

**Location:** `emberheat-esp32/src/matter/stack.rs`

**Challenge:** rs-matter-embassy may have a callback or hook for factory reset events. Need to investigate the API.

**Investigation needed:**
- Check `rs_matter::Matter` for reset callbacks
- Check if `rs_matter_embassy` exposes factory reset hooks
- May need to check Matter General Commissioning cluster handling

### 4. Hardware Testing

**Required tests (on actual ESP32-C6):**

- [ ] Change setpoint via HomeKit, reboot, verify setpoint restored
- [ ] Change mode via HomeKit, reboot, verify mode restored
- [ ] Commission device, reboot, verify still paired (no re-pairing needed)
- [ ] Factory reset via GPIO button, verify device needs re-commissioning
- [ ] Commission with HomeKit AND Home Assistant, reboot, verify both work
- [ ] Stress test: change setpoint 100 times, verify flash doesn't corrupt

### 5. Error Handling Improvements (Optional)

Current implementation logs errors but continues. Consider:

- Retry logic for failed flash writes
- Fallback to defaults if flash is corrupted
- Flash health monitoring (sequential-storage may provide this)

## Files Reference

| File | Purpose |
|------|---------|
| `emberheat-esp32/src/persistence/mod.rs` | Module exports |
| `emberheat-esp32/src/persistence/types.rs` | `PersistedZoneState` |
| `emberheat-esp32/src/persistence/flash_store.rs` | `FlashKvBlobStore` |
| `emberheat-esp32/src/persistence/task.rs` | `run_persist_task`, `load_all_zones` |
| `emberheat-esp32/src/persistence/reset.rs` | Factory reset functions |
| `emberheat-esp32/src/config.rs` | Partition constants (lines 44-68) |
| `emberheat-esp32/src/main.rs` | Integration (lines 505-540) |
| `emberheat-esp32/src/matter/stack.rs` | `matter_task` with KvBlobStore |
| `emberheat-matter/src/state.rs` | Dirty flag methods |

## Quick Start for Continuing

1. Read this document
2. Read design doc: `docs/plans/2026-01-31-flash-persistence-design.md`
3. Start with Task 1 (background persist) - it's the most impactful remaining work
4. Then Task 2 (factory reset button) for complete UX
5. Hardware test after each change
