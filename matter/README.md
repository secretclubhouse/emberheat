# Matter Configuration Files

This directory contains Matter (Connected Home IP) configuration files for the Thermos-Rust project.

## Files

### thermostat.zap

ZAP (ZCL Advanced Platform) configuration file that defines the Matter device data model for the multi-zone thermostat system.

**What it defines:**
- Device types (Root Node + Thermostat)
- Endpoints (Endpoint 0: root, Endpoint 1: thermostat)
- Clusters:
  - Descriptor (0x001D)
  - Basic Information (0x0028)
  - Identify (0x0003)
  - Thermostat (0x0201)
- Attributes for each cluster
- Commands supported

**Current configuration:**
- 1 root device endpoint (endpoint 0)
- 1 thermostat endpoint (endpoint 1)
- Heating-only mode (ControlSequenceOfOperation = 0x04)
- Supports temperature reading, setpoint control, and system mode

## Using ZAP Configuration

### Automatic Generation (Recommended)

ZAP data is **automatically generated** during the Docker Matter build:

```bash
# Build Matter libraries - automatically generates ZAP data
make matter-build-docker
```

The Docker build process:
1. Reads `matter/thermostat.zap` configuration
2. Runs ZAP code generation inside the container
3. Copies generated headers to `third_party/connectedhomeip/zap-generated/`
4. Files are ready to use and commit

Generated files include:
- `gen_config.h` - Cluster enables and feature flags
- `endpoint_config.h` - Endpoint definitions and attribute storage
- `access.h` - Access control definitions
- `IMClusterCommandHandler.cpp` - Cluster command handlers
- Plus standard cluster files from `zzz_generated/app-common`

**Note**: The generated ZAP headers are **committed to the repository** in `third_party/connectedhomeip/zap-generated/` to enable builds without requiring ZAP tooling.

### View/Edit Configuration in GUI

To view or edit the ZAP configuration graphically:

```bash
# Ensure Matter SDK is downloaded
make matter-download

# Launch ZAP GUI
cd third_party/connectedhomeip
./scripts/tools/zap/run_zaptool.sh ../../matter/thermostat.zap
```

### Build with Real ZAP Data

After generating ZAP files (either via Docker or manually):

```bash
# Enable real ZAP data
export USE_REAL_ZAP_DATA=1

# Build
cargo build --release
```

**Note**: The generated ZAP headers are **committed to the repository** to enable builds without requiring Node.js/Java ZAP tooling.

## Customizing for Your Setup

### Adding More Zones/Thermostats

To support multiple thermostat zones:

1. Open `thermostat.zap` in the ZAP GUI
2. Add new endpoints (endpoint 2, 3, etc.)
3. Assign the "MA-thermostat" endpoint type to each
4. Save the file
5. Regenerate code: `make matter-build-docker`
6. Rebuild: `cargo build --release`

Example for 4 zones:
- Endpoint 0: Root device
- Endpoint 1: Zone 1 thermostat
- Endpoint 2: Zone 2 thermostat
- Endpoint 3: Zone 3 thermostat
- Endpoint 4: Zone 4 thermostat

### Enabling Additional Features

The thermostat cluster supports optional features. To enable them:

1. Open `thermostat.zap` in ZAP GUI
2. Navigate to the Thermostat cluster
3. Enable desired attributes:
   - **Cooling**: OccupiedCoolingSetpoint
   - **Auto mode**: Add AUTO to SystemMode enum
   - **Scheduling**: Add schedule-related attributes
   - **Setback**: Add occupied/unoccupied setpoints
4. Update `FeatureMap` attribute accordingly
5. Regenerate code

### Supported Thermostat Features

Current configuration supports:

- ✅ Heating mode
- ✅ Temperature measurement
- ✅ Occupied heating setpoint
- ✅ System mode (off/heat)
- ✅ Running state indication
- ❌ Cooling mode (not configured)
- ❌ Auto mode (not configured)
- ❌ Scheduling (not configured)
- ❌ Setback/Hold (not configured)

## Specification Compliance

The ZAP configuration follows the Matter 1.0 specification:

- **Device Type**: Thermostat (0x0301)
- **Cluster**: Thermostat (0x0201)
- **Cluster Revision**: 6 (Matter 1.0)

Required attributes are enabled. Optional attributes can be added as needed.

## Troubleshooting

### ZAP Generation Fails

**Problem**: `generate.py` fails with errors

**Solution**:
```bash
# Ensure Node.js 20+ is installed
nvm install 20
nvm use 20

# Install ZAP dependencies
cd third_party/connectedhomeip
npm install
```

### Generated Files Missing

**Problem**: Generated files not found after Docker build

**Solution**:
```bash
# Check that generation succeeded
ls -la third_party/connectedhomeip/zap-generated/

# If missing, re-run the Docker build
make matter-build-docker
```

### Build Fails with ZAP Data

**Problem**: Compilation errors when `USE_REAL_ZAP_DATA=1`

**Solution**:
1. Ensure ZAP files match the Matter SDK version
2. Regenerate ZAP files after updating the SDK
3. Check that all required attributes are present
4. Verify Matter libraries were built with compatible configuration

## Version Compatibility

The ZAP configuration is compatible with:

- Matter SDK v1.3.0.0+
- ZAP tool bundled with Matter SDK
- Matter 1.0 specification

When updating the Matter SDK version, regenerate ZAP files to ensure compatibility.

## Resources

- [ZAP Tool Documentation](https://github.com/project-chip/zap)
- [Matter Thermostat Cluster Spec](https://github.com/project-chip/connectedhomeip/blob/master/src/app/zap-templates/zcl/data-model/chip/thermostat-cluster.xml)
- [Matter Device Type Library](https://github.com/project-chip/connectedhomeip/tree/master/src/app/zap-templates)
- [Thermos-Rust ZAP Integration Guide](../docs/ZAP_INTEGRATION.md)

## See Also

- [ZAP_INTEGRATION.md](../docs/ZAP_INTEGRATION.md) - Complete integration guide
- [ARCHITECTURE.md](../ARCHITECTURE.md) - System architecture
- [matter_wrapper.cpp](../src/matter/ffi/matter_wrapper.cpp) - Matter FFI implementation
- [build.rs](../build.rs) - Build system integration
