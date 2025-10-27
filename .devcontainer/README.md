# Thermos-Rust Development Container

This directory contains the VSCode DevContainer configuration for developing Thermos-Rust in a consistent, reproducible Linux environment.

## What's Included

### Development Environment

- **Base Image**: Debian Trixie with Rust 1.90
- **Rust Toolchain**: rustc, cargo, rustfmt, clippy, rust-analyzer
- **Build Tools**: gcc, g++, make, pkg-config
- **Debugging**: gdb, lldb
- **Additional Tools**:
  - cargo-watch (automatic recompilation on file changes)
  - cargo-expand (macro expansion viewer)
  - cargo-tree (dependency tree visualization)

### Services

- **Redis 7**: Running on localhost:6379 for state persistence
  - Data persisted in Docker volume `redis-data`
  - Configured with AOF (Append Only File) persistence

### VSCode Extensions

- **rust-analyzer**: Rust language server
- **CodeLLDB**: Native debugger support
- **Even Better TOML**: TOML file support
- **Dependi**: Dependency management helper
- **Resource Monitor**: System resource monitoring

### Port Forwarding

- **5540**: Matter server port (UDP)
- **8080**: Prometheus metrics HTTP endpoint

## Prerequisites

1. **Docker Desktop** (or Docker Engine + Docker Compose)
   - [Install Docker Desktop](https://www.docker.com/products/docker-desktop)

2. **Visual Studio Code**
   - [Download VSCode](https://code.visualstudio.com/)

3. **Dev Containers Extension**
   - Install from VSCode: `ms-vscode-remote.remote-containers`
   - Or via command line: `code --install-extension ms-vscode-remote.remote-containers`

## Getting Started

### Option 1: VSCode Command Palette (Recommended)

1. Open the project folder in VSCode
2. Press `F1` or `Cmd+Shift+P` (Mac) / `Ctrl+Shift+P` (Windows/Linux)
3. Type: `Dev Containers: Reopen in Container`
4. Wait for the container to build (first time only, ~5-10 minutes)
5. Once ready, you'll have a full Linux development environment

### Option 2: VSCode Notification

1. Open the project folder in VSCode
2. Click "Reopen in Container" when prompted in the bottom-right corner

### Option 3: Command Line

```bash
# From project root
code .

# Then use Command Palette to reopen in container
```

## First Time Setup

When the container first starts, it will:

1. Build the Debian-based development image
2. Install all system dependencies
3. Set up Rust toolchain and components
4. Install cargo tools (watch, expand, tree)
5. Start Redis service
6. Mount your project directory at `/workspace`

This process takes 5-10 minutes on the first run. Subsequent starts are much faster (seconds).

## Verifying the Environment

Once inside the container, verify everything works:

```bash
# Check Rust version
rustc --version
cargo --version

# Check Redis connectivity
redis-cli ping
# Should respond: PONG

# Check project builds
cargo check

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

 # Development Workflow

### Building

```bash
# Check compilation (fast, no binary)
cargo check

# Build debug version
cargo build

# Build release version
cargo build --release

# Watch mode (auto-rebuild on changes)
cargo watch -x check
```

### Testing

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test test_relay_state_conversion

# Run with logging
RUST_LOG=debug cargo test -- --nocapture
```

### Linting and Formatting

```bash
# Format code
cargo fmt

# Check formatting (CI mode)
cargo fmt -- --check

# Run clippy linter
cargo clippy

# Clippy with all warnings
cargo clippy -- -W clippy::pedantic
```

### Running the Application

```bash
# Run with default config
cargo run

# Run with custom config
cargo run -- /path/to/config.json

# Run with debug logging
RUST_LOG=debug cargo run

# Run with trace logging for specific module
RUST_LOG=thermos_rust::devices=trace cargo run
```

### Monitoring

```bash
# Watch Redis state
redis-cli
> KEYS thermostat:*
> GET thermostat:Zone1

# Check metrics endpoint (when app is running)
curl http://localhost:8080/metrics
```

## Important Notes

### GPIO Hardware Access

**Warning**: The devcontainer runs on x86_64/arm64 architecture, not on actual Raspberry Pi hardware. This means:

- ❌ **GPIO operations will NOT work** - No physical GPIO pins available
- ❌ **1-Wire sensors will NOT work** - No `/sys/bus/w1/devices/` filesystem
- ✅ **Code will compile** - All dependencies available
- ✅ **Tests will run** - Unit tests don't require hardware
- ✅ **Logic can be developed** - Full Rust toolchain available

### Testing on Actual Hardware

To test GPIO functionality, you must:

1. Build for Raspberry Pi (cross-compile or build on device)
2. Deploy to actual Raspberry Pi hardware
3. Run with proper permissions (root or gpio group)

See the main [README.md](../README.md) for deployment instructions.

### Network Mode

The container uses `network_mode: host` to:
- Simplify Matter protocol development (UDP broadcast/multicast)
- Allow Redis on standard port 6379
- Enable direct access to Matter port 5540

This means ports are directly exposed on your host machine.

## Customization

### Changing Rust Version

Edit [Dockerfile](Dockerfile):

```dockerfile
FROM rust:1.83-slim-bookworm  # Change version here
```

### Adding VSCode Extensions

Edit [devcontainer.json](devcontainer.json):

```json
"extensions": [
  "rust-lang.rust-analyzer",
  "your-extension-id"
]
```

### Adding System Packages

Edit [Dockerfile](Dockerfile):

```dockerfile
RUN apt-get update && apt-get install -y \
    your-package-here \
    && rm -rf /var/lib/apt/lists/*
```

### Cargo Tools

Pre-installed tools can be customized in [Dockerfile](Dockerfile):

```dockerfile
RUN cargo install cargo-watch cargo-expand your-tool-here
```

## Troubleshooting

### Container Won't Start

```bash
# Rebuild container from scratch
# Command Palette: Dev Containers: Rebuild Container

# Or via Docker CLI:
docker-compose -f .devcontainer/docker-compose.yml down -v
docker-compose -f .devcontainer/docker-compose.yml build --no-cache
```

### Redis Connection Issues

```bash
# Inside container, check Redis is running
redis-cli ping

# If not responding:
sudo systemctl status redis  # If using systemd
docker-compose logs redis    # If using docker-compose
```

### Slow Builds

First builds are slow. To speed up:

1. **Use cargo cache** (already configured in docker-compose.yml)
2. **Use sccache**: Add to Dockerfile
3. **Reduce check frequency**: Use `cargo check` instead of `cargo build`

### Permission Issues

If you encounter permission errors:

```bash
# Inside container
whoami  # Should be 'vscode'
groups  # Should include 'vscode'

# Fix permissions (if needed)
sudo chown -R vscode:vscode /workspace
```

## VS Code Settings

The devcontainer configures these Rust-specific settings:

- **Format on save**: Enabled for Rust files
- **Clippy**: Used for `cargo check` command
- **All features**: Enabled in rust-analyzer
- **LLDB**: Configured for debugging

## Rebuilding the Container

When you modify the devcontainer configuration:

1. **Dockerfile changes**: Requires rebuild
   - Command Palette → `Dev Containers: Rebuild Container`

2. **devcontainer.json changes**: May require reload
   - Command Palette → `Dev Containers: Reopen in Container`

3. **docker-compose.yml changes**: Requires rebuild
   - Rebuild container as above

## CI/CD Integration

This devcontainer configuration can be used for:

- **GitHub Actions**: Use devcontainer action
- **GitLab CI**: Use Docker-in-Docker
- **Local CI**: Use docker-compose directly

Example for local testing:

```bash
# Run tests in container
docker-compose -f .devcontainer/docker-compose.yml run --rm dev cargo test

# Build release
docker-compose -f .devcontainer/docker-compose.yml run --rm dev cargo build --release
```

## Resources

- [VSCode Dev Containers Documentation](https://code.visualstudio.com/docs/devcontainers/containers)
- [Dev Container Specification](https://containers.dev/)
- [Rust in VSCode](https://code.visualstudio.com/docs/languages/rust)
- [rust-analyzer Manual](https://rust-analyzer.github.io/manual.html)

## Support

For issues specific to the devcontainer setup:

1. Check this README
2. Check [VSCode Dev Containers Troubleshooting](https://code.visualstudio.com/docs/devcontainers/troubleshooting)
3. Open an issue in the project repository
