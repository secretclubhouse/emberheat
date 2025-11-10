# AI Agent Guidelines for Thermos-Rust

This document provides comprehensive guidance for AI agents (including Claude, GitHub Copilot, and other AI assistants) working with the Thermos-Rust codebase.

## Quick Start for AI Agents

### Project Identity
- **Name**: Thermos-Rust
- **Type**: Embedded system / IoT thermostat controller
- **Language**: Rust 2021 edition
- **Platform**: Linux (Raspberry Pi)
- **Domain**: HVAC control, smart home automation
- **Safety-critical**: Yes (controls physical heating systems)

### Key Facts
- **Lines of Code**: ~5,735 (src) + 24,000 (tests) + 70,000 (docs)
- **Architecture**: Async (Tokio), multi-zone, event-driven
- **Hardware**: GPIO relays, 1-Wire temperature sensors
- **Protocols**: Matter/Thread for smart home integration
- **State**: Redis-based persistence
- **Metrics**: Prometheus export on port 8080

## Understanding the Codebase

### High-Level Architecture

```
┌──────────────────────────────────────┐
│  Smart Home Controllers              │
│  (HomeKit, Google Home, Alexa)       │
└────────────┬─────────────────────────┘
             │ Matter Protocol (UDP 5540)
      ┌──────▼────────┐
      │ Matter Server │ ─────► src/matter/
      └──────┬────────┘
             │
      ┌──────▼───────────┐
      │ Thermostat Bridge │ ─► src/devices/
      └──────┬───────────┘
             │
   ┌─────────┼─────────┬─────────┐
   │         │         │         │
┌──▼───┐ ┌──▼───┐ ┌──▼───┐ ┌──▼───┐
│Zone1 │ │Zone2 │ │Zone3 │ │ZoneN │
└──┬───┘ └──┬───┘ └──┬───┘ └──┬───┘
   │         │         │         │
   └─────────┴─────────┴─────────┘
             │
      ┌──────▼────────┐
      │ GPIO Controller│ ─────► src/gpio/
      └──────┬────────┘
             │
   ┌─────────┼──────────┬──────────┐
   │         │          │          │
┌──▼───┐ ┌──▼────┐ ┌──▼──────┐ ┌─▼─────┐
│Relays│ │1-Wire │ │HTTP     │ │Metrics│
│      │ │Sensors│ │Sensors  │ │Redis  │
└──────┘ └───────┘ └─────────┘ └───────┘
         src/sensors/  src/sensors/  src/persistence/
                                     src/metrics/
```

### Module Dependency Graph

```
main.rs
  ├─> config::load() ────────────────► config/mod.rs
  ├─> gpio::GpioController::new() ───► gpio/mod.rs
  ├─> persistence::StateManager ─────► persistence/mod.rs
  ├─> devices::ThermostatBridge ─────► devices/mod.rs
  │     └─> sensors::CompositeSensor ► sensors/mod.rs
  │           ├─> sensors::OneWire ──► sensors/onewire.rs
  │           └─> sensors::Http
  ├─> matter::MatterServer ──────────► matter/mod.rs
  │     └─> matter::ffi ─────────────► matter/ffi/mod.rs
  └─> metrics::start_server() ───────► metrics/mod.rs
```

### Data Flow Patterns

**1. Control Loop (Primary Flow)**
```
Timer (6s) → Read Sensor → Validate → Hysteresis Logic → Update GPIO → Persist → Update Metrics
```

**2. Matter Integration (Bidirectional)**
```
HomeKit ↔ Matter Server ↔ Thermostat Bridge ↔ Zone Thermostat ↔ GPIO/State
```

**3. State Persistence**
```
Thermostat State Change → Serialize → Redis → Recovery on Restart
```

## Code Navigation Guide

### Finding Functionality

| I need to... | Look in... | Key functions/types |
|--------------|-----------|---------------------|
| Understand configuration | `src/config/mod.rs` | `Config`, `ZoneConfig`, `load_config()` |
| Modify control logic | `src/devices/mod.rs` | `Thermostat::run()`, hysteresis constants |
| Add sensor type | `src/sensors/mod.rs` | `TemperatureSensor` trait, `CompositeSensor` |
| Change GPIO behavior | `src/gpio/mod.rs` | `GpioController`, `RelayController` |
| Fix Redis issues | `src/persistence/mod.rs` | `StateManager`, `save_state()`, `load_state()` |
| Update metrics | `src/metrics/mod.rs` | Prometheus gauge definitions |
| Work on Matter | `src/matter/mod.rs` | `MatterServer`, `ThermostatAccessory` |
| Write cross-platform tests | `src/mocks/` | Mock implementations |

### Important Files by Purpose

**Configuration & Setup:**
- `config/config.example.json` - Configuration template
- `Cargo.toml` - Dependencies and build configuration
- `build.rs` - Build script for Matter C++ FFI

**Core Logic:**
- `src/devices/mod.rs` - **MOST IMPORTANT** - Thermostat control logic
- `src/sensors/mod.rs` - Temperature sensor abstraction
- `src/gpio/mod.rs` - Hardware control layer

**Integration:**
- `src/matter/mod.rs` - Smart home integration (1,174 lines)
- `src/persistence/mod.rs` - State management
- `src/metrics/mod.rs` - Observability

**Testing:**
- `tests/mock_integration_tests.rs` - Cross-platform tests (15 KB)
- `tests/integration_test.rs` - Hardware integration tests
- `src/mocks/` - Mock implementations for testing

**Documentation:**
- `README.md` - User guide (710 lines)
- `ARCHITECTURE.md` - Technical design (386 lines)
- `TESTING.md` - Test infrastructure
- `.claude/CLAUDE.md` - AI agent instructions

## Critical Concepts

### 1. Hysteresis Control

The core temperature control algorithm prevents rapid relay cycling:

```rust
// Constants (src/devices/mod.rs)
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.5;   // Turn OFF at target+0.5°C
const HYSTERESIS_LOWER_THRESHOLD_C: f32 = -0.5;  // Turn ON at target-0.5°C
const DEFAULT_TARGET_TEMP_C: f32 = 21.1;         // 70°F

// Logic
if current_temp <= target_temp + HYSTERESIS_LOWER_THRESHOLD_C {
    // Too cold → Turn ON
} else if current_temp >= target_temp + HYSTERESIS_UPPER_THRESHOLD_C {
    // Too hot → Turn OFF
} else {
    // Dead band → Maintain current state
}
```

**Why this matters:**
- Relays have limited lifetime (prevent chatter)
- Energy efficiency vs comfort balance
- **Don't change without hardware testing**

### 2. Sensor Error Recovery

When sensors fail, the system performs automatic recovery:

```rust
// Power cycling sequence (src/devices/mod.rs)
1. Detect invalid reading (< 3°C or > 37°C)
2. Log error + increment Prometheus counter
3. Turn OFF VCC pin for 3 seconds
4. Turn ON VCC pin
5. Wait 5 seconds for sensor recovery
6. Use default temperature (21.1°C) until next successful read
```

**Why this matters:**
- DS18B20 sensors can hang/freeze
- Power cycling usually fixes issues
- System remains operational during sensor failures

### 3. Async Concurrency Model

The system uses Tokio for async operations:

```rust
// Each zone runs independently
tokio::spawn(async move {
    thermostat.run().await
});

// Shared state protected by locks
Arc<RwLock<f32>>  // Read-heavy (temperature, state)
Arc<Mutex<HashMap>>  // Write-heavy or fast ops (GPIO)
```

**Why this matters:**
- Multiple zones operate concurrently
- GPIO must be thread-safe (no conflicts)
- State updates must be atomic

### 4. Matter Protocol Integration

Matter uses centi-degrees Celsius for temperature:

```rust
// Conversion (src/matter/mod.rs)
fn celsius_to_matter(temp_c: f32) -> i16 {
    (temp_c * 100.0) as i16
}

fn matter_to_celsius(centi_c: i16) -> f32 {
    (centi_c as f32) / 100.0
}

// Example: 21.1°C → 2110 centi-degrees
// Example: 2050 centi-degrees → 20.5°C
```

**Why this matters:**
- Matter spec requires centi-degrees
- Precision loss if conversion is wrong
- HomeKit displays incorrect values if broken

### 5. State Persistence

State is saved to Redis as JSON:

```rust
// State structure (src/persistence/mod.rs)
{
    "relay_pin": 23,
    "temp_pin": 4,
    "temp_id": "28-00000ab12345",
    "target_temp": 21.1,
    "target_state": 1,       // 0=OFF, 1=ON
    "current_state": 1,
    "last_updated": "2024-01-15T10:30:00Z"
}
```

**Why this matters:**
- System recovers state after restart
- Breaking state format requires migration
- Invalid state can cause startup failure

## Common Development Patterns

### Adding a New Feature

**Example: Add humidity sensor support**

1. **Design phase:**
   ```bash
   # Read related code
   cat src/sensors/mod.rs
   cat src/sensors/onewire.rs

   # Check tests
   grep -r "sensor" tests/
   ```

2. **Implementation:**
   ```rust
   // 1. Define trait (or use existing TemperatureSensor)
   #[async_trait]
   pub trait HumiditySensor: Send + Sync {
       async fn read_humidity(&self) -> Result<f32>;
   }

   // 2. Implement for specific hardware
   pub struct DHT22Sensor { /* ... */ }

   // 3. Add to configuration
   pub struct ZoneConfig {
       // ... existing fields
       pub humidity_sensor: Option<String>,
   }

   // 4. Integrate into Thermostat
   ```

3. **Testing:**
   ```bash
   # Unit tests
   cargo test humidity

   # Mock tests
   cargo test --test mock_integration_tests

   # Format and lint
   cargo fmt
   cargo clippy
   ```

4. **Documentation:**
   - Update `README.md` with new sensor type
   - Add example to `config.example.json`
   - Document in `ARCHITECTURE.md`

### Fixing a Bug

**Example: Temperature sensor timeout**

1. **Reproduce:**
   ```bash
   # Enable debug logging
   RUST_LOG=thermos_rust::sensors=debug cargo run

   # Look for timeout errors in logs
   ```

2. **Locate code:**
   ```bash
   # Find sensor reading code
   grep -r "read_temperature" src/sensors/

   # Check timeout handling
   grep -r "timeout" src/sensors/
   ```

3. **Fix:**
   ```rust
   // Before: No timeout
   let temp = sensor.read().await?;

   // After: With timeout
   let temp = tokio::time::timeout(
       Duration::from_secs(1),
       sensor.read()
   ).await??;
   ```

4. **Test:**
   ```rust
   #[tokio::test]
   async fn test_sensor_timeout() {
       let sensor = SlowSensor::new(Duration::from_secs(5));
       let result = tokio::time::timeout(
           Duration::from_secs(1),
           sensor.read_temperature()
       ).await;
       assert!(result.is_err()); // Should timeout
   }
   ```

### Refactoring

**Example: Extract common error handling**

1. **Identify pattern:**
   ```rust
   // Pattern appears multiple times
   match sensor.read().await {
       Ok(temp) => { /* handle */ },
       Err(e) => {
           error!("Sensor error: {}", e);
           metrics::increment_error();
           DEFAULT_TEMP
       }
   }
   ```

2. **Extract function:**
   ```rust
   async fn read_with_fallback<S: TemperatureSensor>(
       sensor: &S,
       default: f32
   ) -> f32 {
       match sensor.read_temperature().await {
           Ok(reading) => reading.temperature_c,
           Err(e) => {
               error!("Sensor {} error: {}", sensor.id(), e);
               metrics::increment_error(sensor.id());
               default
           }
       }
   }
   ```

3. **Replace usage:**
   ```rust
   // Before
   match sensor.read().await { /* ... */ }

   // After
   let temp = read_with_fallback(&sensor, DEFAULT_TEMP).await;
   ```

4. **Verify:**
   ```bash
   cargo test
   cargo clippy  # Check for warnings
   ```

## Testing Strategy for AI Agents

### When to Run Which Tests

**Quick feedback (fast, cross-platform):**
```bash
cargo check           # Type check only (5-10s)
cargo test --lib      # Unit tests in src/ (30s)
cargo test --test mock_integration_tests  # Mock tests (1m)
```

**Full validation (requires Linux/Pi):**
```bash
cargo test            # All tests including hardware (2-5m)
cargo test --release  # Release mode tests (slower build, faster run)
```

**Coverage analysis:**
```bash
cargo tarpaulin --out Html  # Generate coverage report
# or
make test-coverage
```

### Writing Tests

**Unit test (in module):**
```rust
// src/sensors/mod.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_temperature_validation() {
        assert!(is_valid_temp(20.0));
        assert!(!is_valid_temp(100.0));  // Too hot
        assert!(!is_valid_temp(-10.0));  // Too cold
    }
}
```

**Async unit test:**
```rust
#[tokio::test]
async fn test_sensor_read() {
    let sensor = MockSensor::new(21.5);
    let reading = sensor.read_temperature().await.unwrap();
    assert_eq!(reading.temperature_c, 21.5);
}
```

**Integration test (tests/ directory):**
```rust
// tests/mock_integration_tests.rs
#[tokio::test]
async fn test_control_loop_hysteresis() {
    // Setup
    let sensor = MockSensor::new(19.0);  // Below target
    let thermostat = Thermostat::new(/* ... */);

    // Run one control cycle
    thermostat.tick().await;

    // Verify
    assert!(thermostat.is_heating());
}
```

### Mock Infrastructure

The project has comprehensive mocks in `src/mocks/`:

**Mock GPIO:**
```rust
use crate::mocks::gpio_mock::MockGpioController;

let gpio = MockGpioController::new();
gpio.expect_set_relay()
    .with(eq(23), eq(true))
    .times(1)
    .returning(|_, _| Ok(()));
```

**Mock Sensor:**
```rust
use crate::mocks::sensor_mock::MockSensor;

let sensor = MockSensor::new(21.5);
sensor.set_temperature(22.0);  // Simulate temperature change
```

**Mock State:**
```rust
use crate::mocks::state_mock::MockStateManager;

let state_mgr = MockStateManager::new();
state_mgr.expect_save_state()
    .returning(|_| Ok(()));
```

## Safety and Validation Checklist

Before suggesting changes, verify:

### ✅ Hardware Safety
- [ ] GPIO pin assignments don't conflict
- [ ] Relay operations are fail-safe (default OFF)
- [ ] Temperature ranges are validated (3-37°C)
- [ ] Power cycling won't damage sensors
- [ ] Changes are tested on actual hardware (for GPIO changes)

### ✅ Software Correctness
- [ ] Error handling is comprehensive
- [ ] State transitions are atomic
- [ ] Async operations are cancellation-safe
- [ ] Lock ordering prevents deadlocks
- [ ] Tests cover edge cases

### ✅ Protocol Compliance
- [ ] Matter attributes match specification
- [ ] Temperature conversions are correct (centi-degrees)
- [ ] State synchronization is bidirectional
- [ ] Commissioning flow works end-to-end

### ✅ Performance
- [ ] No blocking operations in control loop
- [ ] Memory allocations are minimal
- [ ] Lock contention is low
- [ ] Startup time is <2 seconds

### ✅ Documentation
- [ ] Public APIs have doc comments
- [ ] README.md is updated (if user-facing)
- [ ] ARCHITECTURE.md reflects design (if structural)
- [ ] Examples are included and tested

## Common Pitfalls to Avoid

### ❌ Don't: Ignore platform dependencies
```rust
// ❌ Bad: Will fail on non-Linux systems
use rppal::gpio::Gpio;
let gpio = Gpio::new()?;

// ✅ Good: Use conditional compilation
#[cfg(target_os = "linux")]
use rppal::gpio::Gpio;

#[cfg(not(target_os = "linux"))]
use crate::mocks::gpio_mock::MockGpio as Gpio;
```

### ❌ Don't: Block the async runtime
```rust
// ❌ Bad: Blocks Tokio thread
std::thread::sleep(Duration::from_secs(1));

// ✅ Good: Async sleep
tokio::time::sleep(Duration::from_secs(1)).await;
```

### ❌ Don't: Assume sensor readings are always valid
```rust
// ❌ Bad: No validation
let temp = sensor.read().await?;
set_temperature(temp);

// ✅ Good: Validate range
let temp = sensor.read().await?;
if temp < 3.0 || temp > 37.0 {
    return Err(SensorError::OutOfRange(temp));
}
```

### ❌ Don't: Modify hysteresis without understanding impact
```rust
// ❌ Bad: Too narrow (relay chatter)
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.1;

// ❌ Bad: Too wide (poor comfort)
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 2.0;

// ✅ Good: Tested value
const HYSTERESIS_UPPER_THRESHOLD_C: f32 = 0.5;
```

### ❌ Don't: Forget to update tests
```rust
// If you change function signature:
pub fn calculate_heating_state(temp: f32, target: f32) -> bool {
    // ...
}

// Update ALL tests that use it:
#[test]
fn test_heating_state() {
    assert!(calculate_heating_state(19.0, 20.0));  // Update call
}
```

## Environment Setup for AI Agents

### Recommended Analysis Tools

```bash
# Code structure analysis
cargo tree           # Dependency tree
cargo bloat          # Binary size breakdown
cargo outdated       # Check for updates

# Code quality
cargo clippy         # Lints
cargo fmt --check    # Format check
cargo audit          # Security vulnerabilities

# Documentation
cargo doc --open     # Generate and view docs
cargo rustdoc        # Advanced doc generation
```

### Environment Variables

```bash
# Logging
RUST_LOG=debug                    # Global debug
RUST_LOG=thermos_rust=trace       # Project trace
RUST_LOG=thermos_rust::devices=debug,thermos_rust::sensors=trace

# Build
CARGO_INCREMENTAL=1               # Faster dev builds
RUSTFLAGS="-C target-cpu=native"  # Optimize for CPU

# Testing
RUST_TEST_THREADS=1               # Serial test execution
RUST_BACKTRACE=1                  # Error backtraces
```

### Cross-Platform Development

**On macOS/Windows (without GPIO):**
```bash
# Use mock tests exclusively
cargo test --test mock_integration_tests

# Skip hardware-dependent tests
cargo test --lib

# Check compilation (won't run)
cargo check
```

**On Linux (with GPIO):**
```bash
# Full test suite
cargo test

# Integration tests only
cargo test --test integration_test
```

## Key Performance Metrics

When making changes, ensure these metrics remain within bounds:

| Metric | Target | Maximum | How to Measure |
|--------|--------|---------|----------------|
| Memory usage | <10 MB | 20 MB | `ps aux \| grep thermos` |
| CPU (idle) | <1% | 2% | `top` or `htop` |
| CPU (active) | <5% | 10% | During control loop |
| Startup time | <1s | 2s | `time ./thermos-rust` |
| Control loop | 6s | 7s | Prometheus `response_time` |
| Binary size | ~8 MB | 15 MB | `ls -lh target/release/thermos-rust` |

## Decision-Making Framework

When proposing changes, ask:

1. **Is this safe?**
   - Could it damage hardware?
   - Could it cause unsafe heating behavior?
   - Is error handling comprehensive?

2. **Is this correct?**
   - Does it follow Rust best practices?
   - Is the async code cancellation-safe?
   - Are there race conditions?

3. **Is this tested?**
   - Can it be tested without hardware?
   - Are edge cases covered?
   - Do existing tests still pass?

4. **Is this maintainable?**
   - Is the code self-documenting?
   - Are complex parts explained?
   - Does it follow project patterns?

5. **Is this performant?**
   - Does it meet performance targets?
   - Are allocations minimized?
   - Is it suitable for Raspberry Pi?

## Getting Help

If uncertain about:

- **Hardware behavior**: Check README.md hardware section, test carefully
- **Async patterns**: See `src/main.rs` for examples, Tokio docs
- **Matter protocol**: Read `src/matter/mod.rs`, Matter specification
- **Testing approach**: See `TESTING.md`, existing test files
- **Architecture**: Read `ARCHITECTURE.md`, ask for clarification

## Summary

Thermos-Rust is a **safety-critical**, **hardware-interfacing**, **async-native** Rust application. When working with this codebase:

1. **Prioritize safety** - This controls physical heating systems
2. **Understand hysteresis** - Core control logic, tested and validated
3. **Use async properly** - Tokio runtime, no blocking operations
4. **Test thoroughly** - Use mocks for rapid iteration, hardware for validation
5. **Document changes** - Especially for hardware/protocol/control logic

**Remember**: When in doubt, default to the safe state (heating OFF, error logged).

---

For more detailed information, see:
- `.claude/CLAUDE.md` - Claude-specific instructions
- `ARCHITECTURE.md` - Technical deep dive
- `TESTING.md` - Testing infrastructure
- `README.md` - User guide and setup

Good luck, and happy coding! 🦀
