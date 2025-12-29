# MQTT Testing Guide for ESP32-C6 Thermostat

This guide explains how to test the MQTT integration for the Week 3 implementation.

## Prerequisites

1. **MQTT Broker**: Mosquitto or any MQTT broker
2. **MQTT Client**: mosquitto_sub for testing
3. **ESP32-C6 Hardware**: Development board with relay and sensor
4. **WiFi Network**: 2.4GHz WiFi (ESP32-C6 doesn't support 5GHz)

## Quick Start

### 1. Install and Start Mosquitto Broker

**On macOS:**
```bash
brew install mosquitto
brew services start mosquitto
```

**On Linux:**
```bash
sudo apt install mosquitto mosquitto-clients
sudo systemctl start mosquitto
sudo systemctl enable mosquitto
```

**On Docker:**
```bash
docker run -d -p 1883:1883 --name mosquitto eclipse-mosquitto
```

### 2. Configure ESP32 WiFi and MQTT

Edit `thermos-esp32/src/main.rs`:

```rust
/// WiFi credentials
const WIFI_SSID: &str = "YourNetworkName";
const WIFI_PASSWORD: &str = "YourPassword";

/// MQTT broker configuration
const MQTT_BROKER: &str = "192.168.1.100"; // Your broker IP
const MQTT_PORT: u16 = 1883;
const DEVICE_ID: &str = "esp32-zone1";

/// Enable WiFi and MQTT
const ENABLE_WIFI: bool = true;
```

### 3. Build and Flash Firmware

```bash
cd thermos-esp32
cargo build --release
espflash flash target/riscv32imac-unknown-none-elf/release/thermos-esp32
```

### 4. Subscribe to MQTT Topics

**Temperature readings (published every 6 seconds):**
```bash
mosquitto_sub -h localhost -t "thermostat/+/temp/#" -v
```

**State change events:**
```bash
mosquitto_sub -h localhost -t "thermostat/+/events/#" -v
```

**Device status:**
```bash
mosquitto_sub -h localhost -t "thermostat/+/status" -v
```

**All topics:**
```bash
mosquitto_sub -h localhost -t "thermostat/#" -v
```

## MQTT Topic Structure

### Temperature Readings

**Topic:** `thermostat/{device_id}/temp/{zone}`

**Example:** `thermostat/esp32-zone1/temp/Zone1`

**Payload (JSON):**
```json
{
  "zone": "Zone1",
  "temp_c": 21.5,
  "temp_f": 70.7,
  "target_c": 21.1,
  "heating": true
}
```

**Published:** Every 6 seconds (control loop interval)

### State Change Events

**Topic:** `thermostat/{device_id}/events/{zone}`

**Example:** `thermostat/esp32-zone1/events/Zone1`

**Payload (JSON):**
```json
{
  "zone": "Zone1",
  "event": "state_change",
  "message": "Heating ON"
}
```

**Published:** When relay state changes or errors occur

**Event types:**
- `state_change` - Heating turned on/off
- `error` - Sensor error or system error

### Device Status

**Topic:** `thermostat/{device_id}/status`

**Example:** `thermostat/esp32-zone1/status`

**Payload (JSON):**
```json
{
  "status": "online"
}
```

**Published:** On startup and periodically

## Expected Output

When you subscribe to all topics and the ESP32 is running, you should see:

```
thermostat/esp32-zone1/status {"status":"online"}
thermostat/esp32-zone1/temp/Zone1 {"zone":"Zone1","temp_c":21.5,"temp_f":70.7,"target_c":21.1,"heating":false}
thermostat/esp32-zone1/temp/Zone1 {"zone":"Zone1","temp_c":21.4,"temp_f":70.5,"target_c":21.1,"heating":false}
thermostat/esp32-zone1/temp/Zone1 {"zone":"Zone1","temp_c":20.8,"temp_f":69.4,"target_c":21.1,"heating":false}
thermostat/esp32-zone1/temp/Zone1 {"zone":"Zone1","temp_c":20.5,"temp_f":68.9,"target_c":21.1,"heating":false}
thermostat/esp32-zone1/events/Zone1 {"zone":"Zone1","event":"state_change","message":"Heating ON"}
thermostat/esp32-zone1/temp/Zone1 {"zone":"Zone1","temp_c":20.4,"temp_f":68.7,"target_c":21.1,"heating":true}
...
```

## Testing Scenarios

### 1. Normal Operation (Mock Sensor)

With `USE_MOCK_SENSOR = true` in main.rs:

```bash
# Monitor temperature readings
mosquitto_sub -h localhost -t "thermostat/+/temp/#" -v

# Expected: Constant temperature (DEFAULT_TEMP_C = 21.1°C)
# Heating state should remain stable
```

### 2. Temperature Change (Real BME280)

With `USE_MOCK_SENSOR = false`:

```bash
# Monitor all topics
mosquitto_sub -h localhost -t "thermostat/#" -v

# Apply heat/cold to sensor
# Expected: Temperature changes, heating cycles on/off
# State change events when heating toggles
```

### 3. Offline Testing

Set `ENABLE_WIFI = false` in main.rs:

```bash
# Build and flash
cargo build --release
espflash flash target/riscv32imac-unknown-none-elf/release/thermos-esp32

# Monitor serial output
espflash monitor

# Expected: Control loop runs without MQTT
# No WiFi connection attempts
# Logging shows "WiFi disabled, running offline"
```

### 4. WiFi Reconnection

```bash
# Start with WiFi enabled and broker running
# Stop the broker
brew services stop mosquitto

# Expected: ESP32 logs connection failures and retries with backoff
# Control loop continues running

# Restart broker
brew services start mosquitto

# Expected: ESP32 reconnects and resumes publishing
```

## Serial Monitor Output

To see detailed logging:

```bash
espflash monitor
```

**Expected logs:**
```
====================================
Thermos ESP32-C6 Thermostat v0.2.0
Week 3: WiFi + MQTT Integration
====================================
WiFi: enabled
MQTT: enabled
Control interval: 6 seconds
Relay initialized to OFF state
NVS storage initialized
Initializing WiFi...
SSID: YourNetwork
WiFi task starting
Starting WiFi controller
WiFi connected successfully
WiFi station connected
IP address: 192.168.1.50
MQTT task starting
Broker: 192.168.1.100:1883
Connecting to MQTT broker...
Resolving DNS for 192.168.1.100...
Connecting to 192.168.1.100:1883...
TCP connected, initializing MQTT client
Sending MQTT CONNECT
MQTT connected successfully
Published online status, entering message loop
Control loop starting for zone: Zone1
Control loop running - target: 21.1°C, hysteresis: ±0.5°C
Temp: 21.1°C, Target: 21.1°C, Heating: OFF
```

## Troubleshooting

### ESP32 Won't Connect to WiFi

1. **Check SSID/Password:**
   - Verify credentials in main.rs
   - Ensure WiFi is 2.4GHz (not 5GHz)
   - Check for special characters in password

2. **Monitor serial output:**
   ```bash
   espflash monitor
   ```
   
3. **Look for errors:**
   - "WiFi connection failed" - wrong credentials
   - "Failed to start WiFi" - hardware issue

### MQTT Connection Fails

1. **Check broker is running:**
   ```bash
   # macOS
   brew services list | grep mosquitto
   
   # Linux
   sudo systemctl status mosquitto
   ```

2. **Test broker connectivity:**
   ```bash
   mosquitto_pub -h 192.168.1.100 -t test -m "hello"
   ```

3. **Check firewall:**
   - Port 1883 must be open
   - Disable firewall temporarily to test

4. **Verify broker IP:**
   - Use `ifconfig` or `ip addr` to find broker machine IP
   - Update `MQTT_BROKER` in main.rs

### No Messages Received

1. **Check subscription topic:**
   ```bash
   # Subscribe to ALL topics
   mosquitto_sub -h localhost -t "#" -v
   ```

2. **Check device ID:**
   - Verify `DEVICE_ID` in main.rs matches expected topics
   - Topics are: `thermostat/{device_id}/...`

3. **Check QoS:**
   - Current implementation uses QoS 0 (fire-and-forget)
   - Messages may be lost if network is unstable

### Heap Memory Issues

If ESP32 crashes with heap errors:

1. **Reduce MQTT_QUEUE_SIZE** in mqtt.rs (currently 16)
2. **Reduce buffer sizes** in mqtt_connect_and_publish()
3. **Monitor heap usage** with esp-println

## Data Retention and Logging

### Option 1: Simple File Logging

```bash
# Log all messages to file
mosquitto_sub -h localhost -t "thermostat/#" -v > thermostat.log
```

### Option 2: InfluxDB + Telegraf

1. **Install InfluxDB and Telegraf**
2. **Configure Telegraf MQTT input:**

```toml
[[inputs.mqtt_consumer]]
  servers = ["tcp://localhost:1883"]
  topics = ["thermostat/+/temp/#"]
  data_format = "json"
  json_string_fields = ["zone"]
```

3. **Query in InfluxDB:**
```sql
SELECT temp_c FROM mqtt_consumer WHERE zone='Zone1'
```

### Option 3: Home Assistant

Add to `configuration.yaml`:

```yaml
mqtt:
  sensor:
    - name: "Zone1 Temperature"
      state_topic: "thermostat/esp32-zone1/temp/Zone1"
      value_template: "{{ value_json.temp_c }}"
      unit_of_measurement: "°C"
      
    - name: "Zone1 Heating"
      state_topic: "thermostat/esp32-zone1/temp/Zone1"
      value_template: "{{ value_json.heating }}"
```

## Performance Metrics

**Expected performance:**

- **Message rate:** ~10 messages/minute (1 per 6 seconds)
- **Message size:** ~100 bytes per temperature reading
- **Bandwidth:** ~1 KB/minute (~45 MB/month)
- **Latency:** <100ms from sensor read to MQTT publish
- **Reliability:** QoS 0 (no retransmission)

**Heap usage:**

- **MQTT buffers:** ~6 KB (rx + tx + write + recv buffers)
- **Channel queue:** ~4 KB (16 messages × 256 bytes)
- **Total MQTT overhead:** ~10 KB

## Next Steps

After verifying MQTT works:

1. **Add QoS 1 for critical events** (state changes, errors)
2. **Implement LWT (Last Will Testament)** for offline detection
3. **Add MQTT configuration UI** (web server or BLE)
4. **Implement MQTT control** (receive target temperature updates)
5. **Add TLS/SSL** for secure connections

## References

- **MQTT Protocol:** https://mqtt.org/
- **Mosquitto Documentation:** https://mosquitto.org/documentation/
- **rust-mqtt Crate:** https://github.com/obabec/rust-mqtt
- **esp-wifi Documentation:** https://docs.espressif.com/projects/rust/esp-wifi/

---

**Week 3 Objectives Completed:**
- ✅ MQTT client implemented with rust-mqtt
- ✅ Temperature readings published every 6 seconds
- ✅ State changes published as events
- ✅ WiFi connectivity with auto-reconnection
- ✅ Can monitor temperature history via MQTT subscriber
