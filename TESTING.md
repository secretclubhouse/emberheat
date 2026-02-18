# Testing Guide for EmberHeat

This document explains how to run tests for the multi-platform emberheat project.

## Overview

The EmberHeat project is organized as a workspace with three crates:
- **emberheat-core** - Platform-agnostic core library (unit tests only)
- **emberheat-linux** - Linux/Raspberry Pi implementation (mock and hardware tests)
- **emberheat-esp32** - ESP32-C6 bare-metal implementation (on-device testing)

The project includes comprehensive mocking infrastructure that allows tests to run on any platform without requiring actual Raspberry Pi GPIO hardware, temperature sensors, or Redis.

The project uses `serial_test` for Matter device tests to prevent global state leakage, ensuring proper test isolation while maintaining parallelism for other tests.

## Mock Components

### 1. MockGpioController (`emberheat-linux/src/mocks/gpio_mock.rs`)

Simulates GPIO operations without requiring rppal or Linux:

```rust
use emberheat::mocks::MockGpioController;

let gpio = MockGpioController::new();
gpio.init_relay(23).unwrap();
gpio.set_relay(23, RelayState::On).unwrap();
assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::On);
```

**Features:**

- Tracks relay states in memory
- Validates pin conflicts (duplicate initialization)
- Simulates VCC power cycling with async sleep
- Thread-safe with Arc<Mutex<>>

### 2. MockTemperatureSensor (`emberheat-linux/src/mocks/sensor_mock.rs`)

Simulates temperature sensor readings:

```rust
use emberheat::mocks::MockTemperatureSensor;

let sensor = MockTemperatureSensor::new("test-sensor", 22.5);
let reading = sensor.read_temperature().await.unwrap();
assert_eq!(reading.celsius, 22.5);

// Simulate sensor failure
sensor.set_failing();
assert!(sensor.read_temperature().await.is_err());

// Track read count
assert_eq!(sensor.get_read_count(), 2);
```

**Features:**

- Configurable temperature values
- Simulates sensor failures and unavailability
- Tracks read counts for test verification
- Builder pattern for complex scenarios
- Implements `TemperatureSensor` trait

### 3. MockStateManager (`emberheat-linux/src/mocks/state_mock.rs`)

In-memory state persistence without Redis:

```rust
use emberheat::mocks::MockStateManager;
use emberheat::persistence::ThermostatState;

let manager = MockStateManager::new();
let state = ThermostatState::default();

manager.save_state("Zone1", &state).await.unwrap();
let loaded = manager.load_state("Zone1").await.unwrap();

assert_eq!(manager.get_save_count(), 1);
assert_eq!(manager.get_load_count(), 1);
```

**Features:**

- HashMap-based in-memory storage
- Simulates connection failures
- Tracks save/load operation counts
- Pre-populate with initial states

## Running Tests

### emberheat-core (cross-platform)

```bash
# Run core library tests (works anywhere)
cargo test -p emberheat-core
```

### emberheat-linux (Raspberry Pi)

```bash
# Full tests (requires Raspberry Pi)
cargo test -p emberheat-linux

# Mock-only tests (works on any platform)
cargo test --test mock_integration_tests
# Or via Makefile:
make test-mock

# Run specific mock tests
cargo test mock_gpio
cargo test mock_sensor
cargo test mock_state
```

### emberheat-esp32 (bare-metal)

ESP32 tests run on-device since it's a no_std environment:

```bash
cd emberheat-esp32

# Build and flash with monitoring
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/emberheat-esp32

# Use defmt for embedded logging/assertions
```

### Run on macOS/Windows

On non-Linux platforms, the `rppal` crate won't compile. To run tests:

```bash
# Core library tests work everywhere
cargo test -p emberheat-core

# Mock integration tests work everywhere
cargo test --test mock_integration_tests
```

## Test Categories

### 1. Unit Tests (in each module)

- **GPIO**: RelayState conversions, error types
- **Sensors**: Temperature conversions, validation
- **Persistence**: Serialization, state management
- **Devices**: Heating state conversions, constants

### 2. Mock Tests (in `emberheat-linux/src/mocks/`)

- Mock GPIO operations
- Mock sensor readings
- Mock state persistence

### 3. Integration Tests (in `emberheat-linux/tests/mock_integration_tests.rs`)

- Hysteresis control logic simulation
- Multi-zone scenarios
- Sensor error recovery timing
- State persistence roundtrips
- Relay control logic
- Configuration validation

### 4. Hardware Integration Tests (requires Raspberry Pi)

- Actual GPIO pin control
- Real temperature sensor readings
- Redis state persistence
- End-to-end thermostat operation

## Test Coverage by Component

### emberheat-linux

| Component | Unit Tests | Mock Tests | Integration Tests | Hardware Tests |
|-----------|------------|------------|-------------------|----------------|
| GPIO | ✓ | ✓ | ✓ | Requires RPi |
| Sensors | ✓ | ✓ | ✓ | Requires sensors |
| Persistence | ✓ | ✓ | ✓ | Requires Redis |
| Devices | ✓ | ✓ | ✓ | Requires all |
| Config | ✓ | N/A | ✓ | N/A |
| Metrics | ✓ | N/A | ✓ | N/A |

### emberheat-core

| Component | Unit Tests |
|-----------|------------|
| types | ✓ |
| control | ✓ |
| config | ✓ |

### emberheat-esp32

| Component | On-Device Tests |
|-----------|-----------------|
| GPIO | ✓ (manual verification) |
| 1-Wire (RMT) | ✓ |
| I2C (BME280) | ✓ |
| WiFi | ✓ |
| MQTT | ✓ |
| NVS | ✓ |

## Writing Tests with Mocks

### Example: Testing Hysteresis Logic

```rust
use emberheat::mocks::{MockGpioController, MockTemperatureSensor, MockStateManager};

#[tokio::test]
async fn test_thermostat_hysteresis() {
    let gpio = MockGpioController::new();
    let sensor = MockTemperatureSensor::new("test", 19.0); // Below target
    let state_mgr = MockStateManager::new();

    // Initialize thermostat with mocks
    gpio.init_relay(23).unwrap();

    // Simulate temperature below target - should turn on
    let temp = sensor.read_temperature().await.unwrap();
    assert_eq!(temp.celsius, 19.0);

    // Apply control logic (would turn relay ON)
    gpio.set_relay(23, RelayState::On).unwrap();

    // Verify relay is on
    assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::On);

    // Simulate temperature rising above target
    sensor.set_temperature(22.0);
    let temp = sensor.read_temperature().await.unwrap();
    assert_eq!(temp.celsius, 22.0);

    // Apply control logic (would turn relay OFF)
    gpio.set_relay(23, RelayState::Off).unwrap();

    assert_eq!(gpio.get_relay_state(23).unwrap(), RelayState::Off);
}
```

### Example: Testing Sensor Fallback

```rust
#[tokio::test]
async fn test_sensor_fallback() {
    let primary = MockTemperatureSensor::new_failing("primary");
    let fallback = MockTemperatureSensor::new("fallback", 21.0);

    // Primary fails
    assert!(primary.read_temperature().await.is_err());

    // Fallback succeeds
    let reading = fallback.read_temperature().await.unwrap();
    assert_eq!(reading.celsius, 21.0);
}
```

### Example: Testing State Persistence

```rust
#[tokio::test]
async fn test_state_persistence() {
    let manager = MockStateManager::new();

    let state = ThermostatState {
        relay_pin: 23,
        temp_pin: 4,
        temp_id: "28-test".to_string(),
        target_temp: 22.0,
        target_state: 1,
        current_state: 0,
        last_updated: chrono::Utc::now(),
    };

    // Save state
    manager.save_state("Zone1", &state).await.unwrap();

    // Load state
    let loaded = manager.load_state("Zone1").await.unwrap();
    assert!(loaded.is_some());
    assert_eq!(loaded.unwrap().target_temp, 22.0);

    // Verify operation counts
    assert_eq!(manager.get_save_count(), 1);
    assert_eq!(manager.get_load_count(), 1);
}
```

## Continuous Integration

For CI/CD pipelines on non-Raspberry Pi hardware:

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test-mocks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v2
      - uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
      - name: Run mock tests
        run: cargo test --test mock_integration_tests

  test-raspberry-pi:
    runs-on: [self-hosted, raspberry-pi]
    steps:
      - uses: actions/checkout@v2
      - name: Run all tests
        run: cargo test
```

## Troubleshooting

### "rppal" fails to compile on macOS/Windows

**Solution**: This is expected. Use mock tests only:

```bash
cargo test --test mock_integration_tests
```

### "Redis connection error" during tests

**Solution**: Use `MockStateManager` instead of real Redis:

```rust
// Instead of:
let state_mgr = StateManager::new("redis://localhost").await?;

// Use:
let state_mgr = MockStateManager::new();
```

### GPIO tests fail with "Permission denied"

**Solution**: GPIO tests require root on Raspberry Pi:

```bash
sudo cargo test
```

Or use mocks:

```bash
cargo test --test mock_integration_tests
```

## Best Practices

1. **Use mocks for unit tests**: Keep tests fast and portable
2. **Use real hardware for integration tests**: Verify actual behavior on target platform
3. **Document test requirements**: Clearly indicate which tests need hardware
4. **Track mock operations**: Use counters to verify interactions
5. **Test error cases**: Use mocks to simulate failures
6. **Keep mocks simple**: Don't over-engineer mock behavior

## Code Coverage

Generate test coverage reports using `cargo-llvm-cov`:

```bash
# Install cargo-llvm-cov
cargo install cargo-llvm-cov

# Generate HTML coverage report
cargo llvm-cov --html

# Generate LCOV format (for CI/CD)
cargo llvm-cov --lcov --output-path lcov.info

# Or use the Makefile
make test-coverage
```

The project targets 75-85% code coverage, with comprehensive coverage for:
- Control logic and hysteresis algorithms
- Matter protocol conversion functions
- Error handling and recovery mechanisms
- State management and persistence

## Future Improvements

- [ ] Add property-based testing with `proptest`
- [ ] Add benchmark tests with `criterion`
- [ ] Add mutation testing
- [x] Add coverage reporting with `cargo-llvm-cov`
- [ ] Create test fixtures for common scenarios
- [ ] Add integration test Docker containers
