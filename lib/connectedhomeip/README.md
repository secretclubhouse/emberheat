# Pre-built Connected Home IP Libraries

This directory contains pre-built Matter (Connected Home IP) libraries to avoid
rebuilding the entire library on every clone.

## Directory Structure

```
lib/connectedhomeip/
├── darwin-arm64/    # macOS ARM64 (Apple Silicon)
├── darwin-x64/      # macOS x86_64 (Intel)
├── linux-arm64/     # Linux ARM64 (Raspberry Pi, etc.)
└── linux-x64/       # Linux x86_64
```

## Usage

The build system (`build.rs`) automatically detects and uses pre-built libraries
from this directory based on your platform.

**Priority order:**
1. Pre-built libraries in `lib/connectedhomeip/{platform}/`
2. Submodule build at `third_party/connectedhomeip/out/host/lib/`
3. Stub implementation (no Matter support)

## Building Libraries

To build and store libraries for your platform:

```bash
# Build the Connected Home IP library
cd third_party/connectedhomeip
source scripts/activate.sh

# For macOS ARM64
gn gen out/host --args='target_os="darwin" target_cpu="arm64"'

# For macOS x64
# gn gen out/host --args='target_os="darwin" target_cpu="x64"'

# For Linux ARM64 (Raspberry Pi)
# gn gen out/host --args='target_os="linux" target_cpu="arm64"'

# For Linux x64
# gn gen out/host --args='target_os="linux" target_cpu="x64"'

ninja -C out/host
cd ../..

# Copy libraries to the appropriate directory
./scripts/copy-matter-libs.sh
```

## Upgrading Libraries

When upgrading the Connected Home IP submodule:

```bash
# Update submodule
cd third_party/connectedhomeip
git checkout <new-version>
cd ../..

# Rebuild and copy libraries for each platform you support
./scripts/copy-matter-libs.sh

# Commit the updated libraries
git add lib/connectedhomeip/
git commit -m "Update Matter libraries to <version>"
```

## Platform Detection

The build script automatically detects your platform:
- **OS:** `darwin` or `linux`
- **Architecture:** `arm64` (aarch64) or `x64` (x86_64)

## Library Files

Each platform directory should contain the required static libraries:
- `libCHIPAppServerLib.a`
- `libchip.a`
- `libCHIPController.a`
- `libCHIPSetupPayload.a`
- And other dependencies as needed

## Size Considerations

The pre-built libraries are typically:
- Per platform: ~50-150 MB
- Total (all platforms): ~200-600 MB

Consider using Git LFS if repository size becomes an issue.
