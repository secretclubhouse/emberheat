//! Flash-backed KvBlobStore implementation
//!
//! Implements the `KvBlobStore` trait from rs-matter-stack using
//! `sequential-storage` for wear-leveled flash storage.

use core::ops::Range;

use embedded_storage_async::nor_flash::{MultiwriteNorFlash, NorFlash};
use rs_matter::error::{Error, ErrorCode};
use rs_matter_embassy::stack::persist::KvBlobStore;
use sequential_storage::cache::NoCache;
use sequential_storage::map::{fetch_item, remove_item, store_item, SerializationError};

use super::types::{zone_key, PersistedZoneState};

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
    F: NorFlash + MultiwriteNorFlash,
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
    pub async fn load_zone(
        &mut self,
        zone_index: usize,
    ) -> Result<Option<PersistedZoneState>, Error> {
        let key = StorageKey(zone_key(zone_index));
        // Buffer size needs to be large enough for the value plus sequential-storage overhead
        let mut buf = [0u8; 64];

        match fetch_item::<StorageKey, &[u8], _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &key,
        )
        .await
        {
            Ok(Some(data)) => {
                if data.len() >= PersistedZoneState::SIZE {
                    Ok(PersistedZoneState::from_bytes(data))
                } else {
                    log::warn!(
                        "Zone {} data too short: {} bytes (expected {})",
                        zone_index,
                        data.len(),
                        PersistedZoneState::SIZE
                    );
                    Ok(None)
                }
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

        store_item::<StorageKey, &[u8], _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut buf,
            &key,
            &data.as_slice(),
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
    F: NorFlash + MultiwriteNorFlash,
{
    async fn load<CB>(&mut self, key: u16, buf: &mut [u8], cb: CB) -> Result<(), Error>
    where
        CB: FnOnce(Option<&[u8]>) -> Result<(), Error>,
    {
        let storage_key = StorageKey(key);
        let mut scratch = [0u8; 256]; // Larger buffer for Matter data

        match fetch_item::<StorageKey, &[u8], _>(
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
        let mut scratch = [0u8; 256]; // Larger buffer for Matter data

        store_item::<StorageKey, &[u8], _>(
            &mut self.flash,
            self.flash_range.clone(),
            &mut NoCache::new(),
            &mut scratch,
            &storage_key,
            &data,
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
        match remove_item::<StorageKey, _>(
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
