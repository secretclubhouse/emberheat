# Completed

Done items for reference. Move here from active.md when complete.

---

## 2025-01-07: DS18B20 1-Wire Support

**Description**: Implemented DS18B20 temperature sensor support via 1-Wire protocol

**Completed**:
- [x] Add esp-hal-rmt-onewire dependency
- [x] Implement DS18B20 sensor reader
- [x] Add configuration for 1-Wire sensors
- [x] Write comprehensive test suite
- [x] Document implementation

**Merged**: PR #37

---

## 2024-12-25: Initial Rust Implementation

**Description**: Rust reimplementation of Python thermostat system

**Completed**:
- [x] Core thermostat logic with hysteresis
- [x] GPIO relay control via rppal
- [x] Redis state persistence
- [x] Prometheus metrics export
- [x] Multi-zone support
- [x] Graceful shutdown handling
- [x] Test infrastructure with mocks
- [x] Documentation (README, ARCHITECTURE, TESTING)
