# Matter QR Code Fix - Complete Summary

## Problem

The Matter QR code was generating an invalid format:
```
MT:Y.KFFF180010F0001344225
```

This is **raw hexadecimal**, not the proper **Base38-encoded** format required by Matter controllers. The QR code would fail to commission because it uses a stub implementation instead of the real Matter SDK.

**Root Cause**: Build system was using a stub implementation because it couldn't find the Matter SDK, even though pre-built libraries existed.

## Solution Overview

The fix involved three main components:

### 1. Update Build System ([build.rs](../build.rs))

**Changes**:
- Check for pre-built libraries FIRST (in `lib/connectedhomeip/<platform>/`)
- Require both headers AND libraries for full Matter support
- Add proper include paths for all header directories

**Key Logic**:
```rust
let has_prebuilt = prebuilt_lib_dir.exists() && has_required_libraries(&prebuilt_lib_dir);
let has_submodule = chip_root.exists() && chip_root.join("src").exists();
let chip_available = has_submodule && (has_prebuilt || chip_root.join("out/host/lib").exists());
```

### 2. Update Docker Build Script ([scripts/docker-build-matter-internal.sh](../scripts/docker-build-matter-internal.sh))

**Changes**:
- Copy all source headers (`.h` files from `src/`)
- Copy third-party headers (nlassert, nlio)
- Generate ASN1OID.h using Python script
- Build minimal example app to generate ZAP files
- Search multiple locations for ZAP-generated files
- Create minimal ZAP stub files as fallback
- Verify critical files exist

**What Gets Copied**:

1. **Source Headers**: `third_party/connectedhomeip/src/**/*.h`
2. **Third-Party Headers**: nlassert, nlio with `.h` and `.hpp` files
3. **Build Config**: `third_party/connectedhomeip/config/`
4. **Generated Headers**:
   - `src/lib/asn1/ASN1OID.h` (generated via Python script)
   - `zap-generated/gen_config.h` (from build or stub)
   - `zap-generated/endpoint_config.h` (from build or stub)
5. **ZZZ Generated**: `zzz_generated/app-common/` (cluster definitions)

### 3. Update Orchestration Script ([scripts/docker-build-matter.sh](../scripts/docker-build-matter.sh))

**Changes**:
- Updated success message to confirm headers were copied
- Updated commit instructions to include both libraries and headers

## How It Works

### Docker Build Process

```bash
make matter-build-docker
```

**Inside the Docker container**:

1. **Download Matter source** (v1.4.2.0)
   ```bash
   git clone --depth=1 --branch v1.4.2.0 connectedhomeip
   ```

2. **Initialize submodules**
   ```bash
   python3 scripts/checkout_submodules.py --shallow --platform linux
   ```

3. **Bootstrap build environment**
   ```bash
   source scripts/bootstrap.sh
   ```

4. **Configure build with GN**
   ```bash
   gn gen out/unified --args="target_cpu=\"arm64\" ..."
   ```

5. **Build libraries**
   ```bash
   ninja -C out/unified src/app/server:server
   ninja -C out/unified src/controller:controller
   ninja -C out/unified src/setup_payload:setup_payload
   ninja -C out/unified src/lib:lib
   ```

6. **Build minimal example** (generates ZAP files)
   ```bash
   ninja -C out/unified examples/all-clusters-minimal-app/linux
   ```

7. **Copy libraries** to `lib/connectedhomeip/<platform>/`

8. **Copy headers** to `third_party/connectedhomeip/`
   - Source headers
   - Third-party headers
   - Generated headers (ASN1OID.h)
   - ZAP-generated files
   - Build configuration

### Thermos-Rust Build Process

```bash
cargo build --release
```

**Build system (build.rs)**:

1. **Detect pre-built libraries** in `lib/connectedhomeip/linux-arm64/`
2. **Detect headers** in `third_party/connectedhomeip/`
3. **Compile Matter wrapper** (`src/matter/ffi/matter_wrapper.cpp`)
   - Uses headers from `third_party/connectedhomeip/`
   - Includes: `<setup_payload/QRCodeSetupPayloadGenerator.h>`
   - Defines: `MATTER_SDK_AVAILABLE`
4. **Link against libraries** from `lib/connectedhomeip/linux-arm64/`
   - Links: `libCHIP.a`

**Result**: Proper Matter SDK QR code generation

## Expected QR Code Format

### Before Fix (Stub)
```
MT:Y.KFFF180010F0001344225
```
- Raw hex concatenation
- Cannot be commissioned
- Not Base38-encoded

### After Fix (Real SDK)
```
MT:Y.K9042C00KA0648G00
```
- Proper Base38 encoding
- Includes all required commissioning data
- Compatible with Matter controllers (HomeKit, Google, Amazon, etc.)

## ZAP-Generated Files

### What are ZAP files?

**ZAP** (ZCL Advanced Platform) is Matter's code generator for:
- Cluster configurations
- Endpoint definitions
- Attribute storage
- Command handling

### Critical Files

1. **gen_config.h**
   - Defines enabled clusters
   - Feature flags
   - Endpoint count

2. **endpoint_config.h**
   - Endpoint definitions
   - Endpoint types
   - Attribute storage

These files **must match** the compiled libraries or you'll get:
- Linking errors
- Undefined references
- Runtime crashes

### Fallback Strategy

If ZAP files aren't found during Docker build, the script creates minimal stubs:

**gen_config.h**:
```c
#define EMBER_AF_GENERATE_CLI 0
#define EMBER_AF_SUPPORT_COMMAND_DISCOVERY 1
#define ZCL_USING_THERMOSTAT_CLUSTER_SERVER 1
#define FIXED_ENDPOINT_COUNT 1
```

**endpoint_config.h**:
```c
#define FIXED_ENDPOINT_COUNT 1
#define GENERATED_ENDPOINT_TYPES { { 0, 0, 0, 0, 0, 0, 0 } }
#define FIXED_ENDPOINT_ARRAY { { 0x0000, 0, 0 } }
```

These allow compilation but may not match all library features.

## Testing the Fix

### 1. Clean Build

```bash
# Remove existing artifacts
rm -rf lib/connectedhomeip/ third_party/connectedhomeip/

# Run Docker build
make matter-build-docker
```

**Expected output**:
```
✓ Build completed successfully!

Platform: linux-arm64
Libraries: 2 files (132M)
Libraries location: lib/connectedhomeip/linux-arm64
Headers copied: Yes
Headers location: third_party/connectedhomeip
```

### 2. Verify Headers

```bash
# Check critical files exist
ls third_party/connectedhomeip/src/setup_payload/QRCodeSetupPayloadGenerator.h
ls third_party/connectedhomeip/src/lib/asn1/ASN1OID.h
ls third_party/connectedhomeip/zap-generated/gen_config.h
ls third_party/connectedhomeip/zap-generated/endpoint_config.h
```

All should exist.

### 3. Build Thermos-Rust

```bash
cargo build --release
```

**Expected output**:
```
INFO: Using pre-built Matter libraries from lib/connectedhomeip/linux-arm64
INFO: Using Matter headers from third_party/connectedhomeip
   Compiling thermos-rust v0.1.0 (/workspace)
    Finished release [optimized] target(s) in X.XXs
```

No header errors!

### 4. Verify QR Code

```bash
# Run the application
cargo run --release

# Check logs for QR code
# Should see Base38-encoded format like:
# QR Code: MT:Y.K9042C00KA0648G00
```

### 5. Test Commissioning

1. Open Matter-compatible app (HomeKit, Google Home, etc.)
2. Scan the QR code
3. Device should commission successfully
4. Thermostats should appear as accessories

## Troubleshooting

### Headers not copied

**Symptom**:
```
WARNING: Pre-built Matter libraries found but headers are missing.
```

**Solution**:
```bash
make matter-build-docker
```

### Missing ZAP files

**Symptom**:
```
fatal error: zap-generated/gen_config.h: No such file or directory
```

**Check**:
```bash
ls third_party/connectedhomeip/zap-generated/
```

**Solution**: The script should create minimal stubs. If not:
```bash
# Rebuild with verbose output
docker logs -f <container-id>
```

### Wrong QR code format

**Symptom**: Still seeing `MT:Y.K<hex>` format

**Check build type**:
```bash
cargo build --release 2>&1 | grep "MATTER_BUILD_TYPE"
```

Should show `full`, not `stub`.

**Solution**:
```bash
# Clean rebuild
cargo clean
cargo build --release
```

## Files Modified

1. ✅ `build.rs` - Build system logic
2. ✅ `scripts/docker-build-matter-internal.sh` - Container build with headers
3. ✅ `scripts/docker-build-matter.sh` - Build orchestration
4. ✅ `docs/matter-docker-build-headers.md` - Header documentation
5. ✅ `docs/matter-qr-code-fix-summary.md` - This file

## Next Steps

After running `make matter-build-docker`:

1. **Commit the artifacts**:
   ```bash
   git add lib/connectedhomeip/linux-arm64/
   git add third_party/connectedhomeip/
   git commit -m "Add Matter libraries and headers for linux-arm64"
   ```

2. **Test on target device** (Raspberry Pi):
   ```bash
   # Deploy to Pi
   scp -r . pi@raspberrypi:/opt/thermos-rust/

   # Build on Pi
   ssh pi@raspberrypi
   cd /opt/thermos-rust
   cargo build --release
   ```

3. **Verify QR code** generates properly

4. **Commission with Matter controller**

## References

- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [Connected Home IP GitHub](https://github.com/project-chip/connectedhomeip)
- [Base38 Encoding](https://github.com/project-chip/connectedhomeip/blob/master/docs/guides/setup_payload.md)
- [ZAP Tool Documentation](https://github.com/project-chip/zap)
