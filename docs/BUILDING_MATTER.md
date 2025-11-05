# Building Matter/Connected Home IP Libraries

This guide explains how to set up and build the Matter (Connected Home IP) libraries for Thermos-Rust.

## Overview

Instead of using git submodules, we download Matter source code from GitHub releases. This approach:

- **Simpler**: No submodule complexity or nested submodules
- **Reliable**: Specific stable releases instead of moving git commits
- **Flexible**: Easy to switch between versions
- **Efficient**: Download only when needed, not tracked in git

## Quick Start

For most users, just run:

```bash
make matter-setup
```

This will:
1. Download Matter v1.4.0.0 source from GitHub
2. Build the required libraries (15-30 minutes)
3. Copy libraries to `lib/connectedhomeip/<platform>/`

Then build thermos-rust normally:

```bash
make build
```

## Detailed Steps

### 1. Download Matter Source

```bash
make matter-download
```

This downloads the Matter source code to `third_party/connectedhomeip/`.

To specify a different version:

```bash
MATTER_VERSION=v1.3.0.0 make matter-download
```

### 2. Build Matter Libraries

```bash
make matter-build
```

This:
- Activates the Matter build environment
- Configures the build with GN
- Compiles libraries with Ninja
- Takes 15-30 minutes depending on your system

**Build Configuration:**
- Network: WiFi enabled, BLE disabled
- Crypto: BoringSSL
- Optimization: Size-optimized release build

### 3. Copy Libraries

```bash
make matter-libs
```

This copies the built libraries from `third_party/connectedhomeip/out/host/lib/` to `lib/connectedhomeip/<platform>/`.

The pre-built libraries should be committed to git so other developers don't need to rebuild.

## Platform Support

The build scripts auto-detect your platform:

| Platform | String | Notes |
|----------|--------|-------|
| macOS Apple Silicon | darwin-arm64 | M1/M2/M3 Macs |
| macOS Intel | darwin-x64 | Intel Macs |
| Linux ARM64 | linux-arm64 | Raspberry Pi 4/5 |
| Linux x64 | linux-x64 | Most Linux systems |

## Troubleshooting

### Build Fails with "ARM GCC package error"

This is a known issue with the upstream CIPD package. The build script works around this by disabling ARM GCC (not needed for host builds).

If you see errors about `qMemTags.html`, the scripts already handle this.

### Build Fails with Python Environment Error

The Matter build needs Python but your system Python may be managed. The build script uses Matter's bundled Python environment.

### Missing Dependencies

The Matter build requires:
- C/C++ compiler (clang or gcc)
- Python 3
- wget or curl

On Ubuntu/Debian:
```bash
sudo apt-get update
sudo apt-get install build-essential python3 wget
```

On macOS:
```bash
xcode-select --install
brew install python3 wget
```

### Build Takes Too Long

The first build can take 20-30 minutes. Subsequent builds are faster due to caching.

To speed up:
- Use pre-built libraries from another developer
- Build only on one machine, commit libraries for team

### Clean Build

To start fresh:

```bash
make matter-clean      # Remove downloaded source
rm -rf lib/connectedhomeip/<platform>/  # Remove libraries
make matter-setup      # Rebuild everything
```

## Using Pre-Built Libraries

If a teammate has already built libraries for your platform:

1. They run: `make matter-libs` and commit the libraries
2. You run: `git pull` to get the libraries
3. You run: `make build` - no Matter build needed!

The libraries are platform-specific, so you need libraries for:
- Development: Your local platform (darwin-arm64, linux-x64, etc.)
- Production: Raspberry Pi (linux-arm64)

## Customizing the Build

### Different Matter Version

```bash
MATTER_VERSION=v1.2.0.0 ./scripts/download-matter.sh
make matter-build
```

### Custom Build Configuration

Edit [scripts/build-matter.sh](/workspace/scripts/build-matter.sh) and modify the `GN_ARGS` array:

```bash
GN_ARGS=(
    'chip_config_network_layer_ble=true'   # Enable BLE
    'chip_enable_wifi=true'
    'chip_crypto="mbedtls"'                # Use mbedTLS instead of BoringSSL
    'is_debug=true'                        # Debug build
)
```

## Integration with Cargo

The Rust build script ([build.rs](/workspace/build.rs)) automatically finds libraries in this order:

1. Pre-built libraries: `lib/connectedhomeip/<platform>/`
2. Fresh build: `third_party/connectedhomeip/out/host/lib/`
3. Stub implementation if neither exists

This means:
- Production builds use committed pre-built libraries
- Development can use local builds
- CI works without building Matter (uses pre-built)

## Directory Structure

```
thermos-rust/
├── third_party/
│   ├── connectedhomeip/              # Downloaded source (gitignored)
│   │   ├── out/host/lib/            # Build output
│   │   └── ...
│   └── README.md
├── lib/
│   └── connectedhomeip/              # Pre-built libraries (committed)
│       ├── darwin-arm64/
│       │   ├── libchip.a
│       │   ├── libCHIPController.a
│       │   └── ...
│       ├── linux-arm64/
│       └── linux-x64/
└── scripts/
    ├── download-matter.sh            # Download script
    ├── build-matter.sh               # Build script
    └── copy-matter-libs.sh           # Copy script
```

## CI/CD Integration

For GitHub Actions or other CI:

```yaml
# Option 1: Use pre-built libraries (fast, no build)
- name: Build
  run: cargo build --release

# Option 2: Build Matter from source (slow, first time only)
- name: Setup Matter
  run: make matter-setup
- name: Build
  run: cargo build --release
```

Most CI should use Option 1 (pre-built libraries already in git).

## Development Workflow

Typical workflow:

```bash
# First time setup (one person on team)
make matter-setup              # Download, build, copy
git add lib/connectedhomeip/
git commit -m "Add Matter libraries for linux-x64"

# Everyone else
git pull                       # Get pre-built libraries
make build                     # Just build thermos-rust

# Updating Matter version
MATTER_VERSION=v1.5.0.0 make matter-setup
git add lib/connectedhomeip/
git commit -m "Update Matter to v1.5.0.0"
```

## Advanced Topics

### Cross-Compilation

To build for a different target:

```bash
# Build for Raspberry Pi on x64 Linux
# (Requires ARM cross-compilation toolchain)
./scripts/build-matter.sh
# Then manually copy to raspberry pi target
```

### Debugging Matter Integration

Enable Matter SDK logging in your Rust code:

```rust
#[cfg(matter_sdk_available)]
{
    // Matter SDK is available
}
#[cfg(matter_sdk_stub)]
{
    // Using stub implementation
}
```

The `build.rs` script sets these cfg flags automatically.
