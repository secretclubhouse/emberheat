//! Background persistence task
//!
//! Periodically checks for dirty zone states and persists them to flash.

use embassy_time::{Duration, Timer};
use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash};

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
    F: NorFlash + MultiwriteNorFlash,
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
    F: NorFlash + MultiwriteNorFlash,
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
