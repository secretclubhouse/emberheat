# Thermos-Rust Quick Start Guide

Get your multi-zone thermostat running in 15 minutes!

## Prerequisites Checklist

- [ ] Raspberry Pi (any model with GPIO)
- [ ] DS18B20 temperature sensor(s)
- [ ] Solid-state relay(s)
- [ ] 4.7kΩ resistor
- [ ] Raspberry Pi OS installed
- [ ] Internet connection

## Step 1: Hardware Setup (5 minutes)

### Wiring Diagram

```
Raspberry Pi
    GPIO 4 ──┬── [4.7kΩ] ── 3.3V
             │
             └── DS18B20 Data

    3.3V ─────── DS18B20 VCC
    GND ──────── DS18B20 GND

    GPIO 23 ──── Relay IN (Zone 1)
    5V ──────── Relay VCC
    GND ──────── Relay GND
```

### Enable 1-Wire Interface

```bash
sudo raspi-config
# Navigate: Interface Options > 1-Wire > Enable
sudo reboot
```

## Step 2: Install Dependencies (3 minutes)

```bash
# Update system
sudo apt-get update
sudo apt-get upgrade -y

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# Install system packages
sudo apt-get install -y build-essential pkg-config libssl-dev redis-server

# Verify Redis is running
sudo systemctl status redis
```

## Step 3: Find Your Sensor IDs (1 minute)

```bash
# List 1-Wire devices
ls /sys/bus/w1/devices/

# Example output:
# 28-00000ab12345  28-00000ab12346  w1_bus_master1

# Test reading a sensor
cat /sys/bus/w1/devices/28-00000ab12345/w1_slave
```

Copy the sensor IDs (starting with `28-`) - you'll need these for configuration.

## Step 4: Build the Application (3 minutes)

```bash
# Clone or navigate to project
cd thermos-rust

# Copy example config
cp config/config.example.json config/config.json

# Edit with your sensor IDs and GPIO pins
nano config/config.json
```

**Example configuration:**

```json
{
    "Zone1": {
        "relay_pin": 23,
        "temp_pin": 4,
        "temp_id": "28-00000ab12345"
    }
}
```

```bash
# Build release version
cargo build --release
```

## Step 5: Run the Application (1 minute)

```bash
# Test run (with debug output)
sudo RUST_LOG=debug ./target/release/thermos-rust

# You should see:
# [INFO] Starting Thermos-Rust multi-zone thermostat system
# [INFO] Loading configuration from: "./config/config.json"
# [INFO] Loaded configuration for 1 zones
# [INFO] Starting Prometheus metrics server on port 8080
# [INFO] Initializing Matter server on port 5540
# ...
```

**Press Ctrl+C to stop**

## Step 6: Install as Service (2 minutes)

```bash
# Create installation directory
sudo mkdir -p /opt/thermos-rust
sudo cp target/release/thermos-rust /opt/thermos-rust/
sudo cp -r config /opt/thermos-rust/

# Install systemd service
sudo cp thermos-rust.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable thermos-rust
sudo systemctl start thermos-rust

# Check status
sudo systemctl status thermos-rust

# View logs
sudo journalctl -u thermos-rust -f
```

## Step 7: Commission with HomeKit

1. **Find the pairing code** in the logs:
   ```bash
   sudo journalctl -u thermos-rust | grep "QR code"
   ```

2. **Open Apple Home app** on your iPhone/iPad

3. **Tap "+" → Add Accessory**

4. **Scan the QR code** or enter manual pairing code

5. **Follow on-screen instructions** to complete setup

6. **Control your zones!** They'll appear as separate thermostats

## Verification Steps

### 1. Check Temperature Reading

```bash
# View current metrics
curl http://localhost:8080/metrics | grep current_temperature

# Should show something like:
# current_temperature{room="Zone1",heat_status="0"} 68.5
```

### 2. Check Redis State

```bash
redis-cli GET thermostat:Zone1

# Should show JSON state:
# {"relay_pin":23,"temp_pin":4,"temp_id":"28-00000ab12345",...}
```

### 3. Test Relay Control

In the Home app:
1. Set target temperature above current → relay should turn ON
2. Set target temperature below current → relay should turn OFF
3. Watch the logs for state changes

## Common Issues and Fixes

### Issue: "Permission denied" accessing GPIO

**Fix:**
```bash
# Add user to gpio group
sudo usermod -a -G gpio $USER
# OR run as root
sudo ./target/release/thermos-rust
```

### Issue: Sensor not found

**Fix:**
```bash
# Verify 1-Wire is loaded
lsmod | grep w1
# Should show: w1_gpio, w1_therm

# If not:
echo "dtoverlay=w1-gpio" | sudo tee -a /boot/config.txt
sudo reboot
```

### Issue: Redis connection failed

**Fix:**
```bash
sudo systemctl start redis
sudo systemctl enable redis
```

### Issue: Can't build on Raspberry Pi (low memory)

**Fix:**
```bash
# Build on another machine and copy binary
# Or use Docker

# Build for Raspberry Pi on x86_64:
rustup target add armv7-unknown-linux-gnueabihf
cargo build --release --target armv7-unknown-linux-gnueabihf

# Copy to Pi:
scp target/armv7-unknown-linux-gnueabihf/release/thermos-rust pi@raspberrypi:/home/pi/
```

### Issue: Matter pairing fails

**Fix:**
```bash
# Ensure UDP port 5540 is not blocked
sudo ufw allow 5540/udp

# Restart the service
sudo systemctl restart thermos-rust

# Check Matter server is running
sudo journalctl -u thermos-rust | grep Matter
```

## Next Steps

### Monitor Your System

```bash
# Real-time logs
sudo journalctl -u thermos-rust -f

# Metrics dashboard
# Open: http://raspberrypi:8080/metrics

# Install Grafana for visualization
docker-compose up -d grafana
# Open: http://raspberrypi:3000
```

### Add More Zones

1. Edit `config/config.json`
2. Add new zone with unique pins and sensor ID
3. Restart: `sudo systemctl restart thermos-rust`
4. Re-commission in Home app to see new zones

### Fine-Tune Control

Edit `/opt/thermos-rust/thermos-rust` (rebuild from source):

```rust
// Adjust hysteresis in src/devices/mod.rs
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.5;  // Wider = less frequent cycles
const HYSTERESIS_LOWER_THRESHOLD_C: f32 = -0.5;

// Adjust control loop interval
const CONTROL_LOOP_INTERVAL_SECS: u64 = 6;  // Longer = lower CPU usage
```

Then rebuild and reinstall.

## Getting Help

- **Logs**: `sudo journalctl -u thermos-rust -f`
- **Metrics**: `curl http://localhost:8080/metrics`
- **Redis**: `redis-cli` then `KEYS thermostat:*`
- **Issues**: https://github.com/yourusername/thermos-rust/issues

## Success!

You now have a working multi-zone thermostat with:
- ✅ HomeKit integration via Matter
- ✅ Automatic temperature control
- ✅ State persistence
- ✅ Prometheus metrics
- ✅ Automatic service restart

Enjoy your automated heating system! 🎉
