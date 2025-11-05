# Matter Protocol Integration Setup Guide

This document describes how to build and use the Matter protocol integration in Thermos-Rust using the Connected Home IP library.

## Overview

Thermos-Rust now integrates with the **Matter protocol** (formerly Project CHIP) using the official **Connected Home IP** C++ library from the Connectivity Standards Alliance. This enables native integration with:

- **Apple HomeKit**
- **Google Home**
- **Amazon Alexa**
- **Samsung SmartThings**
- Any other Matter-compatible smart home ecosystem

## Architecture

```
┌────────────────────────────────────────────────┐
│     Smart Home Controllers                     │
│  (HomeKit, Google Home, Alexa, etc.)           │
└────────────────┬───────────────────────────────┘
                 │ Matter Protocol (UDP 5540)
      ┌──────────▼──────────┐
      │  Connected Home IP   │  (C++ Library)
      │  Matter Stack        │
      └──────────┬───────────┘
                 │ FFI (C ↔ Rust)
      ┌──────────▼──────────┐
      │  matter_wrapper.cpp  │  (C Wrapper)
      └──────────┬───────────┘
                 │ Rust FFI Bindings
      ┌──────────▼──────────┐
      │  MatterServer        │  (Rust)
      └──────────┬───────────┘
                 │
      ┌──────────▼──────────┐
      │  Thermostat Devices  │
      └──────────────────────┘
```

## Prerequisites

### System Dependencies (Debian/Ubuntu)

Install the required system packages:

```bash
sudo apt-get update
sudo apt-get install -y \
    git \
    gcc \
    g++ \
    pkg-config \
    cmake \
    libssl-dev \
    libdbus-1-dev \
    libglib2.0-dev \
    libavahi-client-dev \
    ninja-build \
    python3-venv \
    python3-dev \
    python3-pip \
    unzip \
    libgirepository1.0-dev \
    libcairo2-dev \
    libreadline-dev \
    default-jre
```

### Rust Toolchain

Ensure you have Rust 1.70 or later:

```bash
rustc --version
```

If not installed, get it from https://rustup.rs/

## Pre-built Libraries

**NEW:** To avoid rebuilding the entire Connected Home IP library (~15-30 minutes) on every clone, this project supports **pre-built libraries**.

The build system checks for libraries in this order:
1. **Pre-built libraries** in `lib/connectedhomeip/{platform}/` (committed to git)
2. **Submodule build** at `third_party/connectedhomeip/out/host/lib/`
3. **Stub implementation** (no actual Matter support)

### Supported Platforms

- `darwin-arm64` - macOS Apple Silicon
- `darwin-x64` - macOS Intel
- `linux-arm64` - Linux ARM64 (Raspberry Pi, etc.)
- `linux-x64` - Linux x86_64

### Using Pre-built Libraries (Recommended)

If pre-built libraries exist for your platform, the build will automatically use them:

```bash
# Just build normally
cargo build --release

# The build script will automatically detect and use pre-built libraries
```

**Note:** If you're on a platform where pre-built libraries are already committed, you can skip building the Connected Home IP library entirely!

### Building and Committing Libraries

If you need to build libraries for a new platform or upgrade the Matter version:

```bash
# Step 1: Build the Connected Home IP library (see Option 3 below)
cd third_party/connectedhomeip
source scripts/activate.sh
gn gen out/host --args='target_os="linux" target_cpu="arm64"'
ninja -C out/host
cd ../..

# Step 2: Copy libraries to lib/ directory
./scripts/copy-matter-libs.sh

# Step 3: Test the build
cargo build --release

# Step 4: Commit the libraries
git add lib/connectedhomeip/
git commit -m "Add pre-built Matter libraries for $(uname -s)-$(uname -m)"
git push
```

**Benefits:**
- New clones build immediately (no 30-minute wait)
- Consistent libraries across development and CI
- Easier onboarding for new developers
- No need to install GN/Ninja for basic development

**Trade-offs:**
- Larger repository size (~50-150 MB per platform)
- Need to rebuild when upgrading Matter version
- Platform-specific (may need multiple platforms)

## Building the Matter Integration

### Option 1: Stub Implementation (Quick Start)

The project includes a **stub implementation** that compiles without the full Connected Home IP library. This is useful for development and testing the Rust side of the integration.

```bash
# Standard build (uses stub implementation)
cargo build

# The stub will log warnings but will compile successfully
```

The stub implementation provides:
- All the Rust API structure
- Placeholder Matter functionality
- Test QR codes and pairing codes
- No actual Matter protocol communication

### Option 2: Using Pre-built Libraries (Recommended - Fast)

If pre-built libraries are available for your platform (check `lib/connectedhomeip/`):

```bash
# Just build - that's it!
cargo build --release
```

The build script will automatically detect and use pre-built libraries. **No need to build the Connected Home IP library!**

### Option 3: Building from Source (Required for New Platforms)

For actual Matter protocol support on a platform without pre-built libraries, you need to build the Connected Home IP library.

#### Step 1: Initialize the Submodule

The Connected Home IP library is included as a git submodule:

```bash
# Initialize and update the submodule (if not already done)
git submodule update --init --recursive
```

**Note:** This downloads ~500MB and may take several minutes.

#### Step 2: Build Connected Home IP Library

```bash
# Navigate to the Connected Home IP directory
cd third_party/connectedhomeip

# Activate the build environment
# This downloads GN, Ninja, and sets up Python dependencies
source scripts/activate.sh

# Generate build files
# For Raspberry Pi (ARM):
gn gen out/host --args='target_os="linux" target_cpu="arm64"'

# For macOS ARM64 (Apple Silicon):
# gn gen out/host --args='target_os="darwin" target_cpu="arm64"'

# For macOS x64 (Intel):
# gn gen out/host --args='target_os="darwin" target_cpu="x64"'

# For Linux x86_64:
# gn gen out/host --args='target_os="linux" target_cpu="x64"'

# Build the library (this takes 15-30 minutes on first build)
ninja -C out/host

# Return to project root
cd ../..
```

#### Step 3: Copy Libraries (Optional but Recommended)

Save time for future builds by copying the libraries:

```bash
# Copy built libraries to lib/ directory
./scripts/copy-matter-libs.sh

# Commit them so others don't need to rebuild
git add lib/connectedhomeip/
git commit -m "Add pre-built Matter libraries for $(uname -s)-$(uname -m)"
```

#### Step 4: Build Thermos-Rust

```bash
# Build with the full Matter library
cargo build --release
```

The build script (`build.rs`) will:
1. Check for pre-built libraries first
2. Fall back to submodule build if needed
3. Compile the C++ wrapper (`matter_wrapper.cpp`)
4. Link against the Matter libraries
5. Generate Rust FFI bindings

## Running with Matter Support

### 1. Start the Thermostat Server

```bash
# Run with default configuration
cargo run --release

# Or specify a config file
cargo run --release -- --config config/config.json
```

### 2. Commissioning Information

When the server starts, it will display commissioning information:

```
═══════════════════════════════════════════════
Matter Device Commissioning Information
═══════════════════════════════════════════════
QR Code: MT:Y.K0001800138400020202021
Manual Pairing Code: 34970112332
═══════════════════════════════════════════════
Scan the QR code with your Matter controller
(Apple Home, Google Home, Amazon Alexa, etc.)
═══════════════════════════════════════════════
```

### 3. Commission the Device

#### Apple HomeKit

1. Open the **Home** app on iPhone/iPad
2. Tap **+** → **Add Accessory**
3. Scan the QR code displayed in the terminal
4. OR enter the manual pairing code
5. Follow the on-screen instructions

#### Google Home

1. Open the **Google Home** app
2. Tap **+** → **Set up device** → **New device**
3. Select your home
4. Scan the QR code or enter pairing code
5. Complete setup

#### Amazon Alexa

1. Open the **Alexa** app
2. Tap **Devices** → **+** → **Add Device**
3. Select **Matter** device type
4. Scan QR code or enter code
5. Follow setup instructions

## Configuration

### Matter-Specific Settings

You can customize Matter settings in the code:

```rust
// In src/matter/ffi/wrapper.rs
let device_config = DeviceConfig {
    vendor_id: 0xFFF1,          // Your vendor ID
    product_id: 0x8001,         // Product ID
    device_name: "My Thermostat".to_string(),
    serial_number: "SERIAL123".to_string(),
    discriminator: 3840,        // 12-bit discriminator
    setup_passcode: 20202021,   // 8-digit setup code
    port: 5540,                 // Matter UDP port
};
```

**Important:** For production, you should:
1. Register for an official **Vendor ID** from the CSA
2. Use a unique **discriminator** and **passcode** per device
3. Implement proper **device attestation** certificates

### Per-Zone Configuration

Each thermostat zone is exposed as a separate Matter endpoint:

- **Endpoint 1:** Zone1
- **Endpoint 2:** Zone2
- **Endpoint 3:** Zone3
- etc.

## Network Requirements

### Firewall Rules

Matter uses UDP port **5540** for communication:

```bash
# Allow Matter traffic (if using firewall)
sudo ufw allow 5540/udp
```

### mDNS (Bonjour)

Matter uses **mDNS** for device discovery. Ensure Avahi is running:

```bash
# Check Avahi status
systemctl status avahi-daemon

# Start if not running
sudo systemctl start avahi-daemon
sudo systemctl enable avahi-daemon
```

## Troubleshooting

### Build Errors

**Error: Connected Home IP library not found**
```
Solution: Run `git submodule update --init --recursive`
```

**Error: `gn` command not found**
```
Solution: Source the environment script:
cd third_party/connectedhomeip
source scripts/activate.sh
```

**Error: linking with `cc` failed**
```
Solution: Check that all system dependencies are installed
Check that ninja build completed successfully
```

### Runtime Issues

**Device not discovered**
- Ensure firewall allows UDP 5540
- Check that avahi-daemon is running
- Verify device and controller are on same network
- Check for mDNS blockers in network configuration

**Commissioning fails**
- Verify the pairing code is entered correctly
- Check device logs for errors
- Ensure only one commissioning attempt at a time
- Try power cycling the controller

**Temperature not updating**
- Check sensor configuration in `config.json`
- Verify GPIO pins are accessible
- Review application logs for sensor errors

## Development Notes

### Code Organization

```
src/matter/
├── mod.rs              # Main Matter server implementation
└── ffi/
    ├── mod.rs          # FFI module exports
    ├── bindings.rs     # Raw FFI bindings to C
    ├── wrapper.rs      # Safe Rust wrapper
    ├── matter_wrapper.h    # C API header
    └── matter_wrapper.cpp  # C++ implementation
```

### Testing

Run tests with:

```bash
# Unit tests (use stub implementation)
cargo test

# Integration tests
cargo test --test integration_test
```

### Debugging

Enable debug logging:

```bash
# Set log level
export RUST_LOG=debug

# Run application
cargo run --release
```

View Matter stack logs:
```bash
# The C++ library logs to stderr
cargo run 2>&1 | grep -i matter
```

## Performance Considerations

### Resource Usage

- **Memory:** ~20-30 MB additional for Matter stack
- **CPU:** ~5-10% on Raspberry Pi 4 during commissioning
- **Network:** ~1-5 KB/s steady state

### Optimization Tips

1. **Release builds:** Always use `--release` for production
2. **Link-time optimization:** Enabled by default in `Cargo.toml`
3. **Stripped binaries:** Debug symbols removed in release builds

## Security Considerations

### Device Attestation

The current implementation uses **test credentials**. For production:

1. Obtain **Device Attestation Certificates** from CSA
2. Store certificates securely (not in code)
3. Implement secure storage for commissioning data

### Network Security

- Matter uses **encrypted communication** (TLS 1.3)
- Commissioning requires **physical access** (QR code/pairing code)
- Supports **multiple fabric** isolation

### Update Procedure

To update the Connected Home IP library:

```bash
# Step 1: Update the submodule
cd third_party/connectedhomeip
git fetch origin
git checkout <new-version-tag>
cd ../..
git add third_party/connectedhomeip
git commit -m "Update Connected Home IP submodule to <version>"

# Step 2: Rebuild the libraries
cd third_party/connectedhomeip
source scripts/activate.sh
ninja -C out/host
cd ../..

# Step 3: Copy updated libraries
./scripts/copy-matter-libs.sh

# Step 4: Test the build
cargo build --release

# Step 5: Commit the updated libraries
git add lib/connectedhomeip/
git commit -m "Update pre-built Matter libraries to <version>"
git push
```

**Note:** If using pre-built libraries, you should update them for all platforms you support (darwin-arm64, linux-arm64, etc.) to maintain consistency.

## Additional Resources

- **Matter Specification:** https://csa-iot.org/developer-resource/specifications-download-request/
- **Connected Home IP:** https://github.com/project-chip/connectedhomeip
- **Matter Documentation:** https://project-chip.github.io/connectedhomeip-doc/
- **CSA Developer Portal:** https://csa-iot.org/developer-resource/

## Support

For issues related to:
- **Thermos-Rust:** Open issue at https://github.com/josht/thermos-rust
- **Matter Protocol:** Consult Connected Home IP repository
- **Smart Home Integration:** Check controller-specific documentation

## License

The Matter integration uses:
- **Connected Home IP:** Apache 2.0 License
- **Thermos-Rust:** MIT License

See `third_party/connectedhomeip/LICENSE` for Matter library license details.
