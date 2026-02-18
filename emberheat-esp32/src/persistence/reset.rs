//! Factory reset functionality
//!
//! Provides functions to erase all persisted data and reset to defaults.

use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash};
use rs_matter::error::Error;
use rs_matter::Matter;

use crate::config::NUM_ZONES;
use crate::matter::zones::ZONE_STATES;
use emberheat_matter::types::{defaults, SystemMode};

use super::flash_store::FlashKvBlobStore;
use super::types::zone_key;

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
    F: NorFlash + MultiwriteNorFlash,
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
    F: NorFlash + MultiwriteNorFlash,
{
    use rs_matter_embassy::stack::persist::KvBlobStore;

    log::info!("Resetting zone states...");

    // Remove zone keys from flash
    for zone_idx in 0..NUM_ZONES {
        let key = zone_key(zone_idx);
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
