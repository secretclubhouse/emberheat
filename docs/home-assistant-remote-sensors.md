# Remote room sensors via Home Assistant

EmberHeat can take each zone's temperature from sensors that sit in the room
instead of at the controller. Home Assistant reads those sensors (Matter,
Thread, Zigbee, anything it can see) and publishes the values to your MQTT
broker. The firmware subscribes and averages the readings for each zone.

Adding, removing or swapping a sensor is a Home Assistant change only. The
firmware does not need rebuilding.

This works in both firmware modes (Matter plus MQTT, and MQTT only).

## Turning it on

The firmware ships with `SENSOR_SOURCE = SensorSource::Mock` in
`emberheat-esp32/src/config.rs`, which uses a fixed fake temperature. It stays
that way until the bench checklist below has been run on real hardware. To use
remote sensors, set it to `SensorSource::Mqtt` and reflash.

## MQTT contract

| Item | Value |
|---|---|
| Topic | `thermostat/<device_id>/zone/<zone_name>/sensor/<sensor_id>` |
| `<device_id>` | `MQTT_DEVICE_ID` (default `emberheat-esp32`) |
| `<zone_name>` | the zone's `name` in `ZONE_CONFIGS`, e.g. `Zone1`, matched exactly |
| `<sensor_id>` | any label you choose, up to 24 bytes, unique within the zone, e.g. `living_room_eve` |
| Payload | temperature in degrees Celsius as plain text, e.g. `21.4` |
| QoS | 0 |
| Retained | never |

The firmware subscribes once to `thermostat/<device_id>/zone/+/sensor/+`.

Rules the firmware applies to what it receives:

- A zone averages up to 4 sensors. A fifth sensor is ignored unless one of the
  existing four has gone stale.
- A reading counts for 10 minutes after it arrives. If a sensor goes quiet, it
  drops out of the average after that and the rest carry on.
- Readings must be numbers from 0 to 37 degrees C. Anything else (text, `70.5`,
  below freezing) is logged and ignored, and the sensor's previous reading is
  kept and ages out normally.
- Messages flagged as retained are ignored. The firmware measures when a
  message arrived, not when the sensor measured, so a retained value replayed
  after a reconnect would look fresh when it is not.
- An unknown zone name is logged as a warning and ignored.

Home Assistant should republish every reading every 2 minutes, and again as
soon as the controller announces itself on `thermostat/<device_id>/status`
(the firmware publishes `{"status":"online"}` there after it has subscribed).
The 2-minute period against the 10-minute cutoff tolerates four lost QoS 0
messages in a row.

## Home Assistant automation

Create one automation per zone. This example feeds `Zone1` from two sensors:

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

If your `MQTT_DEVICE_ID` is not the default, change `emberheat-esp32` in both
topics.

If Home Assistant runs in degrees F, the payload template converts to Celsius:

```
{{ ((states(repeat.item) | float - 32) * 5 / 9) | round(2) }}
```

### Why the `last_reported` guard matters

The automation republishes on a timer. Without the guard, a sensor whose
battery has died would keep having its last value republished every 2 minutes,
and the firmware would treat each republish as a fresh reading. The room could
be cold while the thermostat regulates against a number that is hours old. The
guard stops publishing for a sensor that has not reported for 15 minutes, so the
firmware sees it go quiet and drops it from the average after its own 10-minute
cutoff.

## What happens when readings stop

If all of a zone's readings stop (Home Assistant down, broker down, WiFi down,
all sensor batteries dead), the zone does not simply switch off, and it does not
run unbounded either.

| Situation | Zone behaviour |
|---|---|
| Boot, no reading yet | Heating OFF until the first valid reading |
| One of three sensors goes quiet | Average of the remaining two after 10 minutes |
| All sensors quiet for 10 minutes | Fallback: the relay cycles on a 30-minute period at the zone's recent duty cycle, capped at 75%. Matter shows no temperature. Status `fallback` is published. |
| A reading returns during fallback | Normal control resumes immediately |
| Thermostat set to Off during fallback | Heating OFF within a few seconds, and it stays OFF |
| No readings for 6 hours after fallback began | Heating OFF, status `lost` |
| Reboot with nothing publishing | Heating OFF (the duty history lives in RAM only) |
| Broker or WiFi drops | MQTT reconnects with backoff; zone control keeps running; fallback begins after 10 minutes without readings |

"Recent duty cycle" means the fraction of roughly the last two hours the relay
was ON while the zone had fresh readings. A zone that was heating about half the
time keeps heating about half the time. An ON time under 3 minutes is rounded
to zero, because it is not worth a relay cycle. Setpoint changes have no effect
during fallback.

The 6-hour limit is deliberate: it is long enough that an overnight outage
leaves a slightly warm room rather than a cold one, and short enough that
heating never runs unattended indefinitely.

## Sensor status topic

Each zone publishes its input health whenever it changes:

- Topic: `thermostat/<device_id>/zone/<zone_name>/sensor_status`
- Payload: plain text, one of
  - `ok`: fresh readings, normal control
  - `fallback`: readings lost, relay is cycling on recent duty
  - `lost`: fallback limit reached, heating OFF

The temperature message the firmware already publishes for each zone also
carries `"sensors":N`, the number of sensors in the current average.

This lets Home Assistant tell you when fallback starts instead of 6 hours later.
Example alert:

```yaml
alias: EmberHeat Zone1 sensor alert
triggers:
  - trigger: mqtt
    topic: thermostat/emberheat-esp32/zone/Zone1/sensor_status
    payload: fallback
    id: fallback
  - trigger: mqtt
    topic: thermostat/emberheat-esp32/zone/Zone1/sensor_status
    payload: lost
    id: lost
actions:
  - action: notify.notify
    data:
      title: "EmberHeat Zone1"
      message: >-
        {% if trigger.id == 'lost' %}
          Zone1 has had no sensor readings for 6 hours. Heating is OFF.
        {% else %}
          Zone1 lost its sensor readings and is running on its recent duty cycle.
        {% endif %}
```

Replace `notify.notify` with your own notify service.

## Bench checklist

Run this once with real hardware before relying on it. The test rig is an
ESP32-C6 plus ESP32-H2, so it flashes with the dual-radio target. Replace `<id>`
with your `MQTT_DEVICE_ID`.

1. Set `SENSOR_SOURCE = Mqtt` in `emberheat-esp32/src/config.rs` and flash with
   `make flash-monitor-matter-dual-radio`.
2. Run `mosquitto_pub -t thermostat/<id>/zone/Zone1/sensor/test -m 19.0` every
   2 minutes. The relay follows the setpoint, and Apple Home shows 19.0.
3. Publish the same with `-r` (retained). The log shows it ignored. (This is a
   debug-level line.)
4. Publish `70.5` and `abc`. Both are logged and ignored.
5. Shorten the fallback so a full cycle fits on the bench. Temporarily set
   `REMOTE_FALLBACK` to
   `{ max_secs: 600, period_secs: 120, max_duty: 0.75, min_on_secs: 10 }` and
   `REMOTE_SENSOR_MAX_AGE_SECS = 60`, reflash, and run with heating cycling for
   10 minutes so the duty tracker has history. Then stop publishing. Expect:
   `fallback` is published, Apple Home shows no temperature, the relay cycles,
   and after 10 minutes status `lost` is published and the relay is OFF.
6. During fallback, set the thermostat to Off in Apple Home. The relay goes OFF
   within 6 seconds and stays OFF.
7. Resume publishing during fallback. `ok` is published and normal control
   resumes.
8. Stop the broker with heating ON. Expect the same fallback behaviour, and zone
   control keeps running while MQTT is down.
9. Reboot with nothing publishing. The relay stays OFF.
10. Restore the constants you changed in step 5 (and leave `SENSOR_SOURCE` as
    you intend to run it).
