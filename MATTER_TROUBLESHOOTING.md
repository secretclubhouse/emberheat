# Matter Commissioning Troubleshooting Guide

This guide helps you diagnose and resolve Matter commissioning issues with Thermos-Rust.

## Quick Diagnostic Checklist

Before diving into detailed troubleshooting, run through this quick checklist:

- [ ] Device and controller are on the same network
- [ ] UDP port 5540 is accessible
- [ ] Avahi/mDNS daemon is running
- [ ] QR code or pairing code is available
- [ ] No other Matter device is using the same discriminator

## Table of Contents

1. [Built-in Diagnostic Tools](#built-in-diagnostic-tools)
2. [Common Issues](#common-issues)
3. [Network Problems](#network-problems)
4. [Commissioning Failures](#commissioning-failures)
5. [Device Not Discovered](#device-not-discovered)
6. [Temperature Not Updating](#temperature-not-updating)
7. [Advanced Debugging](#advanced-debugging)

---

## Built-in Diagnostic Tools

Thermos-Rust includes several built-in tools to help diagnose Matter issues:

### 1. HTTP Diagnostic Endpoints

Access these endpoints on port 8080 (or your configured metrics port):

#### **GET /matter/status** - Matter Status (JSON)

Returns current Matter commissioning status:

```bash
curl http://localhost:8080/matter/status
```

Example response:
```json
{
  "commissioned": false,
  "initialized": true,
  "qr_code_available": true,
  "qr_code_file_path": "/home/pi/.config/thermos-rust/matter_qr_code.txt"
}
```

#### **GET /matter/qr_code** - QR Code Display (Text)

Displays the Matter commissioning QR code in ASCII format:

```bash
curl http://localhost:8080/matter/qr_code
```

This is useful if you lost the QR code from the terminal output.

#### **GET /metrics** - Prometheus Metrics

Includes Matter-specific metrics:

```bash
curl http://localhost:8080/metrics | grep matter
```

Key metrics:
- `matter_commissioned` - 1 if commissioned, 0 if not
- `matter_initialization_status` - 1 if initialized successfully, 0 if failed

### 2. Network Diagnostics

Thermos-Rust automatically runs network diagnostics on startup. Check the logs:

```bash
journalctl -u thermos-rust | grep "network diagnostics"
```

Or if running directly:
```bash
RUST_LOG=info ./thermos-rust
```

Look for:
- ✓ UDP port 5540 is available and can be bound
- ✓ Avahi daemon is running
- ✓ Network interfaces configured with IP addresses

### 3. Debug Logging

Enable detailed Matter logging:

```bash
RUST_LOG=debug thermos-rust
```

For even more verbose output:

```bash
RUST_LOG=thermos_rust::matter=trace thermos-rust
```

---

## Common Issues

### Issue: "Matter device initialized (STUB)"

**Symptoms:** Logs show "Matter device initialized (STUB)" instead of full initialization.

**Cause:** The application was built without the Matter SDK libraries.

**Solution:**

1. Check if you have the Matter SDK libraries:
   ```bash
   ls lib/connectedhomeip/*/libCHIP*
   # OR
   ls third_party/connectedhomeip/out/host/lib/
   ```

2. If libraries are missing, build them:
   ```bash
   ./scripts/download-matter.sh
   # OR follow MATTER_BUILD_GUIDE.md
   ```

3. Rebuild Thermos-Rust:
   ```bash
   cargo build --release
   ```

**Note:** The stub implementation won't actually commission - it's only for development/testing.

---

### Issue: "Only one Matter device can be created per process"

**Symptoms:** Error message on startup about only one Matter device being allowed.

**Cause:** Either:
1. Another instance of Thermos-Rust is already running
2. The application crashed and didn't clean up properly

**Solution:**

1. Check for running instances:
   ```bash
   ps aux | grep thermos-rust
   ```

2. Kill any stale processes:
   ```bash
   sudo killall thermos-rust
   ```

3. If using systemd, restart the service:
   ```bash
   sudo systemctl restart thermos-rust
   ```

---

## Network Problems

### Issue: UDP Port 5540 Cannot Be Bound

**Symptoms:**
- Network diagnostics show port binding failure
- Error: "Failed to bind UDP port 5540"

**Diagnosis:**

Check what's using the port:
```bash
sudo netstat -ulnp | grep 5540
# OR
sudo lsof -i UDP:5540
```

**Solution:**

1. If another process is using it, stop that process
2. If firewall is blocking:
   ```bash
   sudo ufw allow 5540/udp
   ```
3. Verify port is free and try again

---

### Issue: Avahi/mDNS Not Running

**Symptoms:**
- Network diagnostics show "Avahi daemon is not active"
- Device not discovered by Matter controllers

**Diagnosis:**

```bash
systemctl status avahi-daemon
```

**Solution:**

1. Install Avahi (if not installed):
   ```bash
   sudo apt update
   sudo apt install avahi-daemon avahi-utils
   ```

2. Start and enable Avahi:
   ```bash
   sudo systemctl start avahi-daemon
   sudo systemctl enable avahi-daemon
   ```

3. Verify it's running:
   ```bash
   systemctl is-active avahi-daemon
   ```

4. Test mDNS functionality:
   ```bash
   avahi-browse -a
   ```

---

### Issue: Firewall Blocking Matter Traffic

**Symptoms:**
- Port 5540 bound successfully but device not discovered
- Network diagnostics show firewall is active

**Diagnosis:**

```bash
sudo ufw status
```

**Solution:**

```bash
# Allow Matter port
sudo ufw allow 5540/udp

# Optionally allow mDNS (port 5353)
sudo ufw allow 5353/udp

# Reload firewall
sudo ufw reload
```

For iptables:
```bash
sudo iptables -A INPUT -p udp --dport 5540 -j ACCEPT
sudo iptables -A INPUT -p udp --dport 5353 -j ACCEPT
sudo iptables-save
```

---

## Commissioning Failures

### Issue: QR Code Won't Scan

**Symptoms:**
- QR code displayed in terminal but won't scan
- "Invalid QR code" error in Matter controller app

**Solutions:**

1. **Try the manual pairing code instead:**
   - Look for "Manual Pairing Code" in the logs (11-digit number)
   - Enter this in your Matter controller app

2. **Regenerate QR code in a better format:**
   - Access via HTTP: `curl http://localhost:8080/matter/qr_code`
   - Or check the file: `cat ~/.config/thermos-rust/matter_qr_code.txt`

3. **Display QR code larger:**
   - Screenshot the QR code from HTTP endpoint
   - Display on a larger screen
   - Ensure good lighting when scanning

4. **Check QR code data:**
   ```bash
   grep "QR Code Data" ~/.config/thermos-rust/matter_qr_code.txt
   ```
   Should start with `MT:` and contain alphanumeric characters

---

### Issue: Pairing Fails After QR Scan

**Symptoms:**
- QR code scans successfully
- Pairing process starts but fails mid-way
- "Commissioning failed" error

**Diagnosis Steps:**

1. **Check logs for specific errors:**
   ```bash
   journalctl -u thermos-rust -f
   ```

2. **Verify device is initialized:**
   ```bash
   curl http://localhost:8080/matter/status
   ```
   Should show `"initialized": true`

3. **Check network connectivity:**
   - Device and controller must be on same network
   - No VLANs or network segmentation between them

**Solutions:**

1. **Ensure both devices on same network:**
   ```bash
   # Check device's IP
   ip addr show

   # Verify controller can reach device
   ping <device_ip>
   ```

2. **Disable AP isolation** (if using WiFi):
   - Check router settings
   - Ensure "AP Isolation" or "Client Isolation" is disabled

3. **Restart both device and controller:**
   ```bash
   sudo systemctl restart thermos-rust
   ```
   Then retry pairing from the Matter controller

4. **Check for conflicting discriminators:**
   - If you have multiple Matter devices, ensure they use different discriminators
   - Current discriminator is in logs or check `src/matter/mod.rs` (default: 3840)

---

### Issue: Device Already Paired to Another Fabric

**Symptoms:**
- "Device already commissioned" in logs
- Cannot pair to new controller

**Solution:**

**Option 1: Commission to same Matter fabric**
- Use the same Matter controller/home that originally paired the device
- Device can be in multiple homes via Matter's multi-admin feature

**Option 2: Factory reset** (NOT YET IMPLEMENTED)
- Currently, you need to manually reset:
  1. Stop the service: `sudo systemctl stop thermos-rust`
  2. Clear Matter credentials (implementation-specific)
  3. Restart: `sudo systemctl start thermos-rust`

**Note:** Factory reset functionality is planned for future release via `/matter/factory_reset` endpoint.

---

## Device Not Discovered

### Issue: Matter Controller Can't Find Device

**Symptoms:**
- QR code scans but no device found
- Controller shows "Searching..." indefinitely
- No pairing flow starts

**Diagnosis:**

1. **Verify Matter server is running:**
   ```bash
   curl http://localhost:8080/matter/status
   ```

2. **Check mDNS advertisement:**
   ```bash
   avahi-browse -r _matterc._udp
   ```
   Should show Thermos-Rust device

3. **Check UDP port 5540:**
   ```bash
   sudo netstat -ulnp | grep 5540
   ```

**Solutions:**

1. **Restart Avahi daemon:**
   ```bash
   sudo systemctl restart avahi-daemon
   sudo systemctl restart thermos-rust
   ```

2. **Verify network interfaces:**
   ```bash
   ip addr show
   ```
   Ensure you have an active non-loopback interface with an IP

3. **Check for network issues:**
   - Try from a different controller device
   - Ensure no firewall/router blocking mDNS (port 5353 UDP)
   - Move controller closer to device (WiFi signal strength)

4. **Enable debug logging to see mDNS details:**
   ```bash
   RUST_LOG=debug thermos-rust 2>&1 | tee matter-debug.log
   ```

---

## Temperature Not Updating

### Issue: Matter Controller Shows Stale Temperature

**Symptoms:**
- Device commissioned successfully
- Temperature in HomeKit/Google Home doesn't update
- Local logs show correct temperature

**Diagnosis:**

1. **Check state sync:**
   ```bash
   journalctl -u thermos-rust | grep "state sync"
   ```

2. **Verify thermostat state:**
   ```bash
   curl http://localhost:8080/metrics | grep current_temperature
   ```

**Solutions:**

1. **Restart Matter server:**
   ```bash
   sudo systemctl restart thermos-rust
   ```

2. **Check sensor connectivity:**
   - Verify 1-Wire sensors are detected: `ls /sys/bus/w1/devices/`
   - Check sensor read logs: `journalctl -u thermos-rust | grep sensor`

3. **Re-pair device** (if temperature never updated):
   - Remove device from Matter controller
   - Re-scan QR code and pair again

---

## Advanced Debugging

### Enable C++ Matter SDK Logging

The C++ Matter SDK has its own logging that goes to stderr. Capture it:

```bash
thermos-rust 2>&1 | tee full-debug.log
```

### Check Matter Cluster Attributes

If you have Matter development tools installed:

```bash
# Check thermostat cluster
matter-cli im read <node-id> <endpoint-id> 0x0201 0x0000  # LocalTemperature
matter-cli im read <node-id> <endpoint-id> 0x0201 0x0012  # HeatingSetpoint
matter-cli im read <node-id> <endpoint-id> 0x0201 0x001C  # SystemMode
```

### Inspect QR Code Payload

Decode the QR code to verify setup information:

```bash
# Extract QR code data
QR_DATA=$(grep "QR Code Data:" ~/.config/thermos-rust/matter_qr_code.txt | cut -d' ' -f4)

# The format is: MT:discriminator.passcode.vendorID.productID
echo "QR Code: $QR_DATA"
```

### Network Packet Capture

Capture Matter traffic for analysis:

```bash
sudo tcpdump -i any -w matter-capture.pcap udp port 5540 or udp port 5353
```

Analyze with Wireshark to see actual Matter protocol messages.

### Check System Resources

Matter stack can be resource-intensive:

```bash
# Check memory
free -h

# Check CPU
top -p $(pgrep thermos-rust)

# Check disk space
df -h
```

Ensure:
- At least 50MB free RAM
- CPU usage < 50%
- Sufficient disk space for logs

---

## Getting Help

If you're still stuck after trying these steps:

1. **Gather diagnostic information:**
   ```bash
   # System info
   uname -a

   # Thermos-Rust version
   thermos-rust --version

   # Matter status
   curl http://localhost:8080/matter/status

   # Recent logs
   journalctl -u thermos-rust --since "10 minutes ago"

   # Network diagnostics
   ip addr show
   systemctl status avahi-daemon
   sudo netstat -ulnp | grep 5540
   ```

2. **Enable debug logging and capture full output:**
   ```bash
   RUST_LOG=debug thermos-rust 2>&1 | tee debug-output.log
   ```

3. **Create a GitHub issue** with:
   - Description of the problem
   - Steps to reproduce
   - Diagnostic information from above
   - Debug logs (sanitize any sensitive info)

---

## Additional Resources

- **Matter Specification**: https://csa-iot.org/developer-resource/specifications-download-request/
- **Thermos-Rust Documentation**: See README.md, ARCHITECTURE.md, MATTER_SETUP.md
- **Matter Development**: https://github.com/project-chip/connectedhomeip
- **Avahi Documentation**: https://www.avahi.org/

---

## Quick Reference Commands

```bash
# View Matter status
curl http://localhost:8080/matter/status

# View QR code
curl http://localhost:8080/matter/qr_code

# Check logs
journalctl -u thermos-rust -f

# Enable debug logging
RUST_LOG=debug thermos-rust

# Check network
sudo netstat -ulnp | grep 5540
systemctl status avahi-daemon

# Restart service
sudo systemctl restart thermos-rust

# View metrics
curl http://localhost:8080/metrics
```
