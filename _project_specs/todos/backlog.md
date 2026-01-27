# Backlog

Future work, prioritized. Move to active.md when starting.

---

## High Priority

### HTTP Sensor Integration
**Description**: Add support for HTTP-based temperature sensors (e.g., ESP32 with web server)

**Acceptance Criteria**:
- [ ] Add HTTP sensor configuration in config.json
- [ ] Implement HTTP sensor client with timeout/retry
- [ ] Support multiple HTTP sensors per zone
- [ ] Add metrics for HTTP sensor response time
- [ ] Handle network failures gracefully

**Test Cases**:
- [ ] HTTP sensor responds with valid JSON
- [ ] HTTP sensor timeout (>1s) returns None
- [ ] HTTP sensor returns invalid data
- [ ] Network unreachable handling

---

## Medium Priority

### Web UI
**Description**: Add web interface for monitoring and control

**Acceptance Criteria**:
- [ ] Display current temperature and setpoint per zone
- [ ] Show heating status (on/off) per zone
- [ ] Allow setpoint adjustment via UI
- [ ] Display historical temperature data
- [ ] Show sensor health status

**Test Cases**:
- [ ] UI loads without JavaScript errors
- [ ] Setpoint changes update Redis state
- [ ] Temperature updates refresh in real-time
- [ ] Works on mobile browsers

---

## Low Priority

### Multi-Sensor Averaging
**Description**: Average multiple sensors per zone for better accuracy

**Acceptance Criteria**:
- [ ] Support multiple 1-Wire sensors per zone
- [ ] Implement averaging algorithm
- [ ] Detect and exclude outliers
- [ ] Fall back to single sensor if others fail
- [ ] Add metrics for per-sensor readings

**Test Cases**:
- [ ] Average of 2 sensors is correct
- [ ] Outlier detection removes bad reading
- [ ] Single sensor failure doesn't affect zone
- [ ] All sensors failing triggers safe mode
