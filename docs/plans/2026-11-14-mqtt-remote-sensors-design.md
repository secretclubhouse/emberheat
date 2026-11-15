# Remote zone sensors over MQTT — design

Date: 2026-11-14
Status: approved by Josh 2026-11-14
Depends on: `chore/upgrade-deps` (rust-mqtt 0.6, which can subscribe)

## Intent

Zones currently read a mock sensor, and wired DS18B20 support (issues #1, #2) is
deprioritised. Josh wants each zone to take its temperature from sensors that sit
in the room rather than at the controller: sensors already in the house (Matter,
Thread, Zigbee, anything Home Assistant can see) and, later, custom ESP32 sensors.
A zone may have several sensors, averaged into one zone temperature.

Home Assistant with an MQTT broker is already running, so Home Assistant is the
bridge: it publishes sensor readings to MQTT and EmberHeat subscribes.

## Goals

- A zone's temperature comes from one or more readings received over MQTT.
- Multiple sensors per zone are averaged.
- Adding, removing or swapping a sensor is a Home Assistant change only. No
  firmware rebuild.
- Works in both firmware modes (Matter + MQTT, and MQTT-only).
- The mock sensor stays available for bench testing.

## Non-goals

- Reading Matter sensors directly (Matter binding / client).
- BLE broadcast or ESP-NOW sensors.
- Runtime zone configuration (issue #5), sensor auto-discovery (issue #6).
- Outlier rejection, weighting, or median filtering across sensors.
- TLS to the broker.
- Humidity or any non-temperature data.

## Anti-goals

- **Heating must never regulate against a stale reading as if it were fresh,
  and must never run unbounded without readings.** If all of a zone's readings
  stop (Home Assistant down, broker down, WiFi down, sensor battery dead), the
  zone runs the bounded fallback described below for at most 6 hours, then
  turns heating OFF. (Changed 2026-11-14 from "OFF after 10 minutes": Josh
  would rather wake to a slightly warm room than a cold one after an overnight
  outage.)
- No retained MQTT messages: a retained value looks fresh after a reconnect.

## Approaches

| Approach | Outcome |
|---|---|
| **MQTT via Home Assistant** (picked) | Smallest change; reuses the existing MQTT client and broker; covers every sensor Home Assistant can see. Cost: heating depends on Home Assistant, the broker and WiFi being up. |
| BLE broadcast sensors (BTHome) | No hub dependency, but only BLE sensors, and BLE scanning alongside WiFi + Matter on the C6's single radio is untested. Good follow-up if hub dependency becomes a problem. |
| ESP-NOW custom sensors | Custom hardware only; does not use existing sensors. |
| Direct Matter binding | Apple Home cannot set up bindings, and rs-matter has no client-side read support that we found. |

Josh picked MQTT via Home Assistant on 2026-11-13 because Home Assistant and a
broker are already running.

## Design

### MQTT contract

- Topic: `thermostat/<device_id>/zone/<zone_name>/sensor/<sensor_id>`
  - `<device_id>` is `MQTT_DEVICE_ID`.
  - `<zone_name>` is the zone's `name` in `ZONE_CONFIGS` (e.g. `Zone1`),
    matched exactly.
  - `<sensor_id>` is any short label chosen in Home Assistant (max 24 bytes),
    e.g. `living_room_eve`. It only needs to be unique within the zone.
- Payload: the temperature in °C as plain ASCII decimal, e.g. `21.4`.
  Home Assistant converts from °F in the publishing template if needed.
- QoS 0, not retained.
- The firmware subscribes once to `thermostat/<device_id>/zone/+/sensor/+`.
- Home Assistant republishes every reading every 2 minutes, and immediately
  when the firmware publishes `{"status":"online"}` on
  `thermostat/<device_id>/status`, so a rebooted controller gets readings
  without waiting. With a 10-minute cutoff this tolerates four lost QoS 0
  messages in a row (Claude's call; a 5-minute republish left room for only one).
- The firmware subscribes with retain handling `Never` and ignores any message
  flagged retained.

These are Claude's proposals; Josh has agreed to the topic shape, plain °C
payload, averaging, 10-minute staleness and no-retain in conversation.

### Core logic (`emberheat-core`, pure, unit tested)

New module `emberheat-core/src/remote.rs`:

- `ZoneReadings<const N: usize>`: up to `N` slots, each
  `{ sensor_id: heapless::String<24>, temp_c: f32, received_at_secs: u64 }`.
- `record(sensor_id, temp_c, now_secs) -> Result<(), SensorError>`
  - Rejects NaN/infinite and values outside `MIN_VALID_TEMP_C..=MAX_VALID_TEMP_C`
    (the reading is dropped, the previous value for that sensor is kept and
    ages out normally).
  - `MIN_VALID_TEMP_C` drops from 3 to 0 (Josh, 2026-11-14): readings from
    0 °C up are real and heat normally. Below 0 °C the reading is rejected and
    the zone ends up OFF; Josh's reasoning is that a room below freezing means
    the heating has already failed. This is the shared core constant, so it
    applies to every sensor type.
  - Updates the slot with the same id, else takes a free slot, else replaces a
    stale slot, else rejects the new sensor (all `N` slots hold fresh readings).
- `average(now_secs, max_age_secs) -> Result<(TemperatureReading, usize), SensorError>`
  - Averages slots whose age is `<= max_age_secs`; returns the count used.
  - No fresh slots → `SensorError::Unavailable`.

Time is passed in as seconds so the module has no clock dependency.

New module `emberheat-core/src/fallback.rs` (Josh picked "recent duty cycle"
on 2026-11-14; the numbers and mechanics below are Claude's calls):

- `DutyTracker`: a running estimate of how much of the recent past the relay
  was ON, as a fraction 0.0–1.0.
  - `update(relay_on, dt_secs)`: exponential moving average with a 2-hour time
    constant. Called once per control-loop pass, only while the zone is
    regulating on fresh readings; frozen during fallback.
  - Starts at 0.0 and is not persisted, so a short history underestimates and
    errs toward less heat.
- `fallback_relay(duty, elapsed_secs, &FallbackConfig) -> Option<bool>`
  - `None` once `elapsed_secs > max_secs` (6 h): the caller treats this as a
    sensor error and turns heating OFF.
  - Otherwise the relay cycles on a fixed 30-minute period: OFF first, then ON
    for the last `duty × period` of each period. Starting in the OFF phase
    means readings that keep dropping in and out of fallback err toward less
    heat instead of starting a fresh ON phase on every re-entry.
  - `duty` is capped at 0.75 so a zone that was recovering from a setback when
    readings stopped cannot run flat out for 6 hours.
  - An ON time under 3 minutes becomes 0 (not worth a relay cycle).
- `FallbackConfig { max_secs: 21_600, period_secs: 1_800, max_duty: 0.75,
  min_on_secs: 180 }` as the default.

The hysteresis algorithm and its values are untouched; fallback only replaces
it while there is no reading to feed it.

### Firmware (`emberheat-esp32`)

- `config.rs`
  - Replace `USE_MOCK_SENSORS: bool` with `SENSOR_SOURCE: SensorSource`
    (`Mock` | `Mqtt`). Default stays `Mock` until hardware-tested.
  - `REMOTE_SENSOR_MAX_AGE_SECS = 600`, `MAX_REMOTE_SENSORS_PER_ZONE = 4`.
  - `REMOTE_FALLBACK: FallbackConfig` with the defaults above.
- New `sensor_remote.rs`
  - A static per-zone store: `[Mutex<CriticalSectionRawMutex, RefCell<ZoneReadings<4>>>; NUM_ZONES]`.
  - `record_remote_reading(zone_name, sensor_id, payload)` — called by the MQTT
    task; parses the payload, finds the zone by name, records with
    `embassy_time::Instant::now().as_secs()`. Unknown zone or bad payload is
    logged at `warn` and ignored.
  - `RemoteSensor { zone_index }` implementing `TemperatureSensor`:
    `read_temperature()` returns the zone average or `Unavailable`.
- Zone tasks: both control loops in `main.rs` are hard-typed to `&'static
  MockSensor`. Embassy tasks cannot be generic, so introduce
  `enum ZoneSensor { Mock(MockSensor), Remote(RemoteSensor) }` implementing
  `TemperatureSensor`, and type the loops and static cells to it.
- Control loops (both, `main.rs`) — required fixes found in review:
  - **Publishing must not block control.** Both loops `await` a send on the
    16-slot MQTT queue *before* running hysteresis (`main.rs:252`, `:404`).
    With the broker down the queue fills in about 96 s and the loop stops with
    the relay frozen in its last state, so staleness would never be acted on.
    Switch to `try_send` (drop the message when full) and run relay control
    before publishing. Without this the anti-goal is not met.
  - **Sensor-error path must update Matter.** Today it turns the relay off but
    writes nothing to Matter, so `LocalTemperature` keeps the last value and
    the running state keeps showing "heating". Add a bridge function that sets
    `LocalTemperature` to null and the running state to idle, and call it from
    the error path.
  - **Fallback.** Each loop owns a `DutyTracker`. On a good reading it runs
    hysteresis as today and updates the tracker. On `Unavailable` it notes when
    readings were lost and drives the relay from `fallback_relay`; when that
    returns `None` it takes the sensor-error path (relay OFF). During fallback:
    `LocalTemperature` is null, the running state follows the actual relay,
    system mode Off still forces the relay OFF, and setpoint changes have no
    effect until a reading returns. The first fresh reading ends fallback and
    hysteresis resumes. A zone that has never had a reading since boot does not
    enter fallback (there is no history to replay) and stays OFF.
  - Other sensor errors (not `Unavailable`) keep today's behaviour: relay OFF.
  - Publish a zone sensor-status message (`ok` / `fallback` / `lost`) on every
    change, so Home Assistant can alert as soon as fallback starts rather than
    6 hours later (Claude's call).
- MQTT task (`mqtt.rs::mqtt_connect_and_publish`, shared by both modes)
  - After CONNECT: subscribe to the wildcard filter, wait for the SUBACK, and
    only then publish the `online` status. Today `online` is published first;
    Home Assistant's immediate republish would arrive before the subscription
    exists and be lost.
  - Set `maximum_packet_size` in the CONNECT so an oversized message on a
    matching topic cannot overflow the 1024-byte buffer and force a reconnect.
  - Replace the send-only loop with a `select` over: the outgoing queue,
    `client.poll_header()` (then `poll_body`), and a 30-second ping timer.
    The rust-mqtt demo selects on `poll_header` the same way.
  - `poll_header` is safe to cancel; `poll_body` and `publish` are not, so each
    runs to completion inside its select arm.
  - On an incoming publish: split the topic, call `record_remote_reading`.
    Reset the bump buffer after every event (not only publishes), once the
    event and any error borrowing it have been dropped.
  - Any client error returns from the function; the existing reconnect loop
    with backoff handles it. While disconnected, readings age out after
    `REMOTE_SENSOR_MAX_AGE_SECS` and zones enter fallback.
  - The client's current const generics already allow one subscription and
    QoS 0 receipt; no change expected.
- Status for Home Assistant: the per-zone temperature message the firmware
  already publishes carries the averaged value; add the number of sensors used.

### Behaviour summary

| Situation | Zone behaviour |
|---|---|
| Boot, no reading yet | Heating OFF until the first valid reading |
| 1 of 3 sensors goes quiet | Average of the remaining 2 after 10 min |
| All sensors quiet for 10 min | Fallback: relay cycles at the zone's recent duty (max 75%) on a 30-min period; Matter temperature null; status `fallback` published |
| A reading returns during fallback | Normal hysteresis control resumes immediately |
| Thermostat set to Off during fallback | Heating OFF |
| No readings for 6 h after fallback starts | Sensor error → heating OFF, status `lost` |
| Reboot with no readings arriving | Heating OFF (duty history is not kept across reboots) |
| Payload not a number / out of 0–37 °C | Reading ignored, logged |
| Broker or WiFi drops | MQTT reconnects with backoff; control loop keeps running; fallback after 10 min without readings |
| Fifth sensor for a zone | Ignored unless an existing slot is stale |

### Home Assistant side

Documented in `docs/`, not shipped as code. One automation per zone:

```yaml
alias: EmberHeat Zone1 sensors
triggers:
  - trigger: state
    entity_id: [sensor.living_room_temperature, sensor.hallway_temperature]
  - trigger: time_pattern
    minutes: "/2"
  - trigger: mqtt
    topic: thermostat/emberheat-esp32/status
actions:
  - repeat:
      for_each: [sensor.living_room_temperature, sensor.hallway_temperature]
      sequence:
        # Skip sensors that are unavailable or have not reported for 15 minutes,
        # so a dead sensor ages out in the firmware instead of being republished
        - if: >-
            {{ states(repeat.item) | is_number and
               (now() - states[repeat.item].last_reported).total_seconds() < 900 }}
          then:
            - action: mqtt.publish
              data:
                topic: "thermostat/emberheat-esp32/zone/Zone1/sensor/{{ repeat.item.split('.')[1] }}"
                payload: "{{ states(repeat.item) | float | round(2) }}"
                qos: 0
                retain: false
```

If Home Assistant runs in °F, the payload template converts:
`{{ ((states(repeat.item) | float - 32) * 5 / 9) | round(2) }}`.

The `last_reported` guard matters: the firmware measures when a message
arrived, not when the sensor last measured. Without the guard, Home Assistant
would keep republishing a dead sensor's last value and the firmware would treat
it as fresh.

## Testing

- `emberheat-core` unit tests for `ZoneReadings`: single sensor, average of
  several, stale exclusion at the boundary, all stale → error, invalid values
  rejected without disturbing the stored value, slot reuse and overflow.
- `emberheat-core` unit tests for fallback: tracker converges toward the real
  duty and starts at 0; cycle ON/OFF edges within a period; duty cap; minimum
  ON time; `None` past the 6-hour limit.
- Firmware: `make build-esp32`, `make build-matter`,
  `make build-matter-dual-radio` and the `scan_sensors` binary build; existing
  68 tests still pass.
- Hardware (Josh): with `SENSOR_SOURCE = Mqtt`, publish with `mosquitto_pub`,
  confirm the relay follows the target, then stop publishing and confirm that
  after 10 minutes the zone enters fallback, Matter shows no temperature, and
  the relay cycles. Repeat by shutting down the broker instead, with heating
  ON. For the bench, temporarily shorten the fallback period and limit in
  `config.rs` to see a full cycle and the final OFF without waiting 6 hours.
- `CLAUDE.md` safety section is updated to describe fallback, since "heating
  OFF on errors" no longer covers lost remote readings.

## Left to the builder

- Exact rust-mqtt const generic values and buffer sizes.
- Whether `ZoneSensor` lives in `sensors.rs` or `sensor_remote.rs`.
- Log wording and levels.
- Shape of the "sensors used" field in the published temperature message.
- PINGRESP timeout handling.
- One shared zone-name → index lookup (the Matter loop currently parses "ZoneN").

## Open questions for Josh

1. ~~Matter setpoint and mode are not wired into the control loop.~~ Resolved:
   done as a separate change before this work.
2. ~~What a zone does when all its readings stop.~~ Resolved 2026-11-14:
   recent-duty-cycle fallback for up to 6 hours, see Core logic. The
   Home Assistant `last_reported` guard stays as the dead-sensor check.
3. ~~Readings below 3 °C are rejected as invalid.~~ Resolved: the minimum
   drops to 0 °C, see Core logic.
