# Contributing to EmberHeat

Thank you for your interest in contributing to EmberHeat! This document provides guidelines for contributing.

## Code of Conduct

Please be respectful and constructive in all interactions.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/emberheat.git
   cd emberheat
   ```
3. **Set up development environment**:
   ```bash
   make setup
   ```

## Project Structure

The project is a Cargo workspace with four crates:

- **emberheat-core** - Platform-agnostic core library (no_std compatible)
- **emberheat-matter** - Matter protocol types and cluster definitions (no_std compatible)
- **emberheat-esp32** - ESP32-C6 implementation (active development)
- **emberheat-linux** - Linux/Raspberry Pi implementation (preserved, not active)

Default build commands include `emberheat-core`, `emberheat-matter`, and `emberheat-esp32`. To work on `emberheat-linux`, use explicit package flags:

```bash
cargo build -p emberheat-linux
cargo test -p emberheat-linux
```

## Development Workflow

### 1. Create a Branch

```bash
git checkout -b feature/your-feature-name
# or
git checkout -b fix/bug-description
```

### 2. Make Your Changes

- Write clear, concise code
- Follow Rust conventions and idioms
- Add tests for new functionality
- Update documentation as needed

### 3. Test Your Changes

```bash
# Run tests (emberheat-core and emberheat-matter)
make test

# Check code quality (format + lint + test)
make dev
```

### 4. Commit Your Changes

The repository has a pre-commit hook that runs:
- `cargo fmt` - formatting check
- `cargo clippy` - linting
- `cargo test` - tests

```bash
git add .
git commit -m "feat: description of your changes"
```

Use conventional commit format:
- `feat:` - new feature
- `fix:` - bug fix
- `docs:` - documentation
- `refactor:` - code refactoring
- `test:` - adding tests
- `chore:` - maintenance

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub.

## Code Style

### Formatting

```bash
cargo fmt
```

### Linting

```bash
cargo clippy -p emberheat-core -- -D warnings
```

### Documentation

Add doc comments (`///`) for all public items:

```rust
/// Calculate temperature in Fahrenheit from Celsius.
///
/// # Arguments
///
/// * `celsius` - Temperature in degrees Celsius
///
/// # Returns
///
/// Temperature in degrees Fahrenheit
pub fn celsius_to_fahrenheit(celsius: f32) -> f32 {
    celsius * 9.0 / 5.0 + 32.0
}
```

## Testing

### Unit Tests

Add unit tests in the same file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        assert_eq!(2 + 2, 4);
    }
}
```

### Running Tests

```bash
# Run emberheat-core tests
make test

# With verbose output
make test-verbose
```

## ESP32 Development

### Requirements

- ESP32-C6 development board
- `espflash` tool installed: `cargo install espflash`
- RISC-V target: `rustup target add riscv32imac-unknown-none-elf`

### Building

```bash
# MQTT mode (requires WIFI_SSID and WIFI_PASSWORD in .env.local)
make build-esp32

# Matter mode (WiFi handled by Matter commissioning)
make build-matter

# Matter with dual-radio (C6 WiFi + H2 BLE over UART)
make build-matter-dual-radio
```

### Flashing

```bash
# MQTT mode
make flash          # Flash only
make flash-monitor  # Flash and open serial monitor

# Matter mode
make flash-matter              # Flash only
make flash-monitor-matter      # Flash and open serial monitor
```

### Feature Flags

| Feature | Description |
|---------|-------------|
| `matter` | Enable Matter protocol support (disables standalone MQTT task) |
| `external-ble` | Use ESP32-H2 for BLE over UART HCI (requires `matter`) |
| `onewire-rmt` | Use RMT peripheral for 1-Wire (currently disabled — compatibility issue) |

## Areas for Contribution

See [GitHub Issues](https://github.com/secretclubhouse/emberheat/issues) for the current backlog.

### MVP

- Resolve 1-Wire sensor dependency ([#1](https://github.com/secretclubhouse/emberheat/issues/1))
- Hardware sensor integration and testing ([#2](https://github.com/secretclubhouse/emberheat/issues/2), [#4](https://github.com/secretclubhouse/emberheat/issues/4))

### Feature Ideas

- Runtime zone configuration ([#5](https://github.com/secretclubhouse/emberheat/issues/5))
- Sensor auto-discovery ([#6](https://github.com/secretclubhouse/emberheat/issues/6))
- OTA firmware updates ([#7](https://github.com/secretclubhouse/emberheat/issues/7))
- Web dashboard ([#11](https://github.com/secretclubhouse/emberheat/issues/11))
- Time-based scheduling ([#9](https://github.com/secretclubhouse/emberheat/issues/9))

## Pull Request Checklist

Before submitting:

- [ ] Code compiles without warnings
- [ ] All tests pass (`make test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`make lint`)
- [ ] Documentation updated if needed
- [ ] Commit messages follow conventional format

## Questions?

Open an [Issue](https://github.com/secretclubhouse/emberheat/issues) for questions or discussion.

## License

By contributing, you agree that your contributions will be dual-licensed under the MIT License and Apache License 2.0.
