# Contributing to Thermos-Rust

Thank you for your interest in contributing to Thermos-Rust! This document provides guidelines and instructions for contributing.

## Code of Conduct

Please be respectful and constructive in all interactions. We aim to foster an inclusive and welcoming community.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/thermos-rust.git
   cd thermos-rust
   ```
3. **Set up development environment**:
   ```bash
   make setup
   ```

## Development Workflow

### 1. Create a Branch

Create a descriptive branch for your changes:

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
# Run all tests
make test

# Run with verbose output
make test-verbose

# Check code quality
make dev
```

### 4. Commit Your Changes

Write clear commit messages:

```bash
git add .
git commit -m "Add feature: description of your changes"
```

Good commit message format:
- Use present tense ("Add feature" not "Added feature")
- Keep first line under 72 characters
- Add detailed description if needed

### 5. Push and Create Pull Request

```bash
git push origin feature/your-feature-name
```

Then create a Pull Request on GitHub with:
- Clear title describing the change
- Description of what was changed and why
- Any relevant issue numbers

## Code Style

### Formatting

We use `rustfmt` for consistent formatting:

```bash
cargo fmt
```

All code must be formatted before submission.

### Linting

We use `clippy` for linting:

```bash
cargo clippy -- -D warnings
```

Fix all clippy warnings before submitting.

### Documentation

- Add doc comments (`///`) for all public items
- Include examples in doc comments where helpful
- Update README.md for user-facing changes

Example:

```rust
/// Calculate temperature in Fahrenheit from Celsius
///
/// # Arguments
///
/// * `celsius` - Temperature in degrees Celsius
///
/// # Returns
///
/// Temperature in degrees Fahrenheit
///
/// # Examples
///
/// ```
/// let fahrenheit = celsius_to_fahrenheit(20.0);
/// assert_eq!(fahrenheit, 68.0);
/// ```
pub fn celsius_to_fahrenheit(celsius: f32) -> f32 {
    celsius * 9.0 / 5.0 + 32.0
}
```

## Testing

### Unit Tests

Add unit tests in the same file as the code:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_something() {
        assert_eq!(2 + 2, 4);
    }

    #[tokio::test]
    async fn test_async_function() {
        let result = async_function().await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

Add integration tests in `tests/`:

```rust
// tests/my_integration_test.rs
#[tokio::test]
async fn test_end_to_end_flow() {
    // Test complete workflow
}
```

### Test Coverage

Aim for high test coverage:

```bash
make test-coverage
```

## Areas for Contribution

### High Priority

- Complete Matter protocol integration with rs-matter
- Improve error handling and recovery
- Add more comprehensive tests
- Performance optimizations
- Documentation improvements

### Feature Ideas

- Web UI for configuration and monitoring
- Additional sensor types (DHT22, BME280, etc.)
- Smart scheduling and automation
- Energy usage tracking
- Machine learning for predictive heating
- Home Assistant integration
- Grafana dashboard templates

### Bug Fixes

Check the [Issues](https://github.com/josht/thermos-rust/issues) page for known bugs.

## Specific Guidelines

### GPIO Code

- Always use `Arc<GpioController>` for shared access
- Test on actual Raspberry Pi hardware when possible
- Provide mock implementations for testing
- Handle errors gracefully

### Async Code

- Use `tokio` for async runtime
- Prefer `async/await` over manual futures
- Use `Arc<RwLock<T>>` for shared state in async contexts
- Avoid blocking operations in async code

### Configuration

- Validate all configuration inputs
- Provide clear error messages for invalid configs
- Maintain backward compatibility when possible
- Document all configuration options

### Dependencies

- Minimize external dependencies
- Prefer well-maintained crates
- Keep dependencies up to date
- Document why each dependency is needed

## Pull Request Checklist

Before submitting a PR, ensure:

- [ ] Code compiles without warnings
- [ ] All tests pass (`make test`)
- [ ] Code is formatted (`cargo fmt`)
- [ ] No clippy warnings (`cargo clippy`)
- [ ] Documentation is updated
- [ ] New tests are added for new features
- [ ] Commit messages are clear
- [ ] PR description explains the changes

## Review Process

1. **Automated checks** run on all PRs (formatting, tests, clippy)
2. **Code review** by maintainers
3. **Testing** on actual hardware (for GPIO-related changes)
4. **Merge** once approved

## Release Process

Releases follow semantic versioning (MAJOR.MINOR.PATCH):

- **MAJOR**: Breaking changes
- **MINOR**: New features (backward compatible)
- **PATCH**: Bug fixes

## Questions?

- Open an [Issue](https://github.com/josht/thermos-rust/issues) for questions
- Check existing issues and PRs first
- Be patient - this is maintained by volunteers

## License

By contributing, you agree that your contributions will be licensed under the MIT License.

## Thank You!

Every contribution helps make Thermos-Rust better. Thank you for your time and effort!
