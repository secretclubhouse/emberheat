# Matter Build Fixes - GLib and Third-Party Dependencies

## Latest Changes

### Problem: Missing System Headers (gio/gio.h)

**Error**:
```
fatal error: gio/gio.h: No such file or directory
  from third_party/connectedhomeip/src/platform/GLibTypeDeleter.h:22
```

**Root Cause**: Matter's Linux platform code requires GLib development headers for BlueZ/Bluetooth support. The headers are installed on the system but weren't in the compiler's include path.

**Solution**: Updated [`build.rs`](../build.rs) to add GLib include paths dynamically:

1. Added `get_glib_includes()` function that:
   - Uses `pkg-config --cflags glib-2.0 gio-2.0` to get include paths
   - Falls back to common paths if pkg-config fails
   - Detects architecture (aarch64/x86_64/arm) for arch-specific paths

2. Modified the build configuration to include GLib paths when `MATTER_SDK_AVAILABLE` is defined

**Changes in build.rs**:
- Line 12: Added `use std::process::Command;`
- Lines 29-66: Added `get_glib_includes()` helper function
- Lines 160-164: Call `get_glib_includes()` and add paths to build

### Problem: Missing Third-Party Library Headers (inipp/inipp.h)

**Error**:
```
fatal error: inipp/inipp.h: No such file or directory
  from third_party/connectedhomeip/src/platform/Linux/CHIPLinuxStorageIni.h:28
```

**Root Cause**: The Docker build script only copied two third-party libraries (`nlassert` and `nlio`) but Matter also requires `inipp` for INI file parsing.

**Solution**: Updated both Docker build script and build.rs to include `inipp`:

**Changes in docker-build-matter-internal.sh**:
- Line 189: Added `third_party/inipp` to the list of third-party directories to copy

**Changes in build.rs**:
- Line 159: Added `.include(chip_root.join("third_party/inipp/repo"))`

## Next Steps

### 1. Rebuild Matter Libraries with Docker

You need to rerun the Docker build to get the missing `inipp` headers:

```bash
make matter-build-docker
```

This will:
- Build Matter SDK from source
- Copy all necessary libraries to `lib/connectedhomeip/linux-arm64/`
- Copy all necessary headers to `third_party/connectedhomeip/`, including:
  - Source headers (`.h` and `.hpp` from `src/`)
  - Third-party headers (`nlassert`, `nlio`, **inipp**)
  - Generated headers (`ASN1OID.h`, ZAP files)
  - Build configuration

### 2. Build Thermos-Rust

After the Docker build completes, try building thermos-rust again:

```bash
cargo build --release
```

The build system will now:
1. Detect pre-built Matter libraries
2. Find all necessary headers (including GLib system headers)
3. Compile the Matter C++ wrapper with proper include paths
4. Link against the pre-built libraries

## Files Modified

### [`build.rs`](../build.rs)
- Added `std::process::Command` import
- Added `get_glib_includes()` function to dynamically get GLib paths
- Added GLib include paths when building with Matter SDK
- Added `third_party/inipp/repo` to include paths

### [`scripts/docker-build-matter-internal.sh`](../scripts/docker-build-matter-internal.sh)
- Added `third_party/inipp` to the list of third-party headers to copy

## Technical Details

### Why GLib is Required

The Matter SDK's Linux platform layer uses GLib for:
- **D-Bus communication** (via `gio/gio.h`)
- **BlueZ integration** for Bluetooth support
- **Platform utilities** (timers, main loop, etc.)

These are pulled in transitively when including `<app/server/Server.h>`:

```
matter_wrapper.cpp
  → app/server/Server.h
    → app/CASESessionManager.h
      → platform/CHIPDeviceLayer.h
        → platform/ConnectivityManager.h
          → platform/GLibTypeDeleter.h
            → gio/gio.h  (GLib system header)
```

### Why inipp is Required

The Matter SDK uses the `inipp` library for parsing INI configuration files on Linux:

```
(Platform-specific code)
  → platform/Linux/CHIPLinuxStorageIni.h
    → inipp/inipp.h
```

### GLib Include Paths

GLib headers are split across multiple directories:
- `/usr/include/glib-2.0/` - Main GLib headers
- `/usr/lib/<arch>-linux-gnu/glib-2.0/include/` - Architecture-specific generated headers

The `pkg-config` tool provides the complete list of include paths needed.

### Third-Party Dependencies Structure

Matter third-party libraries are organized as:
```
third_party/<library>/
  repo/
    include/  (or root for headers)
    src/
```

The Docker script copies these to:
```
third_party/connectedhomeip/third_party/<library>/
  repo/
    (headers only)
```

## Expected Outcome

After rerunning the Docker build and cargo build, you should see:

```
INFO: Using pre-built Matter libraries from lib/connectedhomeip/linux-arm64
INFO: Using Matter headers from third_party/connectedhomeip
   Compiling thermos-rust v0.1.0 (/workspace)
    Finished release [optimized] target(s) in X.XXs
```

With no header-related errors!

## Troubleshooting

### If you still see GLib errors

Check that pkg-config can find GLib:
```bash
pkg-config --cflags glib-2.0 gio-2.0
```

Should output include paths. If not, install GLib development packages:
```bash
sudo apt-get install libglib2.0-dev
```

### If you still see inipp errors

Verify the Docker build copied the inipp headers:
```bash
ls third_party/connectedhomeip/third_party/inipp/
```

Should show the `repo` directory with `inipp.h` inside.

If not, check the Docker build log for errors during the third-party header copying phase.

### If there are other missing third-party libraries

The Docker script can be extended to copy additional libraries. Common candidates:
- `third_party/pigweed` - Google's embedded libraries
- `third_party/mbedtls` - Cryptographic library (may be needed)

Add them to the loop in `docker-build-matter-internal.sh` line 189.

## Progress Summary

✅ **Fixed**: GLib system headers (gio/gio.h, glib.h)
✅ **Fixed**: inipp third-party library headers
⏳ **Next**: Rerun Docker build to get inipp headers
⏳ **Next**: Test full cargo build to find any remaining missing dependencies

## See Also

- [QR Code Fix Summary](matter-qr-code-fix-summary.md) - Overall fix strategy
- [Docker Build Headers](matter-docker-build-headers.md) - Header copying documentation
- [Matter SDK Build Guide](https://github.com/project-chip/connectedhomeip/blob/master/docs/guides/BUILDING.md)
