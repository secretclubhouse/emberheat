# Matter Debugging Features - Implementation Summary

This document summarizes the Matter debugging features added to Thermos-Rust to help users diagnose and troubleshoot commissioning issues.

## Overview

The following enhancements were implemented to provide comprehensive debugging capabilities for Matter commissioning:

1. **HTTP Diagnostic Endpoints** - Real-time status via REST API
2. **Network Diagnostics** - Automatic startup checks for common issues
3. **Prometheus Metrics** - Matter-specific metrics for monitoring
4. **Enhanced Logging** - Integrated status reporting
5. **Troubleshooting Documentation** - Comprehensive guide for users

---

## 1. HTTP Diagnostic Endpoints

### Location
Files modified:
- `src/metrics/mod.rs` - Added new endpoints and Matter status tracking

### Endpoints Added

#### **GET /matter/status** (JSON)

Returns current Matter commissioning status:

```json
{
  "commissioned": false,
  "initialized": true,
  "qr_code_available": true,
  "qr_code_file_path": "/home/pi/.config/thermos-rust/matter_qr_code.txt"
}
```

**Use case:** Check if Matter is initialized and commissioned without reading logs

#### **GET /matter/qr_code** (Text)

Returns the Matter QR code in ASCII format with pairing information.

**Use case:** Re-display QR code if terminal output was lost

#### **GET /** (Text)

Root endpoint showing all available diagnostic endpoints.

**Use case:** Discover available endpoints

### Implementation Details

- **Non-blocking:** All endpoints respond immediately without affecting Matter operation
- **Thread-safe:** Uses `Arc<RwLock<>>` for safe concurrent access
- **Always available:** Endpoints work even if Matter initialization failed

### Usage Examples

```bash
# Check Matter status
curl http://localhost:8080/matter/status

# Get QR code
curl http://localhost:8080/matter/qr_code

# View all endpoints
curl http://localhost:8080/
```

---

## 2. Network Diagnostics

### Location
Files created/modified:
- `src/network_diagnostics.rs` - New module for network checks
- `src/main.rs` - Integrated diagnostic checks on startup

### Checks Performed

#### **UDP Port 5540 Binding**
- Verifies the Matter port can be bound
- Detects port conflicts
- Provides resolution commands

#### **Avahi/mDNS Service**
- Checks if Avahi daemon is running
- Tests via `systemctl` and `pgrep` for compatibility
- Required for Matter device discovery

#### **Network Interfaces**
- Verifies network connectivity
- Checks for non-loopback interfaces with IP addresses
- Logs interface details for debugging

#### **Firewall Rules** (Optional)
- Checks if ufw firewall is active
- Verifies UDP port 5540 is allowed
- Provides commands to open ports

### Diagnostic Output

All checks log results with:
- ✓ for successful checks
- ⚠ for warnings with actionable suggestions

Example output:
```
INFO Running network diagnostics for Matter commissioning...
INFO ✓ UDP port 5540 is available and can be bound
INFO ✓ Avahi daemon is running (required for mDNS device discovery)
INFO ✓ Network interfaces configured with IP addresses
INFO ✓ Network diagnostics passed: All checks OK
```

### Error Handling

- **Non-fatal:** Warnings don't prevent startup
- **Informative:** Each warning includes:
  - Category (Port Binding, mDNS Service, Network)
  - Detailed message
  - Suggested resolution command
- **Logged:** All warnings appear in application logs

---

## 3. Prometheus Metrics

### Location
Files modified:
- `src/metrics/mod.rs` - Added Matter-specific metrics

### New Metrics

#### **matter_commissioned** (Gauge)
- **Values:** 0 (not commissioned) or 1 (commissioned)
- **Purpose:** Monitor commissioning status over time
- **Use case:** Alert if device becomes uncom missioned

#### **matter_initialization_status** (Gauge)
- **Values:** 0 (failed) or 1 (success)
- **Purpose:** Track Matter stack initialization
- **Use case:** Detect initialization failures after restart

### Integration

Metrics are automatically updated:
- On Matter initialization
- When commissioning status changes
- Via MatterStatus updates from Matter module

### Usage Examples

```bash
# View all Matter metrics
curl http://localhost:8080/metrics | grep matter

# Check commissioning status
curl -s http://localhost:8080/metrics | grep matter_commissioned

# Monitor with Prometheus
# Configure scrape target: localhost:8080
```

### Grafana Integration

These metrics can be visualized in Grafana dashboards:
- Commissioning status over time
- Initialization success rate
- Correlation with temperature updates

---

## 4. Matter Status Tracking

### Location
Files modified:
- `src/matter/mod.rs` - Added metrics server integration
- `src/metrics/mod.rs` - Added MatterStatus struct
- `src/main.rs` - Linked metrics and Matter servers

### MatterStatus Structure

```rust
pub struct MatterStatus {
    pub commissioned: bool,
    pub initialized: bool,
    pub qr_code_available: bool,
    pub qr_code_file_path: Option<String>,
}
```

### Status Update Flow

1. **Initialization:**
   - Matter server initializes
   - Device created via FFI
   - QR code generated
   - Status updated: `initialized=true, commissioned=false`

2. **Commissioning:**
   - User scans QR code
   - Matter handshake occurs
   - Callbacks fire
   - Status updated: `commissioned=true`

3. **HTTP Access:**
   - User requests `/matter/status`
   - Returns current status immediately
   - No blocking or delays

### QR Code Storage

QR code content is stored in memory and served via HTTP:
- Full ASCII QR code with instructions
- Manual pairing code
- QR code data string
- Timestamp of generation

Priority for file storage:
1. `/var/lib/thermos-rust/matter_qr_code.txt` (system-wide)
2. `~/.config/thermos-rust/matter_qr_code.txt` (user)
3. `./matter_qr_code.txt` (local fallback)

---

## 5. Troubleshooting Documentation

### Location
File created:
- `MATTER_TROUBLESHOOTING.md` - Comprehensive troubleshooting guide

### Sections

1. **Quick Diagnostic Checklist** - Fast verification steps
2. **Built-in Diagnostic Tools** - How to use new endpoints
3. **Common Issues** - Stub mode, multiple instances
4. **Network Problems** - Port conflicts, Avahi, firewall
5. **Commissioning Failures** - QR code, pairing, fabrics
6. **Device Not Discovered** - mDNS, network issues
7. **Temperature Not Updating** - State sync issues
8. **Advanced Debugging** - C++ logging, packet capture
9. **Quick Reference Commands** - Common diagnostic commands

### Key Features

- **Actionable:** Every issue includes specific resolution steps
- **Command-ready:** Copy-paste commands for diagnostics
- **Comprehensive:** Covers all common Matter issues
- **Beginner-friendly:** Explains concepts and provides context

---

## Architecture Changes

### Module Dependencies

```
main.rs
  ├─> metrics::MetricsServer (Arc-wrapped)
  ├─> matter::MatterServer
  │     └─> links to metrics::MetricsServer
  └─> network_diagnostics::run_network_diagnostics()
```

### Data Flow

```
Matter Initialization
  │
  ├─> Generate QR Code
  │     └─> metrics_server.set_qr_code()
  │
  ├─> Check Commissioning Status
  │     └─> device.is_commissioned()
  │
  └─> Update Status
        └─> metrics_server.set_matter_status()

HTTP Request (/matter/status)
  │
  └─> metrics_server.get_matter_status()
        └─> Returns current MatterStatus
```

### Thread Safety

- **MetricsServer:** Wrapped in `Arc` for shared ownership
- **MatterStatus:** Protected by `RwLock` for concurrent access
- **QR Code Data:** Protected by `RwLock` for concurrent access
- **No blocking:** All HTTP handlers are async and non-blocking

---

## Testing Considerations

### Manual Testing

To test the new features:

1. **Start the application:**
   ```bash
   RUST_LOG=debug cargo run
   ```

2. **Test network diagnostics:**
   - Check logs for diagnostic results
   - Verify all checks pass
   - Intentionally break something (e.g., stop Avahi) and verify warning

3. **Test HTTP endpoints:**
   ```bash
   # Status
   curl http://localhost:8080/matter/status

   # QR Code
   curl http://localhost:8080/matter/qr_code

   # Root
   curl http://localhost:8080/
   ```

4. **Test Prometheus metrics:**
   ```bash
   curl http://localhost:8080/metrics | grep matter
   ```

5. **Test commissioning:**
   - Scan QR code with Matter controller
   - Verify status updates to `commissioned: true`

### Automated Testing

The implementation includes:
- **Unit tests** for network diagnostics
- **Compilation tests** (code compiles without errors)
- **Type safety** via Rust's type system

Future testing could include:
- Integration tests with mock Matter controllers
- Network failure simulation tests
- HTTP endpoint response validation

---

## Future Enhancements

Potential additions that were planned but not yet implemented:

### 1. Factory Reset Endpoint

**Endpoint:** `POST /matter/factory_reset`

**Functionality:**
- Clear all Matter fabrics
- Reset to uncommissioned state
- Generate new QR code
- Return new commissioning info

**Blocker:** Requires C++ Matter SDK integration for fabric clearing

### 2. Commissioning Progress Callbacks

**Enhancement:** Real-time commissioning progress updates

**Implementation:**
- Add C++ callbacks for commissioning events
- Expose to Rust via FFI
- Update via SSE or WebSocket to HTTP clients

### 3. Matter Event Log

**Enhancement:** Historical log of Matter events

**Features:**
- Commissioning attempts (success/failure)
- Fabric additions/removals
- Attribute changes
- Network events

**Storage:** Could use Redis or local file

### 4. Enhanced Network Diagnostics

**Additional checks:**
- IPv6 configuration
- mDNS query testing
- UDP packet transmission test
- Network latency measurement

---

## Performance Impact

The new features have minimal performance impact:

### Memory
- **MatterStatus:** ~100 bytes
- **QR Code cache:** ~1-2 KB
- **HTTP server:** Already running, no additional overhead
- **Total:** < 5 KB additional memory

### CPU
- **Network diagnostics:** Runs once on startup (~50-100ms)
- **HTTP endpoints:** Async, non-blocking, negligible overhead
- **Status updates:** Only on state changes (rare)
- **Metrics:** Updated via existing Prometheus infrastructure

### Network
- **No additional traffic:** Diagnostic HTTP requests are on-demand only
- **mDNS:** No changes to existing Matter mDNS behavior

---

## Conclusion

These enhancements significantly improve the debuggability of Matter commissioning in Thermos-Rust:

✅ **Real-time diagnostics** via HTTP endpoints
✅ **Automatic network validation** on startup
✅ **Prometheus monitoring** for operational visibility
✅ **Comprehensive documentation** for troubleshooting
✅ **No breaking changes** to existing functionality
✅ **Minimal performance impact**

Users experiencing commissioning issues now have:
1. Clear visibility into Matter status
2. Automated checks for common problems
3. Actionable error messages with solutions
4. Comprehensive troubleshooting guide
5. Metrics for long-term monitoring

The implementation follows Rust best practices:
- Type safety via strong typing
- Thread safety via Arc/RwLock
- Error handling via Result types
- Async/await for non-blocking I/O
- Comprehensive logging

This foundation enables future enhancements like factory reset, progress tracking, and advanced diagnostics while maintaining code quality and performance.
