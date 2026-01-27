<!--
UPDATE WHEN:
- Adding new entry points or key files
- Introducing new patterns
- Discovering non-obvious behavior

Helps quickly navigate the codebase when resuming work.
-->

# Code Landmarks

Quick reference to important parts of the codebase.

## Entry Points
| Location | Purpose |
|----------|---------|
| src/main.rs | Application entry, orchestration, signal handling |
| config.example.json | Configuration template with all options |

## Core Business Logic
| Location | Purpose |
|----------|---------|
| src/devices/mod.rs | Thermostat logic, hysteresis control, control loop |
| src/sensors/mod.rs | TemperatureSensor trait, CompositeSensor |
| src/sensors/onewire.rs | DS18B20 1-Wire implementation |
| src/gpio/mod.rs | Hardware abstraction, relay control, VCC power cycling |

## Configuration
| Location | Purpose |
|----------|---------|
| src/config/mod.rs | Config loading, validation, ZoneConfig struct |
| config.example.json | Example configuration with comments |

## Key Patterns
| Pattern | Example Location | Notes |
|---------|------------------|-------|
| Async task per zone | src/main.rs:run_zone() | Each zone runs independently |
| Shared GPIO controller | src/gpio/mod.rs:GpioController | Arc<Mutex> prevents conflicts |
| State persistence | src/persistence/mod.rs | RwLock for read-heavy access |
| Error resilience | src/devices/mod.rs | Defaults to safe state (heating OFF) |
| Hysteresis control | src/devices/mod.rs:HYSTERESIS_* | Prevents relay chatter |

## Testing
| Location | Purpose |
|----------|---------|
| tests/mock_integration_tests.rs | Cross-platform integration tests |
| src/mocks/gpio_mock.rs | Mock GPIO for testing |
| src/mocks/sensor_mock.rs | Mock temperature sensors |
| src/mocks/state_mock.rs | Mock state manager |

## Gotchas & Non-Obvious Behavior

| Location | Issue | Notes |
|----------|-------|-------|
| src/sensors/onewire.rs | Power cycling on error | VCC pin toggled to reset stuck sensors |
| src/devices/mod.rs | Temperature validation | Only 3-37°C accepted, else returns None |
| src/gpio/mod.rs | BCM pin numbering | Uses Broadcom GPIO numbers, not physical pins |
| src/config/mod.rs | Search order | CLI arg > /etc > ~/.config > ./config |
| src/main.rs | Shutdown handling | SIGTERM/SIGINT cleanup GPIO state |
| Cargo.toml | rppal dependency | Linux-only, requires GPIO permissions |

## Hardware Dependencies
| Component | Requirement | Notes |
|-----------|-------------|-------|
| GPIO access | Linux + rppal | Requires root or gpio group membership |
| 1-Wire bus | Kernel module w1-gpio | Enable via raspi-config |
| Redis | Optional | Falls back to in-memory if unavailable |
| Prometheus | Port 8080 | Metrics endpoint, no auth |

## Critical Constants
| Constant | Location | Value | Impact |
|----------|----------|-------|--------|
| HYSTERESIS_UPPER_THRESHOLD_C | src/devices/mod.rs | 0.5°C | Turn off when this far above setpoint |
| HYSTERESIS_LOWER_THRESHOLD_C | src/devices/mod.rs | -0.5°C | Turn on when this far below setpoint |
| CONTROL_LOOP_INTERVAL | src/main.rs | 6 seconds | How often to check temp and update |
| SENSOR_TIMEOUT | src/sensors/onewire.rs | 1 second | Max time for sensor read |
| POWER_CYCLE_OFF_TIME | src/gpio/mod.rs | 3 seconds | VCC off duration for sensor reset |
| POWER_CYCLE_RECOVERY_TIME | src/gpio/mod.rs | 5 seconds | Wait after VCC restore |
