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

The project is a Cargo workspace with three crates:

- **emberheat-core** - Platform-agnostic core library (no_std compatible)
- **emberheat-esp32** - ESP32-C6 implementation (active development)
- **emberheat-linux** - Linux/Raspberry Pi implementation (preserved, not active)

Default build commands only include `emberheat-core` and `emberheat-esp32`. To work on `emberheat-linux`, use explicit package flags:

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
# Run tests (emberheat-core)
make test

# Check code quality
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

### Building

```bash
make build-esp32
```

### Flashing

```bash
make flash          # Flash only
make flash-monitor  # Flash and open serial monitor
```

### Requirements

- ESP32-C6 development board
- `espflash` tool installed
- RISC-V target: `rustup target add riscv32imac-unknown-none-elf`

## Areas for Contribution

### High Priority

- Matter protocol integration with rs-matter
- Improved error handling
- Additional sensor support
- Documentation improvements

### Feature Ideas

- Web UI for configuration
- Home Assistant integration
- Smart scheduling

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

By contributing, you agree that your contributions will be licensed under the MIT License.
