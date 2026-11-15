//! Readings from remote (MQTT) temperature sensors for one zone.
//!
//! Remote sensors push readings at their own pace, so each zone keeps the
//! latest value per sensor and averages only the fresh ones. Pure logic with
//! no allocation, so the control loop and the MQTT task can share it.

use crate::sensor::SensorError;
use crate::types::{TemperatureReading, MAX_VALID_TEMP_C, MIN_VALID_TEMP_C};

/// Longest accepted sensor id, in bytes.
pub const MAX_SENSOR_ID_LEN: usize = 24;

/// A slot whose reading is older than this may be taken over by a new sensor.
const STALE_SLOT_SECS: u64 = 600;

fn msg(text: &str) -> heapless::String<128> {
    let mut s = heapless::String::new();
    // Messages are short literals; truncation on overflow is acceptable.
    let _ = s.push_str(text);
    s
}

#[derive(Debug, Clone)]
struct Slot {
    sensor_id: heapless::String<MAX_SENSOR_ID_LEN>,
    temp_c: f32,
    received_at_secs: u64,
}

impl Slot {
    fn age(&self, now_secs: u64) -> u64 {
        // Saturating so a clock that steps backwards reads as "just received".
        now_secs.saturating_sub(self.received_at_secs)
    }
}

/// Latest reading from each of up to `N` remote sensors in one zone.
#[derive(Debug, Clone)]
pub struct ZoneReadings<const N: usize> {
    slots: [Option<Slot>; N],
}

impl<const N: usize> Default for ZoneReadings<N> {
    fn default() -> Self {
        Self::new()
    }
}

// SensorError is the crate-wide error type; its size is not this module's to change.
#[allow(clippy::result_large_err)]
impl<const N: usize> ZoneReadings<N> {
    /// Creates an empty store.
    pub const fn new() -> Self {
        Self {
            slots: [const { None }; N],
        }
    }

    /// Records a reading, replacing the sensor's previous one.
    ///
    /// Invalid readings are rejected without touching the stored value, so a
    /// bad message never overwrites a good reading. A new sensor takes a free
    /// slot, else the stalest slot older than `STALE_SLOT_SECS`, else is rejected.
    pub fn record(
        &mut self,
        sensor_id: &str,
        temp_c: f32,
        now_secs: u64,
    ) -> Result<(), SensorError> {
        if sensor_id.is_empty() || sensor_id.len() > MAX_SENSOR_ID_LEN {
            return Err(SensorError::InvalidData(msg("invalid sensor id length")));
        }
        if !temp_c.is_finite() {
            return Err(SensorError::InvalidData(msg("non-finite temperature")));
        }
        if !(MIN_VALID_TEMP_C..=MAX_VALID_TEMP_C).contains(&temp_c) {
            return Err(SensorError::OutOfRange(temp_c));
        }

        let index = self
            .slots
            .iter()
            .position(|s| {
                s.as_ref()
                    .is_some_and(|s| s.sensor_id.as_str() == sensor_id)
            })
            .or_else(|| self.slots.iter().position(|s| s.is_none()))
            .or_else(|| {
                self.slots
                    .iter()
                    .enumerate()
                    .filter_map(|(i, s)| s.as_ref().map(|s| (i, s.age(now_secs))))
                    .filter(|&(_, age)| age > STALE_SLOT_SECS)
                    .max_by_key(|&(_, age)| age)
                    .map(|(i, _)| i)
            })
            .ok_or_else(|| SensorError::Unavailable(msg("no free remote sensor slot")))?;

        let mut id = heapless::String::new();
        id.push_str(sensor_id)
            .map_err(|_| SensorError::InvalidData(msg("invalid sensor id length")))?;
        self.slots[index] = Some(Slot {
            sensor_id: id,
            temp_c,
            received_at_secs: now_secs,
        });
        Ok(())
    }

    /// Averages readings no older than `max_age_secs`, returning the average
    /// and the number of sensors used. `Unavailable` when none are fresh.
    pub fn average(
        &self,
        now_secs: u64,
        max_age_secs: u64,
    ) -> Result<(TemperatureReading, usize), SensorError> {
        let (sum, count) = self
            .slots
            .iter()
            .flatten()
            .filter(|s| s.age(now_secs) <= max_age_secs)
            .fold((0.0f32, 0usize), |(sum, n), s| (sum + s.temp_c, n + 1));
        if count == 0 {
            return Err(SensorError::Unavailable(msg("no fresh remote readings")));
        }
        Ok((TemperatureReading::from_celsius(sum / count as f32), count))
    }
}

/// `thermostat/<device_id>/zone/<zone>/sensor/<sensor_id>` -> (zone, sensor_id)
pub fn parse_sensor_topic<'a>(topic: &'a str, device_id: &str) -> Option<(&'a str, &'a str)> {
    let rest = topic.strip_prefix("thermostat/")?;
    let rest = rest.strip_prefix(device_id)?;
    let rest = rest.strip_prefix("/zone/")?;
    let (zone, sensor_id) = rest.split_once("/sensor/")?;
    if zone.is_empty() || zone.contains('/') || sensor_id.is_empty() || sensor_id.contains('/') {
        return None;
    }
    Some((zone, sensor_id))
}

/// ASCII decimal °C, surrounding ASCII whitespace allowed. Finite values only.
pub fn parse_temp_payload(payload: &[u8]) -> Option<f32> {
    let text = core::str::from_utf8(payload.trim_ascii()).ok()?;
    // Rust's parser accepts "nan" and "inf", so the finite check rejects them.
    text.parse::<f32>().ok().filter(|t| t.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_sensor_average() {
        let mut z = ZoneReadings::<4>::new();
        z.record("a", 21.4, 100).unwrap();
        let (r, n) = z.average(100, 600).unwrap();
        assert_eq!((r.celsius, n), (21.4, 1));
    }
    #[test]
    fn averages_fresh_sensors_only() {
        let mut z = ZoneReadings::<4>::new();
        z.record("a", 20.0, 0).unwrap();
        z.record("b", 22.0, 500).unwrap();
        assert_eq!(z.average(600, 600).unwrap().1, 2); // a is exactly 600 s old: still fresh
        let (r, n) = z.average(601, 600).unwrap(); // a aged out
        assert_eq!((r.celsius, n), (22.0, 1));
    }
    #[test]
    fn all_stale_is_unavailable() {
        let mut z = ZoneReadings::<4>::new();
        z.record("a", 20.0, 0).unwrap();
        assert!(matches!(
            z.average(601, 600),
            Err(SensorError::Unavailable(_))
        ));
        assert!(matches!(
            ZoneReadings::<4>::new().average(0, 600),
            Err(SensorError::Unavailable(_))
        ));
    }
    #[test]
    fn same_id_updates_in_place() {
        let mut z = ZoneReadings::<1>::new();
        z.record("a", 20.0, 0).unwrap();
        z.record("a", 21.0, 10).unwrap();
        assert_eq!(z.average(10, 600).unwrap().0.celsius, 21.0);
    }
    #[test]
    fn invalid_values_keep_previous_reading() {
        let mut z = ZoneReadings::<4>::new();
        z.record("a", 20.0, 0).unwrap();
        assert!(matches!(
            z.record("a", 70.5, 10),
            Err(SensorError::OutOfRange(_))
        )); // °F by mistake
        assert!(matches!(
            z.record("a", -0.1, 10),
            Err(SensorError::OutOfRange(_))
        ));
        assert!(matches!(
            z.record("a", f32::NAN, 10),
            Err(SensorError::InvalidData(_))
        ));
        assert!(matches!(
            z.record("a", f32::INFINITY, 10),
            Err(SensorError::InvalidData(_))
        ));
        assert_eq!(z.average(10, 600).unwrap().0.celsius, 20.0);
        z.record("a", 0.0, 20).unwrap(); // boundary is valid
    }
    #[test]
    fn only_out_of_range_sensor_leaves_zone_unavailable() {
        let mut z = ZoneReadings::<4>::new();
        let _ = z.record("a", 70.5, 0);
        assert!(z.average(0, 600).is_err());
    }
    #[test]
    fn full_store_rejects_new_sensor_until_a_slot_is_stale() {
        let mut z = ZoneReadings::<2>::new();
        z.record("a", 20.0, 0).unwrap();
        z.record("b", 20.0, 300).unwrap();
        assert!(matches!(
            z.record("c", 20.0, 400),
            Err(SensorError::Unavailable(_))
        ));
        z.record("c", 24.0, 601).unwrap(); // replaces a (stale), not b
        let (r, n) = z.average(601, 600).unwrap();
        assert_eq!((r.celsius, n), (22.0, 2));
    }
    #[test]
    fn rejects_bad_ids() {
        let mut z = ZoneReadings::<4>::new();
        assert!(z.record("", 20.0, 0).is_err());
        assert!(z.record("x234567890123456789012345", 20.0, 0).is_err()); // 25 bytes
        assert!(z.record("x23456789012345678901234", 20.0, 0).is_ok()); // 24 bytes
    }
    #[test]
    fn clock_going_backwards_does_not_panic() {
        let mut z = ZoneReadings::<4>::new();
        z.record("a", 20.0, 100).unwrap();
        assert!(z.average(50, 600).is_ok());
    }
    #[test]
    fn topic_parsing() {
        let id = "emberheat-esp32";
        assert_eq!(
            parse_sensor_topic("thermostat/emberheat-esp32/zone/Zone1/sensor/eve", id),
            Some(("Zone1", "eve"))
        );
        for bad in [
            "thermostat/other/zone/Zone1/sensor/eve",
            "thermostat/emberheat-esp32/zone/Zone1/sensor/a/b",
            "thermostat/emberheat-esp32/zone/Zone1/sensor/",
            "thermostat/emberheat-esp32/zone//sensor/eve",
            "thermostat/emberheat-esp32/zone/Zone1/sensor_status",
            "thermostat/emberheat-esp32/temp/Zone1",
            "",
        ] {
            assert_eq!(parse_sensor_topic(bad, id), None, "{bad}");
        }
    }
    #[test]
    fn payload_parsing() {
        assert_eq!(parse_temp_payload(b"21.4"), Some(21.4));
        assert_eq!(parse_temp_payload(b" 21.4\n"), Some(21.4));
        assert_eq!(parse_temp_payload(b"-3"), Some(-3.0));
        for bad in [
            &b""[..],
            b"unavailable",
            b"nan",
            b"inf",
            b"21.4C",
            b"\xff\xfe",
        ] {
            assert_eq!(parse_temp_payload(bad), None);
        }
    }
}
