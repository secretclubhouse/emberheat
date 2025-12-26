# Matter Protocol Re-Integration Guide

This document describes how to re-add Matter/HomeKit support to thermos-rust.
The Connected Home IP integration was removed to simplify the build process.

## Overview

Matter (formerly Project CHIP) is an open-source connectivity standard for smart home devices.
It enables integration with Apple HomeKit, Google Home, Amazon Alexa, and other ecosystems.

### What Was Removed

1. **FFI Layer** (`src/matter/ffi/`):
   - `matter_wrapper.h` - C header defining the wrapper API
   - `matter_wrapper.cpp` - C++ implementation wrapping Connected Home IP
   - `bindings.rs` - Rust FFI bindings to the C wrapper
   - `wrapper.rs` - Safe Rust wrapper around unsafe FFI
   - `mod.rs` - FFI module definition

2. **Build System**:
   - `build.rs` - Complex build script that compiled C++ and linked Matter library
   - Matter-related Makefile targets (`matter-download`, `matter-build`, etc.)
   - Docker build for Matter libraries (`Dockerfile.matter-build`)
   - Various shell scripts for Matter library management

3. **Third-Party Dependencies**:
   - `third_party/connectedhomeip/` - Connected Home IP source code
   - `lib/connectedhomeip/` - Pre-built Matter libraries (via Git LFS)
   - ZAP-generated cluster definitions

4. **Documentation**:
   - Multiple Matter setup, debugging, and troubleshooting guides

## Current State

The `src/matter/mod.rs` now contains a **stub implementation** that:
- Logs warnings that Matter is not implemented
- Provides the same public API (`MatterServer`, `ThermostatAccessory`)
- Allows the rest of the application to compile and run

Thermostats will function locally but won't be accessible via HomeKit/Google Home/Alexa.

## Re-Integration Steps

### Step 1: Connected Home IP Library

You need the Matter SDK (Connected Home IP) which is a large C++ project.

**Option A: Pre-built Libraries**

Download or build the Matter library for your target platform:
- `libCHIP.a` - Main Matter library

Store in `lib/connectedhomeip/<platform>/` where platform is:
- `linux-arm64` - Raspberry Pi (64-bit)
- `linux-x64` - Linux x86_64
- `darwin-arm64` - macOS Apple Silicon
- `darwin-x64` - macOS Intel

**Option B: Build from Source**

```bash
# Clone Matter SDK
git clone --depth 1 https://github.com/project-chip/connectedhomeip.git third_party/connectedhomeip
cd third_party/connectedhomeip

# Initialize submodules (required for Pigweed, etc.)
scripts/checkout_submodules.py --shallow --platform linux

# Activate build environment
source scripts/activate.sh

# Build the library
gn gen out/host
ninja -C out/host src/lib
```

### Step 2: ZAP Configuration

ZAP (ZCL Advanced Platform) generates cluster definitions from `.zap` files.

1. Create `matter/thermostat.zap` with your device configuration
2. Run ZAP code generation to create cluster headers
3. Place generated files in `third_party/connectedhomeip/zap-generated/`

Key generated files:
- `gen_config.h` - Cluster configuration
- `endpoint_config.h` - Endpoint definitions
- `callback.h` - Callback interfaces
- `cluster-objects.h/cpp` - Cluster implementations

### Step 3: C++ Wrapper

Create `src/matter/ffi/matter_wrapper.h` with C-compatible API:

```c
// Opaque handles
typedef struct MatterDevice MatterDevice;
typedef struct MatterThermostat MatterThermostat;

// Device lifecycle
MatterDevice* matter_device_init(const MatterDeviceConfig* config);
void matter_device_shutdown(MatterDevice* device);
int matter_device_run_event_loop(MatterDevice* device);

// Commissioning
int matter_device_get_qr_code(MatterDevice* device, char* buffer, size_t size);
int matter_device_get_pairing_code(MatterDevice* device, char* buffer, size_t size);

// Thermostat
MatterThermostat* matter_thermostat_create(MatterDevice* device, const MatterThermostatConfig* config);
void matter_thermostat_destroy(MatterThermostat* thermostat);
int matter_thermostat_set_local_temperature(MatterThermostat* thermostat, int16_t temp);
int matter_thermostat_set_running_state(MatterThermostat* thermostat, uint16_t state);

// Callbacks
typedef void (*SetpointChangedCallback)(void* ctx, uint16_t endpoint, int16_t heat, int16_t cool);
typedef void (*ModeChangedCallback)(void* ctx, uint16_t endpoint, uint8_t mode);
void matter_thermostat_set_setpoint_callback(MatterThermostat* t, SetpointChangedCallback cb, void* ctx);
void matter_thermostat_set_mode_callback(MatterThermostat* t, ModeChangedCallback cb, void* ctx);
```

Create `src/matter/ffi/matter_wrapper.cpp` implementing the API using CHIP headers:

```cpp
#include "matter_wrapper.h"

#ifdef MATTER_SDK_AVAILABLE
#include <platform/CHIPDeviceLayer.h>
#include <app/server/Server.h>
// ... other CHIP includes
#endif

extern "C" {

MatterDevice* matter_device_init(const MatterDeviceConfig* config) {
#ifdef MATTER_SDK_AVAILABLE
    // Initialize CHIP stack
    chip::DeviceLayer::PlatformMgr().InitChipStack();
    // ... full implementation
#else
    return nullptr;  // Stub
#endif
}

// ... other functions

}
```

### Step 4: Rust FFI Bindings

Create `src/matter/ffi/bindings.rs`:

```rust
use std::os::raw::{c_char, c_int, c_void};

#[repr(C)]
pub struct MatterDevice { _unused: [u8; 0] }

#[repr(C)]
pub struct MatterThermostat { _unused: [u8; 0] }

extern "C" {
    pub fn matter_device_init(config: *const MatterDeviceConfig) -> *mut MatterDevice;
    pub fn matter_device_shutdown(device: *mut MatterDevice);
    // ... other bindings
}
```

Create `src/matter/ffi/wrapper.rs` with safe wrappers:

```rust
pub struct Device {
    ptr: *mut MatterDevice,
}

impl Device {
    pub fn new(config: DeviceConfig) -> Result<Self> {
        let ptr = unsafe { matter_device_init(&c_config) };
        if ptr.is_null() {
            return Err(anyhow!("Failed to initialize"));
        }
        Ok(Device { ptr })
    }
    // ... other methods
}

impl Drop for Device {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe { matter_device_shutdown(self.ptr); }
        }
    }
}
```

### Step 5: Build Script

Update `build.rs` to compile the C++ wrapper and link Matter:

```rust
fn main() {
    let chip_available = check_for_matter_library();
    
    if chip_available {
        // Compile C++ wrapper
        cc::Build::new()
            .cpp(true)
            .file("src/matter/ffi/matter_wrapper.cpp")
            .include("third_party/connectedhomeip/src")
            .define("MATTER_SDK_AVAILABLE", "1")
            .compile("matter_wrapper");
        
        // Link Matter library
        println!("cargo:rustc-link-search=native=lib/connectedhomeip/{}", platform);
        println!("cargo:rustc-link-lib=static=CHIP");
        println!("cargo:rustc-link-lib=dylib=pthread");
        println!("cargo:rustc-link-lib=dylib=stdc++");
        
        // Platform-specific (Linux)
        println!("cargo:rustc-link-lib=dylib=glib-2.0");
        println!("cargo:rustc-link-lib=dylib=gio-2.0");
        
        println!("cargo:rustc-cfg=matter_enabled");
    }
}
```

### Step 6: Update Matter Module

Replace the stub in `src/matter/mod.rs` with the full implementation:

```rust
#[cfg(matter_enabled)]
pub mod ffi;

use crate::devices::Thermostat;
// ... full implementation with MatterServer, ThermostatAccessory, callbacks, etc.
```

### Step 7: Cargo.toml

Add build dependencies:

```toml
[build-dependencies]
cc = "1.0"
# Optional: bindgen = "0.69"  # For auto-generating bindings
```

## Matter Thermostat Cluster

The thermostat implementation uses Matter cluster 0x0201 with these attributes:

| Attribute | ID | Type | Description |
|-----------|-----|------|-------------|
| LocalTemperature | 0x0000 | int16 | Current temp (centi-°C) |
| OccupiedHeatingSetpoint | 0x0012 | int16 | Heat setpoint (centi-°C) |
| SystemMode | 0x001C | enum8 | Off(0x00) or Heat(0x04) |
| ThermostatRunningState | 0x0029 | bitmap8 | Bit 0 = heating active |

Temperature conversions:
```rust
// Celsius to centi-degrees
let centi_c = (temp_c * 100.0) as i16;

// Centi-degrees to Celsius
let temp_c = (centi_c as f32) / 100.0;
```

## Commissioning

Matter devices are commissioned (paired) via:

1. **QR Code** - Generated from device credentials, scanned by controller app
2. **Manual Pairing Code** - 11-digit code entered manually

The QR code encodes:
- Vendor ID
- Product ID
- Discriminator (12-bit)
- Setup passcode

## Key Dependencies

When re-implementing, you'll need:

- Connected Home IP SDK (C++ library, ~300MB shallow clone)
- GLib 2.0 (Linux platform layer dependency)
- pkg-config (for finding GLib)
- C++17 compiler

## Testing

Test Matter integration with:

- **chip-tool** - Command-line Matter controller
- **Apple Home** - iOS/macOS HomeKit
- **Google Home** - Android/web
- **Amazon Alexa** - Echo devices

## Resources

- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [Connected Home IP GitHub](https://github.com/project-chip/connectedhomeip)
- [Matter Developer Guide](https://developers.home.google.com/matter)
- [Apple HomeKit + Matter](https://developer.apple.com/homekit/)

## Files to Create/Restore

```
src/matter/
├── mod.rs                    # Full implementation (replace stub)
└── ffi/
    ├── mod.rs                # Module definition
    ├── bindings.rs           # Raw FFI bindings
    ├── wrapper.rs            # Safe Rust wrapper
    ├── matter_wrapper.h      # C header
    └── matter_wrapper.cpp    # C++ implementation

build.rs                      # Full build script with Matter compilation

lib/connectedhomeip/
└── <platform>/
    └── libCHIP.a             # Pre-built Matter library

third_party/connectedhomeip/  # (Optional) Full SDK source
└── zap-generated/            # Generated cluster definitions
```
