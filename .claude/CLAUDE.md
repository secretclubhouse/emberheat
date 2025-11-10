# Claude Code Instructions for Thermos-Rust

This document provides guidance for Claude (AI assistant) when working with the Thermos-Rust project.

## Project Overview

Thermos-Rust is a multi-zone HomeKit thermostat system for hydronic heating, written in Rust with Matter protocol support. It's a Rust reimplementation of a Python project, targeting Raspberry Pi with GPIO control.

**Platform:** Linux (Raspberry Pi recommended)
**Language:** Rust (2021 edition)
**Protocol:** Matter/Thread for smart home integration
**Hardware:** GPIO-based relay control, 1-Wire temperature sensors

## Key Project Characteristics

### Architecture Philosophy
- **Safety-first**: Memory safety via Rust's type system
- **Async-native**: Tokio runtime for concurrent operations
- **Modular design**: Clear separation of concerns
- **Error resilience**: Comprehensive error handling and recovery
- **Observable**: Prometheus metrics and extensive logging

### Critical Constraints
1. **Hardware dependency**: GPIO operations require Linux (rppal crate)
2. **Real-time control**: 6-second control loop must be reliable
3. **State persistence**: Redis required for production use
4. **Matter protocol**: Complex C++ FFI integration via Connected Home IP
5. **Safety critical**: Controls physical heating systems

## Project Structure

```
src/
├── main.rs              - Entry point, orchestration, signal handling
├── config/mod.rs        - Configuration loading, validation (470 lines)
├── devices/mod.rs       - Thermostat logic, hysteresis control (581 lines)
├── gpio/mod.rs          - Hardware abstraction, relay control (372 lines)
├── sensors/             - Temperature sensor implementations
│   ├── mod.rs          - Sensor traits, composite sensors (318 lines)
│   └── onewire.rs      - DS18B20 1-Wire implementation (222 lines)
├── persistence/mod.rs   - Redis state management (333 lines)
├── metrics/mod.rs       - Prometheus metrics export (279 lines)
├── matter/mod.rs        - Matter protocol integration (1,174 lines)
└── mocks/               - Test infrastructure
    ├── gpio_mock.rs    - Mock GPIO for testing (190 lines)
    ├── sensor_mock.rs  - Mock sensors (228 lines)
    └── state_mock.rs   - Mock state manager (276 lines)
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
   - Matter cluster attributes must match specification
   - State persistence format affects Redis compatibility
   - Hysteresis constants affect heating behavior

### Testing Strategy

**Hardware-dependent code:**
```bash
# Full tests (requires Raspberry Pi)
cargo test

# Cross-platform mock tests (works anywhere)
cargo test --test mock_integration_tests
# OR
make test-mock
```

**When to use mocks:**
- Development on non-Linux systems (macOS, Windows)
- Testing control logic without hardware
- CI/CD pipelines without GPIO access
- Rapid iteration on algorithms

**Critical test areas:**
- Hysteresis logic (prevent relay chatter)
- Sensor error handling (power cycling)
- State persistence (Redis round-trips)
- Matter attribute conversions (centi-degrees)
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

1. Implement `TemperatureSensor` trait in `src/sensors/`
2. Update `CompositeSensor` to include new type
3. Add configuration field in `ZoneConfig`
4. Write unit tests for sensor
5. Update `config.example.json` with example
6. Document in README.md

#### Modifying Hysteresis Logic

⚠️ **CRITICAL**: Changes affect heating behavior

1. Review constants in `src/devices/mod.rs`:
   - `HYSTERESIS_UPPER_THRESHOLD_C` (currently 0.5°C)
   - `HYSTERESIS_LOWER_THRESHOLD_C` (currently -0.5°C)
2. Write tests for edge cases (boundary conditions)
3. Test with hardware before deploying
4. Document rationale in commit message
5. Update ARCHITECTURE.md if design changes

#### Adding Matter Cluster Attributes

1. Define attribute in `src/matter/mod.rs`
2. Implement read/write handlers
3. Add to thermostat state synchronization
4. Test with Matter controller (HomeKit, etc.)
5. Verify specification compliance
6. Update documentation

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

The project includes a comprehensive Makefile (286 lines):

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
4. Consider platform compatibility (Linux focus)
5. Update `Cargo.toml` with version constraints
6. Run `cargo update` to update lockfile
7. Test build on Raspberry Pi architecture

**Current critical dependencies:**
- `tokio` - Async runtime (pinned to 1.48)
- `rppal` - GPIO library (Linux-only, 0.22)
- `redis` - State persistence (0.27)
- `prometheus` - Metrics (0.14)
- `serde/serde_json` - Serialization (1.0)

### Matter Protocol Integration

The Matter integration uses FFI to Connected Home IP (C++ library):

**Build process:**
1. `build.rs` compiles C wrapper code
2. Generates Rust bindings
3. Links against CHIP library (if available)
4. Falls back to stub implementation for development

**Working with Matter code:**
- C++ library in `third_party/connectedhomeip` (git submodule)
- FFI bindings in `src/matter/ffi/`
- Rust wrapper in `src/matter/mod.rs`
- See `MATTER_SETUP.md` for full build instructions

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
- **Matter security**: Uses Matter's built-in commissioning flow
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

**"Matter commissioning fails"**
- Check UDP port 5540 open
- Verify device on same network
- Review logs for specific errors

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
docs: update Matter setup instructions
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

## Matter Protocol Specifics

### Thermostat Cluster (0x0201)

**Attributes:**
- `LocalTemperature` (0x0000): int16, centi-degrees C, read-only
- `OccupiedHeatingSetpoint` (0x0012): int16, centi-degrees C, read-write
- `SystemMode` (0x001C): enum8, Off(0x00) or Heat(0x04), read-write
- `ThermostatRunningState` (0x0029): bitmap8, bit 0 = heat on, read-only

**Conversions:**
```rust
// Celsius to centi-degrees
let centi_c = (temp_c * 100.0) as i16;

// Centi-degrees to Celsius
let temp_c = (centi_c as f32) / 100.0;
```

### Commissioning Flow
1. App starts Matter server
2. Generate QR code + pairing code (displayed in logs)
3. User scans QR code in smart home app
4. Matter handshake, cryptographic pairing
5. Device joins Matter fabric
6. Thermostats appear as accessories

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
- Matter protocol: 5540 (UDP)
- Redis: 6379

## Resources

- [Rust Book](https://doc.rust-lang.org/book/)
- [Tokio Tutorial](https://tokio.rs/tokio/tutorial)
- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [rppal Documentation](https://docs.rs/rppal)
- [Prometheus Metrics](https://prometheus.io/docs/introduction/overview/)

## Contact and Support

- **Issues**: GitHub Issues for bugs and feature requests
- **Discussions**: GitHub Discussions for questions
- **Documentation**: See `/docs` directory for detailed guides

---

**Remember**: This controls physical heating systems. Safety and reliability are paramount. When in doubt, default to safe states (heating OFF, error logging).
