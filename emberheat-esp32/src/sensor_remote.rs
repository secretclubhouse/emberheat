//! Remote temperature sensors fed over MQTT, and the per-zone sensor source.
//!
//! The MQTT task calls [`record_remote_reading`] for each incoming message; the
//! zone control loops read the zone's average through [`ZoneSensor`]. Which
//! source a zone uses is chosen at compile time by `config::SENSOR_SOURCE`.

use core::cell::RefCell;
use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::blocking_mutex::Mutex;
use embassy_time::Instant;
use emberheat_core::remote::{parse_temp_payload, ZoneReadings};
use emberheat_core::{SensorError, TemperatureReading, TemperatureSensor};
use log::warn;

use crate::config::{
    SensorSource, MAX_REMOTE_SENSORS_PER_ZONE, MOCK_TEMPERATURE_C, NUM_ZONES,
    REMOTE_SENSOR_MAX_AGE_SECS, SENSOR_SOURCE, ZONE_CONFIGS,
};
use crate::sensors::MockSensor;

type ZoneStore = Mutex<CriticalSectionRawMutex, RefCell<ZoneReadings<MAX_REMOTE_SENSORS_PER_ZONE>>>;

/// Latest readings per zone, written by the MQTT task and read by zone tasks.
static READINGS: [ZoneStore; NUM_ZONES] =
    [const { Mutex::new(RefCell::new(ZoneReadings::new())) }; NUM_ZONES];

/// Index of a zone in `ZONE_CONFIGS` by exact name.
pub fn zone_index_by_name(zone_name: &str) -> Option<usize> {
    ZONE_CONFIGS.iter().position(|z| z.name == zone_name)
}

/// Records an incoming sensor message for a zone.
///
/// An unknown zone, bad payload or rejected reading is logged at warn and ignored.
pub fn record_remote_reading(zone_name: &str, sensor_id: &str, payload: &[u8]) {
    let Some(zone_index) = zone_index_by_name(zone_name) else {
        warn!("Remote sensor reading for unknown zone '{}'", zone_name);
        return;
    };
    let Some(temp_c) = parse_temp_payload(payload) else {
        warn!(
            "[{}] Unparseable payload from remote sensor '{}'",
            zone_name, sensor_id
        );
        return;
    };
    let now = Instant::now().as_secs();
    let result = READINGS[zone_index].lock(|r| r.borrow_mut().record(sensor_id, temp_c, now));
    if let Err(e) = result {
        warn!(
            "[{}] Rejected reading from remote sensor '{}': {}",
            zone_name, sensor_id, e
        );
    }
}

/// Zone temperature from the average of fresh remote sensor readings.
pub struct RemoteSensor {
    zone_index: usize,
    name: &'static str,
    last_count: AtomicU8,
}

impl RemoteSensor {
    pub fn new(zone_index: usize) -> Self {
        Self {
            zone_index,
            name: ZONE_CONFIGS[zone_index].name,
            last_count: AtomicU8::new(0),
        }
    }
}

impl TemperatureSensor for RemoteSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        let now = Instant::now().as_secs();
        let result =
            READINGS[self.zone_index].lock(|r| r.borrow().average(now, REMOTE_SENSOR_MAX_AGE_SECS));
        match result {
            Ok((reading, count)) => {
                self.last_count.store(count as u8, Ordering::Relaxed);
                Ok(reading)
            }
            Err(e) => {
                self.last_count.store(0, Ordering::Relaxed);
                Err(e)
            }
        }
    }

    fn id(&self) -> &str {
        self.name
    }
}

/// The sensor a zone task reads, chosen by `SENSOR_SOURCE`.
///
/// An enum rather than a trait object because Embassy tasks need concrete types.
pub enum ZoneSensor {
    Mock(MockSensor),
    Remote(RemoteSensor),
}

impl ZoneSensor {
    pub fn for_zone(zone_index: usize) -> Self {
        match SENSOR_SOURCE {
            SensorSource::Mock => Self::Mock(MockSensor::new(
                MOCK_TEMPERATURE_C,
                ZONE_CONFIGS[zone_index].name,
            )),
            SensorSource::Mqtt => Self::Remote(RemoteSensor::new(zone_index)),
        }
    }

    /// Sensors behind the last successful read (Mock: 1).
    pub fn sensors_used(&self) -> u8 {
        match self {
            Self::Mock(_) => 1,
            Self::Remote(r) => r.last_count.load(Ordering::Relaxed),
        }
    }
}

impl TemperatureSensor for ZoneSensor {
    async fn read_temperature(&self) -> Result<TemperatureReading, SensorError> {
        match self {
            Self::Mock(s) => s.read_temperature().await,
            Self::Remote(s) => s.read_temperature().await,
        }
    }

    fn id(&self) -> &str {
        match self {
            Self::Mock(s) => s.id(),
            Self::Remote(s) => s.id(),
        }
    }
}
