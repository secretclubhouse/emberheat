# ZAP Data Integration Guide

## Overview

This document provides comprehensive instructions for generating and integrating real ZAP (ZCL Advanced Platform) data into the Thermos-Rust Matter integration.

**ZAP** is the Matter/Connected Home IP code generator that creates C++ code for:
- Device type configurations
- Cluster implementations
- Endpoint definitions
- Attribute storage structures
- Command handlers

## Current Status

The project includes **real ZAP-generated files** created from `matter/thermostat.zap`:
- `lib/connectedhomeip/*/include/zap-generated/*.h` (committed to repository)

These files are automatically generated during Docker builds (`make matter-build-docker`) and committed to the repository for easy use. The code at `src/matter/ffi/matter_wrapper.cpp` supports toggling between real ZAP data (via `USE_REAL_ZAP_DATA=1`) and stub mode (default for backward compatibility).

## Why Use Real ZAP Data?

Real ZAP-generated code provides:

1. **Proper Data Model**: Full CodegenDataModelProvider support
2. **Specification Compliance**: Ensures thermostat cluster matches Matter spec
3. **Better Interoperability**: Improved compatibility with controllers (HomeKit, Google Home, Alexa)
4. **Rich Metadata**: Proper attribute types, constraints, and command definitions
5. **Future-Proofing**: Support for new Matter SDK versions that deprecate legacy paths

## Prerequisites

### Software Requirements

1. **Node.js 20+** (for ZAP tool)
   ```bash
   # Already included in devcontainer
   nvm install 20
   nvm use 20
   ```

2. **Java Runtime** (for ZAP backend)
   ```bash
   # Already included in devcontainer
   sudo apt-get install default-jre
   ```

3. **Matter SDK** (ZAP tool is included)
   ```bash
   # Download Matter source if not already present
   make matter-download
   ```

4. **Python 3** (for codegen scripts)
   ```bash
   # Already available in most environments
   python3 --version
   ```

## Quick Start (Using Docker Build)

**Recommended approach**: The Docker build automatically generates ZAP data from `matter/thermostat.zap`:

```bash
# Build Matter libraries with automatic ZAP generation
make matter-build-docker

# The ZAP-generated files are now in lib/connectedhomeip/<platform>/include/zap-generated/
# These files are committed to the repository for convenience
```

The Docker build process:
1. Reads `matter/thermostat.zap` configuration
2. Runs ZAP code generation
3. Copies generated headers to the appropriate platform directory
4. Files are ready to commit to the repository

## Manual Generation (For Customization)

If you need to modify the ZAP configuration or regenerate files locally:

### Step 1: Create/Edit ZAP Configuration File

Edit the `.zap` file at `matter/thermostat.zap` to customize your thermostat device.

**Key sections to configure:**

1. **Device Type**: Thermostat (device type 0x0301)
2. **Endpoints**:
   - Endpoint 0: Root node (standard for all Matter devices)
   - Endpoint 1+: Thermostat endpoints (one per zone)
3. **Clusters**:
   - Basic Information (0x0028)
   - Thermostat (0x0201)
   - Optional: Diagnostics, OTA, etc.
4. **Attributes**: Temperature, setpoints, modes, running state

**Example ZAP file structure:**
```json
{
  "writeTime": "2025-01-17",
  "featureLevel": 103,
  "creator": "zap",
  "endpointTypes": [
    {
      "id": 1,
      "name": "Thermostat",
      "deviceTypeRef": {
        "code": 769,  // 0x0301 = Thermostat
        "profileId": 259,  // Matter profile
        "label": "MA-thermostat"
      },
      "clusters": [
        {
          "name": "Thermostat",
          "code": 513,  // 0x0201
          "side": "server",
          "enabled": 1,
          "attributes": [
            {
              "name": "LocalTemperature",
              "code": 0,
              "side": "server",
              "type": "int16s",
              "included": 1,
              "storageOption": "RAM"
            },
            // ... more attributes
          ]
        }
      ]
    }
  ],
  "endpoints": [
    {
      "endpointTypeName": "Thermostat",
      "endpointTypeIndex": 1,
      "profileId": 259,
      "endpointId": 1,
      "networkId": 0
    }
  ]
}
```

### Step 2: Generate ZAP Code

#### Option A: Using the ZAP GUI (Recommended for initial setup)

```bash
# Navigate to Matter SDK
cd third_party/connectedhomeip

# Launch ZAP GUI
./scripts/tools/zap/run_zaptool.sh matter/thermostat.zap
```

In the ZAP GUI:
1. Review/edit your configuration
2. File → Generate to create C++ code
3. Output will be in `zzz_generated/` directory

#### Option B: Using ZAP CLI (For automation)

```bash
cd third_party/connectedhomeip

# Run ZAP code generation
./scripts/tools/zap/generate.py \
  -z ../../matter/thermostat.zap \
  -o zzz_generated/thermostat \
  -t examples/thermostat/thermostat-common
```

**Parameters:**
- `-z`: Input .zap file
- `-o`: Output directory
- `-t`: Template directory (use thermostat example templates)

### Step 3: Copy and Commit Generated Files

After generation, copy the ZAP-generated files to your project:

```bash
# From thermos-rust root directory
PLATFORM=$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)

# Create destination directory
mkdir -p lib/connectedhomeip/$PLATFORM/include/zap-generated

# Copy generated files
cp third_party/connectedhomeip/zzz_generated/thermostat/zap-generated/*.h \
   lib/connectedhomeip/$PLATFORM/include/zap-generated/

# Commit the generated files (they're tracked in git)
git add lib/connectedhomeip/$PLATFORM/include/zap-generated/
git commit -m "Update ZAP-generated files for $PLATFORM"
```

**Note**: The generated files are **committed to the repository** (unlike typical generated code) because:
- Allows builds without Node.js/Java dependencies
- Ensures consistent builds across environments
- Same philosophy as committing pre-built Matter libraries
- Header files are small (~100KB total)

### Step 4: Enable Real ZAP Data in Build

Set the environment variable to enable real ZAP data:

```bash
# Enable real ZAP data for builds
export USE_REAL_ZAP_DATA=1

# Build the project
cargo build --release
```

Or add to your `.env` file:
```bash
USE_REAL_ZAP_DATA=1
```

### Step 5: Update Matter Wrapper Code

When `USE_REAL_ZAP_DATA=1`, the build will:

1. Define `USE_REAL_ZAP_DATA` preprocessor macro
2. Include real ZAP-generated headers
3. Use `CodegenDataModelProvider` instead of nullptr
4. Link against ZAP-generated attribute storage

The wrapper code (`src/matter/ffi/matter_wrapper.cpp`) automatically handles this via conditional compilation:

```cpp
#ifdef USE_REAL_ZAP_DATA
    // Use real ZAP-generated data model
    #include <zap-generated/gen_config.h>
    #include <zap-generated/endpoint_config.h>
    #include <app/codegen-data-model/CodegenDataModelProvider.h>

    static chip::app::CodegenDataModelProvider sDataModelProvider;
    initParams.dataModelProvider = &sDataModelProvider;
#else
    // Use legacy ember path (stub mode)
    initParams.dataModelProvider = nullptr;
#endif
```

## Updating ZAP Configuration

### Adding a New Zone/Endpoint

1. Edit `matter/thermostat.zap` in ZAP GUI
2. Add new endpoint with same thermostat endpoint type
3. Increment endpoint ID (1, 2, 3, etc.)
4. Regenerate code (Step 2)
5. Copy files (Step 3)
6. Rebuild (Step 4)

### Modifying Attributes

1. Open `matter/thermostat.zap` in ZAP GUI
2. Navigate to endpoint → Thermostat cluster
3. Enable/disable attributes as needed
4. Modify attribute properties (storage, defaults, bounds)
5. Save and regenerate

### Adding Custom Clusters

```bash
# Example: Add Fan Control cluster (0x0202)
# 1. Open ZAP GUI
# 2. Select endpoint
# 3. Add cluster from list
# 4. Configure attributes and commands
# 5. Regenerate
```

## Build System Integration

### Makefile Target

Add a convenience target to regenerate ZAP data:

```bash
make zap-generate
```

This target:
1. Checks for Matter SDK
2. Runs ZAP code generation
3. Copies files to correct platform directory
4. Shows summary of generated files

### Automated Generation in Docker

For Docker builds, ZAP generation can be integrated into the Matter build:

```bash
# In scripts/docker-build-matter-internal.sh
# After building Matter libraries, generate ZAP files
if [ -f "$REPO_ROOT/matter/thermostat.zap" ]; then
    echo "Generating ZAP code..."
    cd "$MATTER_DIR"
    ./scripts/tools/zap/generate.py \
        -z "$REPO_ROOT/matter/thermostat.zap" \
        -o zzz_generated/thermostat \
        -t examples/thermostat/thermostat-common
fi
```

## Testing

### Verify ZAP Files

```bash
# Check that required files exist
ls -l lib/connectedhomeip/*/include/zap-generated/

# Expected files:
# - gen_config.h
# - endpoint_config.h
# - attribute-size.h
# - cluster-objects.h
# - callback.h
```

### Build Tests

```bash
# Clean build with real ZAP data
export USE_REAL_ZAP_DATA=1
cargo clean
cargo build --release

# Should see in build output:
# "INFO: Using real ZAP-generated data model"
```

### Runtime Tests

```bash
# Run thermostat with real ZAP data
export USE_REAL_ZAP_DATA=1
cargo run --release

# Check logs for:
# - "Matter device initialized with CodegenDataModelProvider"
# - Proper endpoint/cluster registration
# - No attribute storage errors
```

### Verify with Matter Controller

1. Commission device to HomeKit/Google Home
2. Verify all attributes are readable
3. Test setpoint changes
4. Check mode switching (off/heat)
5. Verify running state updates

## Troubleshooting

### ZAP Generation Fails

**Error**: `zap-cli not found`
```bash
# Solution: Ensure Node.js 20+ is installed
nvm install 20
nvm use 20
cd third_party/connectedhomeip
npm install
```

**Error**: `Java not found`
```bash
# Solution: Install Java runtime
sudo apt-get install default-jre
```

### Build Errors with Real ZAP Data

**Error**: `gen_config.h: No such file or directory`
```bash
# Solution: Copy ZAP files to correct platform directory
PLATFORM=$(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)
cp -r third_party/connectedhomeip/zzz_generated/thermostat/zap-generated/* \
      lib/connectedhomeip/$PLATFORM/include/zap-generated/
```

**Error**: `undefined reference to emberAf...`
```bash
# Solution: Ensure Matter libraries were built with same ZAP config
# Rebuild Matter libraries:
make matter-clean
make matter-setup-docker
```

### Runtime Errors

**Error**: `CodegenDataModelProvider: unknown attribute`
- Check that ZAP file includes all required thermostat attributes
- Verify LocalTemperature (0x0000), OccupiedHeatingSetpoint (0x0012), SystemMode (0x001C)
- Regenerate with complete thermostat cluster

**Error**: `Endpoint registration failed`
- Ensure `FIXED_ENDPOINT_COUNT` in gen_config.h matches your endpoints
- Check endpoint IDs don't conflict
- Verify endpoint 0 (root node) is always present

## Platform-Specific Notes

### Linux (Raspberry Pi)

ZAP files are platform-independent. Use the same generated files across platforms:
```bash
# Generate once, copy to all platforms
for platform in linux-arm64 linux-x64 darwin-arm64; do
    cp -r zzz_generated/thermostat/zap-generated/* \
          lib/connectedhomeip/$platform/include/zap-generated/
done
```

### macOS (Development)

Same ZAP files work on macOS. Only the compiled libraries are platform-specific.

### Docker Builds

ZAP generation can run in the Docker build:
```dockerfile
# Install Node.js for ZAP
RUN curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.39.0/install.sh | bash
RUN nvm install 20

# Generate ZAP during build
RUN cd /matter && ./scripts/tools/zap/generate.py ...
```

## Best Practices

1. **Version Control**: Commit both the `.zap` file AND the generated headers
   - `.zap` file is the source of truth
   - Generated headers in `lib/connectedhomeip/*/include/zap-generated/` ARE committed (exception to typical practice)
   - This allows builds without ZAP tooling installed
   - Regenerate and recommit when `.zap` file changes

2. **Regenerate After SDK Updates**: When updating Matter SDK version:
   ```bash
   # Pull new SDK
   make matter-download
   # Regenerate with new templates
   make zap-generate
   # Rebuild libraries
   make matter-setup-docker
   ```

3. **Document Custom Attributes**: If adding non-standard attributes, document in ARCHITECTURE.md

4. **Test Both Modes**: Regularly test with both stub and real ZAP data to ensure fallback works

5. **CI/CD Integration**: Generate ZAP files in CI pipeline:
   ```yaml
   # .github/workflows/build.yml
   - name: Generate ZAP data
     run: |
       export USE_REAL_ZAP_DATA=1
       make zap-generate
   ```

## Reference Files

- **Sample ZAP Config**: `matter/thermostat.zap`
- **Matter Wrapper**: `src/matter/ffi/matter_wrapper.cpp`
- **Build Script**: `build.rs`
- **Docker Build**: `scripts/docker-build-matter-internal.sh`

## Further Reading

- [Matter Specification](https://csa-iot.org/developer-resource/specifications-download-request/)
- [ZAP Tool Documentation](https://github.com/project-chip/zap)
- [Matter Device Type Library](https://github.com/project-chip/connectedhomeip/tree/master/src/app/zap-templates)
- [Thermostat Cluster Spec](https://github.com/project-chip/connectedhomeip/blob/master/src/app/zap-templates/zcl/data-model/chip/thermostat-cluster.xml)

## Support

For issues with ZAP integration:
1. Check logs in `third_party/connectedhomeip/zzz_generated/`
2. Review generated files for errors
3. Compare with Matter example thermostats
4. Open GitHub issue with ZAP file and error details
