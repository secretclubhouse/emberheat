# Claude Code Instructions for Thermos-Rust

This document provides guidance for Claude (AI assistant) when working with the Thermos-Rust project.

## Skills
**Read and follow these skills before writing any code:**
- `.claude/skills/base.md` - Core development principles, atomic todos, test-driven development
- `.claude/skills/security.md` - Security best practices, secret management, vulnerability prevention
- `.claude/skills/session-management.md` - Context preservation, decision logging, state tracking

**Project-specific instructions below supplement the skills with Rust/embedded systems guidance.**

## Project Overview

Thermos-Rust is a multi-zone thermostat system for hydronic heating, written in Rust. It's a Rust reimplementation of a Python project with multi-platform support.

**Supported Platforms:**
- **Linux (Raspberry Pi)** - Full implementation with Redis persistence, Prometheus metrics
- **ESP32-C6 (bare-metal)** - Embedded no_std implementation with WiFi, MQTT, NVS flash

**Language:** Rust (2021 edition)
**Architecture:** Cargo workspace with `thermos-core`, `thermos-linux`, `thermos-esp32`

> **Note:** Matter/HomeKit integration is a stub (not functional). See `MATTER_INTEGRATION.md` for planned rs-matter integration on ESP32.

## Key Project Characteristics

### Architecture Philosophy
- **Safety-first**: Memory safety via Rust's type system
- **Async-native**: Tokio runtime for concurrent operations
- **Modular design**: Clear separation of concerns
- **Error resilience**: Comprehensive error handling and recovery
- **Observable**: Prometheus metrics and extensive logging

### Critical Constraints
1. **Platform-specific GPIO**: rppal on Linux, esp-hal on ESP32
2. **Real-time control**: 6-second control loop must be reliable
3. **State persistence**: Redis (Linux) or NVS flash (ESP32)
4. **Safety critical**: Controls physical heating systems

## Project Structure

The project uses a Cargo workspace with three crates:

```
thermos-rust/
├── thermos-core/           - Platform-agnostic core library (no_std)
│   └── src/
│       ├── types.rs       - HeatingState, TemperatureReading, etc.
│       ├── sensor.rs      - TemperatureSensor trait
│       ├── gpio.rs        - GpioManager trait
│       ├── persistence.rs - StateStorage trait
│       └── control.rs     - Hysteresis control algorithm
├── thermos-linux/          - Linux/Raspberry Pi implementation
│   └── src/
│       ├── main.rs        - Tokio entry point
│       ├── config/        - JSON config loading
│       ├── devices/       - Thermostat bridge
│       ├── gpio/          - rppal GPIO
│       ├── sensors/       - 1-Wire sysfs, HTTP sensors
│       ├── persistence/   - Redis integration
│       └── metrics/       - Prometheus export
├── thermos-esp32/          - ESP32-C6 bare-metal implementation
│   └── src/
│       ├── main.rs        - Embassy entry point
│       ├── gpio_impl.rs   - esp-hal GPIO
│       ├── sensor_*.rs    - 1-Wire RMT, I2C BME280
│       ├── persistence_nvs.rs - NVS flash storage
│       ├── wifi.rs        - WiFi connectivity
│       └── mqtt.rs        - MQTT logging
└── src/                    - Legacy single-crate (being migrated)
```

## Working with This Project

### Before Making Changes

1. **Read relevant documentation:**
   - `README.md` - User-facing documentation, features, setup
   - `ARCHITECTURE.md` - Technical design, concurrency model, data flow
   - `TESTING.md` - Test infrastructure, mock implementations
   - `CONTRIBUTING.md` - Development guidelines

2. **Understand the module:**
   - Each module has clear responsibilities (see ARCHITECTURE.md)
   - Check inline documentation: `cargo doc --open`
   - Review existing tests for examples

3. **Check for breaking changes:**
   - GPIO pin assignments are critical (documented in config)
   - State persistence format affects Redis compatibility
   - Hysteresis constants affect heating behavior

### Testing Strategy

**thermos-linux (Raspberry Pi):**
```bash
# Full tests (requires Raspberry Pi)
cargo test -p thermos-linux

# Cross-platform mock tests (works anywhere)
cargo test --test mock_integration_tests
# OR
make test-mock
```

**thermos-esp32 (bare-metal):**
```bash
# Build and flash
cd thermos-esp32
cargo build --release
espflash flash --monitor target/riscv32imac-unknown-none-elf/release/thermos-esp32
```

**When to use mocks:**
- Development on non-Linux systems (macOS, Windows)
- Testing control logic without hardware
- CI/CD pipelines without GPIO access
- Rapid iteration on algorithms

**Critical test areas:**
- Hysteresis logic (prevent relay chatter)
- Sensor error handling (power cycling)
- State persistence (Redis/NVS round-trips)
- GPIO safety (no concurrent access)

### Code Style Guidelines

1. **Error handling:**
   - Use `Result<T, E>` for fallible operations
   - Define domain-specific error types with `thiserror`
   - Log errors at appropriate levels (error!, warn!, info!)
   - Provide context with error messages

2. **Async patterns:**
   - Use `async fn` for I/O operations
   - Prefer `tokio::select!` for cancellation
   - Use `Arc<RwLock<T>>` for read-heavy shared state
   - Use `Arc<Mutex<T>>` for write-heavy or fast operations

3. **Documentation:**
   - Document all public APIs with `///`
   - Include examples in doc comments
   - Explain "why" not just "what"
   - Update README.md for user-facing changes

4. **Concurrency:**
   - Each zone runs in separate async task
   - Shared GPIO controller protects against conflicts
   - State updates are atomic via RwLock
   - Respect control loop timing (6 seconds)

### Common Tasks

#### Adding a New Sensor Type

For thermos-linux:
1. Implement `TemperatureSensor` trait in `thermos-linux/src/sensors/`
2. Update `CompositeSensor` to include new type
3. Add configuration field in `ZoneConfig`
4. Write unit tests for sensor
5. Update `config.example.json` with example
6. Document in README.md

For thermos-esp32:
1. Implement sensor in `thermos-esp32/src/sensor_*.rs`
2. Use Embassy async and embedded-hal-async traits
3. Add to main control loop in `main.rs`

#### Modifying Hysteresis Logic

⚠️ **CRITICAL**: Changes affect heating behavior

1. Review constants in `src/devices/mod.rs`:
   - `HYSTERESIS_UPPER_THRESHOLD_C` (currently 0.5°C)
   - `HYSTERESIS_LOWER_THRESHOLD_C` (currently -0.5°C)
2. Write tests for edge cases (boundary conditions)
3. Test with hardware before deploying
4. Document rationale in commit message
5. Update ARCHITECTURE.md if design changes

#### Changing GPIO Pin Assignments

⚠️ **DANGEROUS**: Incorrect pins can damage hardware

1. Verify pin compatibility (BCM numbering)
2. Check for conflicts with existing assignments
3. Update `config.example.json`
4. Test with multimeter before powering relays
5. Document in README.md hardware section

### Build System

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Run tests
cargo test

# Check without building
cargo check

# Format code
cargo fmt

# Lint code
cargo clippy

# Generate documentation
cargo doc --open

# Run with logging
RUST_LOG=debug cargo run

# Test coverage
cargo tarpaulin --out Html
```

### Makefile Targets

```bash
make help           # Show all available targets
make build          # Build release binary
make test           # Run all tests (requires Linux)
make test-mock      # Run cross-platform mock tests
make test-coverage  # Generate coverage report
make install        # Install to /opt/thermos-rust
make install-service # Install systemd service
make docker-build   # Build Docker image
make clean          # Clean build artifacts
```

### Dependencies Management

**When adding dependencies:**
1. Justify the need (avoid dependency bloat)
2. Check license compatibility (MIT preferred)
3. Verify maintenance status (active development)
4. Consider platform compatibility (check no_std for thermos-core/esp32)
5. Update appropriate `Cargo.toml` with version constraints
6. Run `cargo update` to update lockfile
7. Test build on target architecture

**thermos-linux dependencies:**
- `tokio` - Async runtime (1.48)
- `rppal` - GPIO library (Linux-only, 0.22)
- `redis` - State persistence (0.27)
- `prometheus` - Metrics (0.14)
- `reqwest` - HTTP client (0.12)

**thermos-esp32 dependencies:**
- `esp-hal` - ESP32 HAL (1.0)
- `embassy-executor` - Async runtime (0.9)
- `esp-hal-rmt-onewire` - 1-Wire via RMT (0.4)
- `bme280` - I2C sensor (0.5)
- `rust-mqtt` - MQTT client (0.3)
- `esp-wifi` - WiFi (0.11)

**thermos-core dependencies:**
- `serde` - Serialization (no_std, 1.0)
- `heapless` - No-std collections (0.9)

### Configuration Format

Configuration is JSON-based (`config.json`):

```json
{
    "redis_url": "redis://localhost:6379",
    "Zone1": {
        "relay_pin": 23,
        "temp_pin": 4,
        "temp_id": "28-00000ab12345",
        "extra_sensor": "http://192.168.1.100/temp"
    }
}
```

**Search locations (in order):**
1. Command line argument: `thermos-rust /path/to/config.json`
2. System-wide: `/etc/thermos-rust/config.json`
3. User-specific: `~/.config/thermos-rust/config.json`
4. Local: `./config/config.json`

## Safety and Security Considerations

### Hardware Safety
- **GPIO conflicts**: Never assign same relay pin to multiple zones
- **Power cycling**: VCC control prevents sensor lock-ups
- **Valid ranges**: Sensor readings validated (3°C - 37°C)
- **Default state**: Heating defaults to OFF on errors

### Software Safety
- **Graceful shutdown**: SIGTERM/SIGINT handlers cleanup GPIO
- **State recovery**: Redis persistence for restart resilience
- **Error isolation**: Zone failures don't affect other zones
- **Timeout handling**: Network operations have timeouts

### Security
- **No authentication**: Redis and Prometheus should be firewalled
- **Config validation**: Invalid configs fail fast at startup
- **Privilege escalation**: Requires root for GPIO access

## Debugging Tips

### Enable Detailed Logging

```bash
# Trace level (very verbose)
RUST_LOG=trace ./thermos-rust

# Module-specific logging
RUST_LOG=thermos_rust::devices=debug ./thermos-rust

# Multiple modules
RUST_LOG=thermos_rust::devices=debug,thermos_rust::sensors=trace ./thermos-rust
```

### Common Issues

**"GPIO permission denied"**
- Run as root: `sudo ./thermos-rust`
- Or add user to gpio group: `sudo usermod -a -G gpio $USER`

**"1-Wire sensors not found"**
- Enable 1-Wire: `sudo raspi-config` → Interface Options → 1-Wire
- Check module loaded: `lsmod | grep w1`
- List devices: `ls /sys/bus/w1/devices/`

**"Redis connection refused"**
- Check Redis running: `sudo systemctl status redis`
- Test connectivity: `redis-cli ping`

### Metrics and Monitoring

Prometheus metrics available at `http://<ip>:8080/metrics`:

- `current_temperature{room="Zone1"}` - Current temp reading
- `target_temperature{room="Zone1"}` - Target setpoint
- `heat_status{room="Zone1"}` - Heating on/off (0/1)
- `response_time{room="Zone1"}` - Sensor read time
- `reset_error_count{room="Zone1"}` - Power cycle count

**Grafana dashboards:**
- Pre-built dashboard: `grafana/dashboards/thermostat.json`
- Import into Grafana for visualization

## Git Workflow

### Branch Strategy
- `main` - Stable, production-ready code
- `claude/*` - AI assistant feature branches
- Feature branches should be descriptive

### Commit Messages
Follow conventional commits format:
```
feat: add humidity sensor support
fix: correct hysteresis boundary condition
docs: update setup instructions
test: add integration tests for power cycling
refactor: extract sensor error handling
```

### Before Committing
```bash
cargo fmt              # Format code
cargo clippy           # Check for issues
cargo test             # Run tests
cargo test --test mock_integration_tests  # Cross-platform tests
```

## Documentation Standards

### Code Documentation
- All public items must have doc comments (`///`)
- Include examples for non-trivial functions
- Document panics, errors, safety requirements
- Cross-reference related items with `[`item`]`

### User Documentation
- **README.md**: User-facing, getting started
- **ARCHITECTURE.md**: Technical design decisions
- **TESTING.md**: Test infrastructure, mocking
- **CONTRIBUTING.md**: Development workflow
- **MATTER_INTEGRATION.md**: Re-adding Matter support

### Inline Comments
- Explain "why" not "what" (code shows what)
- Highlight non-obvious behavior
- Document workarounds with context
- Reference issues/PRs for historical context

## Performance Considerations

### Resource Constraints
- Target: Raspberry Pi (1-4 GB RAM)
- Memory budget: <10 MB total
- CPU usage: <5% per zone
- Startup time: <2 seconds

### Optimization Tips
- Use `Arc<RwLock>` for read-heavy shared state
- Avoid allocations in hot paths (control loop)
- Use `lazy_static` for global constants
- Profile with `cargo flamegraph` if needed
- Release builds use LTO, optimization level 3

### Timing Sensitivity
- Control loop: 6 seconds (configurable)
- Sensor timeout: 1 second
- Power cycle: 3s off, 5s recovery
- Redis writes: async, non-blocking
- Metrics update: per control loop iteration

## Documentation and Specs

### Project Specifications
- `_project_specs/overview.md` - Project vision, goals, success metrics
- `_project_specs/todos/active.md` - Current work items (atomic todo format)
- `_project_specs/todos/backlog.md` - Future work, prioritized
- `_project_specs/todos/completed.md` - Done items for reference

### Session Management
- `_project_specs/session/current-state.md` - Live session state (update after todos, decisions, or ~20 tool calls)
- `_project_specs/session/decisions.md` - Architectural/implementation decisions (append-only)
- `_project_specs/session/code-landmarks.md` - Important code locations for quick navigation

**When to update session files:**
- Update `current-state.md` after completing any todo, making decisions, or every 15-20 tool calls
- Log to `decisions.md` when choosing between approaches or making significant technical choices
- Update `code-landmarks.md` when adding new entry points or discovering non-obvious behavior

### Technical Documentation
- `README.md` - User-facing documentation, features, setup
- `ARCHITECTURE.md` - Technical design, concurrency model, data flow
- `TESTING.md` - Test infrastructure, mock implementations
- `CONTRIBUTING.md` - Development guidelines
- `MATTER_INTEGRATION.md` - Instructions for re-adding Matter support

## Quick Reference

### File Locations
- Config: `/etc/thermos-rust/config.json` or `~/.config/thermos-rust/config.json`
- Binary: `target/release/thermos-rust`
- Systemd service: `/etc/systemd/system/thermos-rust.service`
- Logs: `journalctl -u thermos-rust`

### Important Constants
- Hysteresis upper: 0.5°C (turn off)
- Hysteresis lower: -0.5°C (turn on)
- Default target: 21.1°C (70°F)
- Control loop: 6 seconds
- Sensor valid range: 3-37°C

### Port Usage
- Prometheus metrics: 8080
- Redis: 6379

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [rppal Documentation](https://docs.rs/rppal)
- [Prometheus Metrics](https://prometheus.io/docs/introduction/overview/)

## Contact and Support

- **Issues**: GitHub Issues for bugs and feature requests
- **Discussions**: GitHub Discussions for questions
- **Documentation**: See `/docs` directory for detailed guides

---

**Remember**: This controls physical heating systems. Safety and reliability are paramount. When in doubt, default to safe states (heating OFF, error logging).
