# MQTT Remote Zone Sensors Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Each zone takes its temperature from one or more room sensors published over MQTT by Home Assistant, and keeps the room near its usual temperature for up to 6 hours if those readings stop.

**Architecture:** All decision logic (reading store and averaging, topic/payload parsing, duty tracking, fallback cycle) is pure code in `emberheat-core` with unit tests. The firmware adds a per-zone static store fed by the MQTT task, a `ZoneSensor` enum so the static Embassy tasks can use either the mock or the remote source, and a receive path in the existing MQTT client. The two control loops in `main.rs` gain a fallback branch and stop blocking on the MQTT queue.

**Tech Stack:** Rust 2021, `no_std`; `heapless` 0.9; Embassy (`embassy-time`, `embassy-sync`, `embassy-futures`); `rust-mqtt` 0.6 (v5, `bump`); rs-matter via the project forks.

**Spec:** `docs/plans/2026-11-14-mqtt-remote-sensors-design.md`

## Global Constraints

- `emberheat-core` stays `no_std`: no `std` types outside `#[cfg(test)]`, no allocation in the new modules.
- Do not change `HysteresisConfig` values or `apply_hysteresis_control`.
- Relays are active-low: ON = `set_low()`, OFF = `set_high()`. Any error not explicitly covered by fallback leaves the relay OFF.
- Valid reading range is `0.0..=37.0` °C (`MIN_VALID_TEMP_C = 0.0`, `MAX_VALID_TEMP_C = 37.0`).
- Per-sensor staleness: 600 s. Max sensors per zone: 4. Sensor id max 24 bytes.
- Fallback: `max_secs = 21_600`, `period_secs = 1_800`, `max_duty = 0.75`, `min_on_secs = 180`, duty time constant 7200 s. Duty history is RAM only, never persisted.
- MQTT: QoS 0, never retained; subscribe filter `thermostat/<device_id>/zone/+/sensor/+`; messages flagged retained are ignored.
- `SENSOR_SOURCE` defaults to `Mock` until Josh has bench-tested `Mqtt`.
- Work on branch `feat/mqtt-remote-sensors`. Commit per task; do not push.
- Firmware has no host tests. "Firmware builds" in this plan means all three succeed: `make build-esp32`, `make build-matter`, `make build-matter-dual-radio`.
- Both `zone_control_loop` functions in `emberheat-esp32/src/main.rs` (`#[cfg(not(feature = "matter"))]` at line 197 and `#[cfg(feature = "matter")]` at line 336) get every control-loop change. `controller.rs` is not spawned; leave it alone.

## Review Focus

1. Home Assistant sends a payload with surrounding whitespace or a non-number (`" 21.4\n"`, `"unavailable"`, `"nan"`, `""`): whitespace is trimmed and accepted; the rest are dropped without touching the stored value. (Task 2)
2. A sensor is published in °F by mistake (`70.5`): rejected as out of range, and a zone whose only sensor does this behaves like a zone with no readings rather than averaging garbage. (Task 2)
3. Readings stop minutes after boot: the duty estimate is near zero, so fallback must leave the relay OFF rather than cycle on an invented duty. (Task 3)
4. A topic that nearly matches (`…/zone/Zone1/sensor/a/b`, empty sensor id, another device's id, the firmware's own `sensor_status` topic): ignored. (Task 2)
5. The thermostat is switched to Off in Apple Home while a zone is in fallback: relay goes OFF on the next pass and stays OFF. (Task 7; no host test possible, so it is a named bench step in Task 8.)

---

### Task 1: Lower the minimum valid temperature to 0 °C

**Files:**
- Modify: `emberheat-core/src/types.rs:85` and its test at `:199`
- Modify: `emberheat-core/src/control.rs:252` (test)
- Modify: `CLAUDE.md:162`

- [ ] **Step 1: Change the tests first**

In `types.rs` `test_temperature_validation`, replace the `too_cold` case:

```rust
assert!(TemperatureReading::from_celsius(0.0).is_valid());
assert!(TemperatureReading::from_celsius(2.0).is_valid());
assert!(!TemperatureReading::from_celsius(-0.1).is_valid());
```

In `control.rs` `test_validate_temperature_too_cold`, use `-1.0` instead of `2.0`.

- [ ] **Step 2: Run** `cargo test -p emberheat-core validation` — Expected: FAIL (2.0 is still invalid).
- [ ] **Step 3: Set `MIN_VALID_TEMP_C: f32 = 0.0`** and update its doc comment to say why: below freezing the heating has already failed, so the reading is treated as a fault.
- [ ] **Step 4: Run** `make test` — Expected: PASS.
- [ ] **Step 5: Update `CLAUDE.md`** "Sensor Validation": minimum 0°C.
- [ ] **Step 6: Commit** `fix: accept room readings down to 0C`

---

### Task 2: Reading store, averaging and message parsing (core)

**Files:**
- Create: `emberheat-core/src/remote.rs`
- Modify: `emberheat-core/src/lib.rs` (add `pub mod remote;`, module list in the crate doc)

**Interfaces — Produces:**

```rust
pub const MAX_SENSOR_ID_LEN: usize = 24;

pub struct ZoneReadings<const N: usize> { /* private */ }
impl<const N: usize> ZoneReadings<N> {
    pub const fn new() -> Self;
    pub fn record(&mut self, sensor_id: &str, temp_c: f32, now_secs: u64) -> Result<(), SensorError>;
    pub fn average(&self, now_secs: u64, max_age_secs: u64) -> Result<(TemperatureReading, usize), SensorError>;
}

/// `thermostat/<device_id>/zone/<zone>/sensor/<sensor_id>` -> (zone, sensor_id)
pub fn parse_sensor_topic<'a>(topic: &'a str, device_id: &str) -> Option<(&'a str, &'a str)>;

/// ASCII decimal °C, surrounding ASCII whitespace allowed. Finite values only.
pub fn parse_temp_payload(payload: &[u8]) -> Option<f32>;
```

`record` errors: non-finite → `InvalidData`; outside `MIN_VALID_TEMP_C..=MAX_VALID_TEMP_C` → `OutOfRange(temp_c)`; id empty or longer than 24 bytes → `InvalidData`; no usable slot → `Unavailable`. Slot choice in order: same id, free slot, the stalest slot older than 600 s (pass staleness as a `const STALE_SLOT_SECS: u64 = 600` in this module), else reject. Ages use `now_secs.saturating_sub(received_at_secs)`.

- [ ] **Step 1: Write the failing tests** in `remote.rs` `#[cfg(test)] mod tests`:

```rust
#[test] fn single_sensor_average() {
    let mut z = ZoneReadings::<4>::new();
    z.record("a", 21.4, 100).unwrap();
    let (r, n) = z.average(100, 600).unwrap();
    assert_eq!((r.celsius, n), (21.4, 1));
}
#[test] fn averages_fresh_sensors_only() {
    let mut z = ZoneReadings::<4>::new();
    z.record("a", 20.0, 0).unwrap();
    z.record("b", 22.0, 500).unwrap();
    assert_eq!(z.average(600, 600).unwrap().1, 2);          // a is exactly 600 s old: still fresh
    let (r, n) = z.average(601, 600).unwrap();               // a aged out
    assert_eq!((r.celsius, n), (22.0, 1));
}
#[test] fn all_stale_is_unavailable() {
    let mut z = ZoneReadings::<4>::new();
    z.record("a", 20.0, 0).unwrap();
    assert!(matches!(z.average(601, 600), Err(SensorError::Unavailable(_))));
    assert!(matches!(ZoneReadings::<4>::new().average(0, 600), Err(SensorError::Unavailable(_))));
}
#[test] fn same_id_updates_in_place() {
    let mut z = ZoneReadings::<1>::new();
    z.record("a", 20.0, 0).unwrap();
    z.record("a", 21.0, 10).unwrap();
    assert_eq!(z.average(10, 600).unwrap().0.celsius, 21.0);
}
#[test] fn invalid_values_keep_previous_reading() {
    let mut z = ZoneReadings::<4>::new();
    z.record("a", 20.0, 0).unwrap();
    assert!(matches!(z.record("a", 70.5, 10), Err(SensorError::OutOfRange(_))));   // °F by mistake
    assert!(matches!(z.record("a", -0.1, 10), Err(SensorError::OutOfRange(_))));
    assert!(matches!(z.record("a", f32::NAN, 10), Err(SensorError::InvalidData(_))));
    assert!(matches!(z.record("a", f32::INFINITY, 10), Err(SensorError::InvalidData(_))));
    assert_eq!(z.average(10, 600).unwrap().0.celsius, 20.0);
    z.record("a", 0.0, 20).unwrap();                                               // boundary is valid
}
#[test] fn only_out_of_range_sensor_leaves_zone_unavailable() {
    let mut z = ZoneReadings::<4>::new();
    let _ = z.record("a", 70.5, 0);
    assert!(z.average(0, 600).is_err());
}
#[test] fn full_store_rejects_new_sensor_until_a_slot_is_stale() {
    let mut z = ZoneReadings::<2>::new();
    z.record("a", 20.0, 0).unwrap();
    z.record("b", 20.0, 300).unwrap();
    assert!(matches!(z.record("c", 20.0, 400), Err(SensorError::Unavailable(_))));
    z.record("c", 24.0, 601).unwrap();                       // replaces a (stale), not b
    let (r, n) = z.average(601, 600).unwrap();
    assert_eq!((r.celsius, n), (22.0, 2));
}
#[test] fn rejects_bad_ids() {
    let mut z = ZoneReadings::<4>::new();
    assert!(z.record("", 20.0, 0).is_err());
    assert!(z.record("x234567890123456789012345", 20.0, 0).is_err());   // 25 bytes
    assert!(z.record("x23456789012345678901234", 20.0, 0).is_ok());     // 24 bytes
}
#[test] fn clock_going_backwards_does_not_panic() {
    let mut z = ZoneReadings::<4>::new();
    z.record("a", 20.0, 100).unwrap();
    assert!(z.average(50, 600).is_ok());
}
#[test] fn topic_parsing() {
    let id = "emberheat-esp32";
    assert_eq!(parse_sensor_topic("thermostat/emberheat-esp32/zone/Zone1/sensor/eve", id), Some(("Zone1", "eve")));
    for bad in [
        "thermostat/other/zone/Zone1/sensor/eve",
        "thermostat/emberheat-esp32/zone/Zone1/sensor/a/b",
        "thermostat/emberheat-esp32/zone/Zone1/sensor/",
        "thermostat/emberheat-esp32/zone//sensor/eve",
        "thermostat/emberheat-esp32/zone/Zone1/sensor_status",
        "thermostat/emberheat-esp32/temp/Zone1",
        "",
    ] { assert_eq!(parse_sensor_topic(bad, id), None, "{bad}"); }
}
#[test] fn payload_parsing() {
    assert_eq!(parse_temp_payload(b"21.4"), Some(21.4));
    assert_eq!(parse_temp_payload(b" 21.4\n"), Some(21.4));
    assert_eq!(parse_temp_payload(b"-3"), Some(-3.0));
    for bad in [&b""[..], b"unavailable", b"nan", b"inf", b"21.4C", b"\xff\xfe"] {
        assert_eq!(parse_temp_payload(bad), None);
    }
}
```

- [ ] **Step 2: Run** `cargo test -p emberheat-core remote` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement `remote.rs`** to the interface above. Slots are `[Option<Slot>; N]` with `Slot { sensor_id: heapless::String<24>, temp_c: f32, received_at_secs: u64 }`. `parse_temp_payload` goes through `core::str::from_utf8`, `trim_ascii`, `parse::<f32>`, then `is_finite` (Rust's parser accepts `nan`/`inf`, so the finite check is what rejects them).
- [ ] **Step 4: Run** `make test` and `cargo build -p emberheat-core --no-default-features` — Expected: PASS, builds.
- [ ] **Step 5: Commit** `feat: add remote sensor reading store to core`

---

### Task 3: Duty tracking and fallback cycle (core)

**Files:**
- Create: `emberheat-core/src/fallback.rs`
- Modify: `emberheat-core/src/lib.rs` (add `pub mod fallback;`)

**Interfaces — Produces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallbackConfig { pub max_secs: u64, pub period_secs: u64, pub max_duty: f32, pub min_on_secs: u64 }
impl FallbackConfig { pub const DEFAULT: Self; }   // 21_600, 1_800, 0.75, 180; also `impl Default`

pub const DUTY_TIME_CONSTANT_SECS: f32 = 7200.0;

pub struct DutyTracker { /* duty: f32 */ }
impl DutyTracker {
    pub const fn new() -> Self;                       // duty 0.0
    pub fn update(&mut self, relay_on: bool, dt_secs: u32);
    pub fn duty(&self) -> f32;
}

/// None once elapsed_secs > cfg.max_secs.
pub fn fallback_relay(duty: f32, elapsed_secs: u64, cfg: &FallbackConfig) -> Option<bool>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FallbackDecision { Relay(bool), Expired, NoHistory }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorStatus { Ok, Fallback, Lost }
impl SensorStatus { pub const fn as_str(self) -> &'static str; }   // "ok" / "fallback" / "lost"

pub struct FallbackController { /* tracker, lost_at_secs: Option<u64>, has_history: bool */ }
impl FallbackController {
    pub const fn new() -> Self;
    /// Call on every pass that had a good reading, after the relay was set.
    pub fn on_reading(&mut self, relay_on: bool, dt_secs: u32);
    /// Call on every pass whose reading was `SensorError::Unavailable`.
    pub fn on_unavailable(&mut self, now_secs: u64, cfg: &FallbackConfig) -> FallbackDecision;
    pub fn duty(&self) -> f32;
}
```

Algorithm the signatures do not determine:

- `update`: `alpha = (dt_secs as f32 / DUTY_TIME_CONSTANT_SECS).min(1.0); duty += alpha * (x - duty)` with `x` 1.0 or 0.0.
- `fallback_relay`: `d = duty` clamped to `0.0..=cfg.max_duty`, non-finite treated as 0; `on_secs = (d * period_secs as f32) as u64`; if `on_secs < min_on_secs` then `on_secs = 0`; result is `elapsed_secs % period_secs < on_secs`.
- `on_reading` clears `lost_at_secs`, sets `has_history`, updates the tracker. `on_unavailable` returns `NoHistory` when `!has_history`; otherwise records `lost_at_secs` on first call, and maps `fallback_relay(duty, now.saturating_sub(lost_at), cfg)` to `Relay(_)` / `Expired`. The tracker is not updated in `on_unavailable`.

- [ ] **Step 1: Write the failing tests:**

```rust
const C: FallbackConfig = FallbackConfig::DEFAULT;

#[test] fn tracker_starts_at_zero_and_converges() {
    let mut t = DutyTracker::new();
    assert_eq!(t.duty(), 0.0);
    for _ in 0..1200 { t.update(true, 6); }                  // 2 h on
    assert!((t.duty() - 0.632).abs() < 0.01);
    for _ in 0..6000 { t.update(true, 6); }                  // +10 h on
    assert!(t.duty() > 0.99 && t.duty() <= 1.0);
    for _ in 0..7200 { t.update(false, 6); }
    assert!(t.duty() < 0.01);
}
#[test] fn tracker_follows_a_half_duty_pattern() {
    let mut t = DutyTracker::new();
    for i in 0..12_000 { t.update((i / 100) % 2 == 0, 6); }   // 10 min on, 10 min off, 20 h
    assert!((t.duty() - 0.5).abs() < 0.05);
}
#[test] fn cycle_edges() {
    assert_eq!(fallback_relay(0.5, 0, &C), Some(true));
    assert_eq!(fallback_relay(0.5, 899, &C), Some(true));
    assert_eq!(fallback_relay(0.5, 900, &C), Some(false));
    assert_eq!(fallback_relay(0.5, 1799, &C), Some(false));
    assert_eq!(fallback_relay(0.5, 1800, &C), Some(true));
}
#[test] fn duty_is_capped() {
    assert_eq!(fallback_relay(1.0, 1349, &C), Some(true));
    assert_eq!(fallback_relay(1.0, 1350, &C), Some(false));
}
#[test] fn short_on_time_is_skipped() {
    assert_eq!(fallback_relay(0.09, 0, &C), Some(false));     // 162 s < 180 s
    assert_eq!(fallback_relay(0.11, 0, &C), Some(true));      // 198 s
    assert_eq!(fallback_relay(0.0, 0, &C), Some(false));
    assert_eq!(fallback_relay(f32::NAN, 0, &C), Some(false));
    assert_eq!(fallback_relay(-1.0, 0, &C), Some(false));
}
#[test] fn expires_after_six_hours() {
    assert!(fallback_relay(0.5, 21_600, &C).is_some());
    assert_eq!(fallback_relay(0.5, 21_601, &C), None);
}
#[test] fn controller_without_history_never_heats() {
    let mut c = FallbackController::new();
    assert_eq!(c.on_unavailable(1000, &C), FallbackDecision::NoHistory);
}
#[test] fn controller_with_minutes_of_history_stays_off() {
    let mut c = FallbackController::new();
    for _ in 0..50 { c.on_reading(true, 6); }                 // 5 min of heating after boot
    assert_eq!(c.on_unavailable(1000, &C), FallbackDecision::Relay(false));
}
#[test] fn controller_cycles_then_expires_then_recovers() {
    let mut c = FallbackController::new();
    for i in 0..12_000 { c.on_reading((i / 100) % 2 == 0, 6); }
    assert_eq!(c.on_unavailable(50_000, &C), FallbackDecision::Relay(true));    // elapsed 0
    assert_eq!(c.on_unavailable(51_700, &C), FallbackDecision::Relay(false));   // elapsed 1700
    assert_eq!(c.on_unavailable(71_601, &C), FallbackDecision::Expired);        // elapsed 21_601
    c.on_reading(false, 6);
    assert_eq!(c.on_unavailable(80_000, &C), FallbackDecision::Relay(true));    // a new outage restarts the clock
}
#[test] fn duty_is_frozen_during_fallback() {
    let mut c = FallbackController::new();
    for _ in 0..1200 { c.on_reading(true, 6); }
    let d = c.duty();
    for s in 0..100 { c.on_unavailable(10_000 + s * 6, &C); }
    assert_eq!(c.duty(), d);
}
#[test] fn status_strings() {
    assert_eq!(SensorStatus::Ok.as_str(), "ok");
    assert_eq!(SensorStatus::Fallback.as_str(), "fallback");
    assert_eq!(SensorStatus::Lost.as_str(), "lost");
}
```

- [ ] **Step 2: Run** `cargo test -p emberheat-core fallback` — Expected: FAIL (module missing).
- [ ] **Step 3: Implement `fallback.rs`** to the interface and algorithm above. Module doc explains why fallback exists (Josh, 2026-11-14: a slightly warm room beats a cold one after an overnight outage) and that it is bounded.
- [ ] **Step 4: Run** `make test` and `cargo build -p emberheat-core --no-default-features` — Expected: PASS, builds.
- [ ] **Step 5: Commit** `feat: add duty-cycle fallback logic to core`

---

### Task 4: Control loops never block on MQTT, and sensor errors reach Matter

**Files:**
- Modify: `emberheat-esp32/src/main.rs` (both `zone_control_loop`s)
- Modify: `emberheat-esp32/src/matter/bridge.rs`, `emberheat-esp32/src/matter/mod.rs` (re-export)

**Interfaces — Produces:**

```rust
// bridge.rs
/// Report a pass that had no temperature reading: LocalTemperature becomes null
/// (temperature::NULL_TEMPERATURE) and the running state follows the relay.
pub fn update_matter_without_reading(zone_index: usize, heating_active: bool);

// main.rs
/// Queue a message for MQTT, dropping it when the queue is full so control never waits on the network.
fn queue_mqtt(channel: &'static Channel<CriticalSectionRawMutex, mqtt::MqttMessage, MQTT_QUEUE_SIZE>, msg: mqtt::MqttMessage);   // try_send, ignore Full
```

- [ ] **Step 1: In both loops**, move the temperature publish to after the relay update and send it with `queue_mqtt` instead of `mqtt_channel.send(msg).await`. Nothing in either loop may `await` the channel.
- [ ] **Step 2: Add `update_matter_without_reading`** (re-exported from `matter/mod.rs`) and call it with `state.heating_active` at the end of the Matter loop's `Err` arm, after the relay has been turned off, so Apple Home shows no temperature and idle.
- [ ] **Step 3: Verify** firmware builds; `make test` passes.
- [ ] **Step 4: Commit** `fix: keep zone control running when MQTT is down`

---

### Task 5: Remote sensor source in the firmware

**Files:**
- Create: `emberheat-esp32/src/sensor_remote.rs`
- Modify: `emberheat-esp32/src/config.rs` (replace `USE_MOCK_SENSORS`, add constants, update the config test)
- Modify: `emberheat-esp32/src/main.rs` (module decl, sensor types at lines 114–146, 199, 338, 565–825, 858, 1106–1202)
- Modify: `CLAUDE.md:16`, `README.md:123` (mention `SENSOR_SOURCE`)

**Interfaces — Consumes:** `ZoneReadings`, `parse_temp_payload` (Task 2).

**Interfaces — Produces:**

```rust
// config.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorSource { Mock, Mqtt }
pub const SENSOR_SOURCE: SensorSource = SensorSource::Mock;
pub const REMOTE_SENSOR_MAX_AGE_SECS: u64 = 600;
pub const MAX_REMOTE_SENSORS_PER_ZONE: usize = 4;
pub const REMOTE_FALLBACK: emberheat_core::fallback::FallbackConfig = emberheat_core::fallback::FallbackConfig::DEFAULT;

// sensor_remote.rs
/// Index of a zone in ZONE_CONFIGS by exact name. Replaces the "ZoneN" parsing in the Matter loop.
pub fn zone_index_by_name(zone_name: &str) -> Option<usize>;
/// Called by the MQTT task for each incoming sensor message. Unknown zone, bad payload
/// or a rejected reading is logged at warn and ignored.
pub fn record_remote_reading(zone_name: &str, sensor_id: &str, payload: &[u8]);
pub struct RemoteSensor { /* zone_index, name, last_count: AtomicU8 */ }
impl RemoteSensor { pub fn new(zone_index: usize) -> Self; }
impl TemperatureSensor for RemoteSensor { /* average or SensorError::Unavailable */ }

pub enum ZoneSensor { Mock(MockSensor), Remote(RemoteSensor) }
impl ZoneSensor {
    pub fn for_zone(zone_index: usize) -> Self;      // picks by SENSOR_SOURCE; Mock uses MOCK_TEMPERATURE_C
    /// Sensors behind the last successful read (Mock: 1).
    pub fn sensors_used(&self) -> u8;
}
impl TemperatureSensor for ZoneSensor { /* delegate */ }
```

Store: `static READINGS: [Mutex<CriticalSectionRawMutex, RefCell<ZoneReadings<MAX_REMOTE_SENSORS_PER_ZONE>>>; NUM_ZONES]` (`embassy_sync::blocking_mutex::Mutex`). Time is `embassy_time::Instant::now().as_secs()`.

- [ ] **Step 1: Add the config items**; delete `USE_MOCK_SENSORS`; the startup log at `main.rs:858` prints `"Mock"` or `"MQTT"`.
- [ ] **Step 2: Write `sensor_remote.rs`** to the interface above.
- [ ] **Step 3: Retype** every `MockSensor` static cell, task parameter and loop parameter in `main.rs` to `ZoneSensor`, and initialise each with `ZoneSensor::for_zone(i)`. Replace the `strip_prefix("Zone")` index parsing in the Matter loop with `zone_index_by_name(zone_name).unwrap_or(0)`.
- [ ] **Step 4: Add `"sensors":N`** to the JSON built by `build_temperature_message` (new `sensors: u8` parameter fed from `sensor.sensors_used()`).
- [ ] **Step 5: Verify** firmware builds with `SENSOR_SOURCE = Mock`, then temporarily with `Mqtt` (`make build-matter-dual-radio`), then set it back to `Mock`. `make test` passes.
- [ ] **Step 6: Commit** `feat: add MQTT remote sensor source for zones`

---

### Task 6: MQTT task subscribes and receives

**Files:**
- Modify: `emberheat-esp32/src/mqtt.rs` (`mqtt_connect_and_publish`, lines 236–363)

**Interfaces — Consumes:** `emberheat_core::remote::parse_sensor_topic`; `sensor_remote::record_remote_reading` (Task 5).

Read `~/.cargo/registry/src/*/rust-mqtt-0.6.0/examples/demo.rs` and `src/client/mod.rs` (`subscribe`, `poll_header`, `poll_body`, `ping`) before starting; the demo selects on `poll_header` the same way.

- [ ] **Step 1: CONNECT** adds `.maximum_packet_size(NonZero::new(512))` so an oversized message on a matching topic cannot overflow the 1024-byte bump buffer.
- [ ] **Step 2: After CONNACK**: subscribe to `thermostat/<device_id>/zone/+/sensor/+` at QoS 0 with `RetainHandling::NeverSend`; poll until the `Event::Suback` arrives (10 s timeout → `Err("No SUBACK")`, failure reason code → `Err("Subscribe rejected")`); only then publish `{"status":"online"}`.
- [ ] **Step 3: Replace the send-only loop** with `embassy_futures::select::select3` over `rx_channel.receive()`, `client.poll_header()`, and a 30 s `Timer`:
  - queue message → `publish_qos0` to completion (as today, error returns `Err`);
  - header → `client.poll_body(header)` to completion; on `Event::Publish` with `retain == false`, `parse_sensor_topic(topic, config.device_id)` then `record_remote_reading(zone, sensor_id, payload)`; retained publishes and non-matching topics are dropped at `debug`; other events ignored;
  - timer → `client.ping()`.
  `poll_header` is the only future dropped by the select; `poll_body`, `publish` and `ping` always run to completion inside their arm.
- [ ] **Step 4: Reset the bump buffer** (`unsafe { client.buffer_mut().reset() }`) after every poll event, once the event and anything borrowing it are dropped. Any client error returns `Err` so the existing backoff loop reconnects.
- [ ] **Step 5: Move** the 10 ms `Timer::after` into the queue-message arm, so it paces outgoing publishes only and never delays receiving.
- [ ] **Step 6: Verify** firmware builds; `make test` passes.
- [ ] **Step 7: Commit** `feat: receive remote sensor readings over MQTT`

---

### Task 7: Fallback in the control loops

**Files:**
- Modify: `emberheat-esp32/src/main.rs` (both loops; new helper)

**Interfaces — Consumes:** `FallbackController`, `FallbackDecision`, `SensorStatus` (Task 3); `REMOTE_FALLBACK` (Task 5); `queue_mqtt`, `update_matter_without_reading` (Task 4).

**Interfaces — Produces:**

```rust
/// thermostat/<device_id>/zone/<zone>/sensor_status, payload "ok" | "fallback" | "lost"
fn build_sensor_status_message(zone_name: &str, status: SensorStatus) -> Option<mqtt::MqttMessage>;
```

Each loop owns `let mut fallback = FallbackController::new();` and `let mut status: Option<SensorStatus> = None;`. Per pass:

| Sensor result | Relay | Other effects | Status |
|---|---|---|---|
| `Ok(reading)` | hysteresis, as today | then `fallback.on_reading(relay_is_on, CONTROL_INTERVAL_SECS as u32)` | `Ok` |
| `Err(Unavailable)` → `Relay(on)` | `on`, except OFF when the Matter mode is not Heat (`get_control_target(zone_index).0 == HeatingState::Off`) | Matter: `update_matter_without_reading(zone_index, relay_is_on)` | `Fallback` |
| `Err(Unavailable)` → `Expired` or `NoHistory` | OFF | existing error path | `Lost` |
| any other `Err` | OFF | existing error path | `Lost` |

- [ ] **Step 1: Implement the table** in both loops. Relay writes go through the same set-and-check code as today (state only changes when the GPIO call succeeds). The MQTT-only loop has no mode check.
- [ ] **Step 2: Publish status on change**: when the pass's status differs from `status`, `queue_mqtt(build_sensor_status_message(..))`, log at `warn` for `Fallback`/`Lost` and `info` for `Ok` (include the duty for `Fallback`), then store it.
- [ ] **Step 3: Error counting**: the existing `increment_errors` / suppression logic applies to `Lost` passes only; `Fallback` passes log one line on entry, not every 6 s.
- [ ] **Step 4: Verify** firmware builds; `make test` passes.
- [ ] **Step 5: Commit** `feat: run zones on recent duty cycle when readings are lost`

---

### Task 8: Documentation and bench checklist

**Files:**
- Create: `docs/home-assistant-remote-sensors.md`
- Modify: `CLAUDE.md` (Safety Defaults, Project Structure, test counts), `README.md`

- [ ] **Step 1: Write `docs/home-assistant-remote-sensors.md`**: the topic contract, the per-zone automation and °F template copied verbatim from the spec's "Home Assistant side", why the `last_reported` guard matters, the `sensor_status` topic with an example alert automation on `fallback`/`lost`, and this bench checklist:
  1. Set `SENSOR_SOURCE = Mqtt`, flash with `make flash-monitor-matter-dual-radio`.
  2. `mosquitto_pub -t thermostat/<id>/zone/Zone1/sensor/test -m 19.0` every 2 min: relay follows the setpoint; Apple Home shows 19.0.
  3. Publish with `-r` (retained): the log shows it ignored.
  4. Publish `70.5` and `abc`: both logged and ignored.
  5. Temporarily set `REMOTE_FALLBACK` to `{ max_secs: 600, period_secs: 120, max_duty: 0.75, min_on_secs: 10 }` and `REMOTE_SENSOR_MAX_AGE_SECS = 60`; run with heating cycling for 10 min, stop publishing: `fallback` is published, Apple Home shows no temperature, the relay cycles, then `lost` and OFF after 10 min.
  6. During fallback, set the thermostat to Off in Apple Home: relay OFF within 6 s and stays OFF.
  7. Resume publishing during fallback: `ok` is published and normal control resumes.
  8. Stop the broker with heating ON: same fallback behaviour, and control keeps running.
  9. Reboot with nothing publishing: relay stays OFF.
  10. Restore the constants.
- [ ] **Step 2: Update `CLAUDE.md`**: Safety Defaults says heating is OFF on errors except lost remote readings, which run the bounded duty-cycle fallback (max 6 h, 75%); add `remote.rs`, `fallback.rs`, `sensor_remote.rs` to the structure; update the core test count from `make test` output.
- [ ] **Step 3: Verify** `make dev` passes.
- [ ] **Step 4: Commit** `docs: document MQTT remote sensors and fallback`
