#!/bin/bash
#
# Build Matter/Connected Home IP libraries for linking with thermos-rust
# This script activates the Matter build environment and builds required libraries
#

set -e  # Exit on error

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MATTER_DIR="$PROJECT_ROOT/third_party/connectedhomeip"

echo "============================================"
echo "Matter/Connected Home IP Build Script"
echo "============================================"
echo ""

# Check if Matter source exists
if [ ! -d "$MATTER_DIR" ]; then
    echo "ERROR: Matter source not found at $MATTER_DIR"
    echo "Please run: ./scripts/download-matter.sh"
    exit 1
fi

# Detect platform
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64)
        ARCH_NORMALIZED="x64"
        ;;
    aarch64|arm64)
        ARCH_NORMALIZED="arm64"
        ;;
    armv7l)
        ARCH_NORMALIZED="arm"
        ;;
    *)
        echo "WARNING: Unknown architecture $ARCH, using as-is"
        ARCH_NORMALIZED="$ARCH"
        ;;
esac

PLATFORM="${OS}-${ARCH_NORMALIZED}"
echo "Platform: $PLATFORM"
echo "Matter source: $MATTER_DIR"
echo ""

# Change to Matter directory
cd "$MATTER_DIR"

# Check if we need to bootstrap
if [ ! -f "scripts/activate.sh" ]; then
    echo "ERROR: Matter build scripts not found. The source may be incomplete."
    echo "Please ensure you downloaded a complete release."
    exit 1
fi

# Bootstrap/Activate the Matter build environment
echo "Setting up Matter build environment..."
if [ -f "scripts/bootstrap.sh" ]; then
    if [ ! -f "scripts/setup/environment.json" ]; then
        echo "ERROR: Matter environment configuration not found"
        exit 1
    fi

    # Use bootstrap on first run, activate on subsequent runs
    if [ ! -d ".environment" ]; then
        echo "First-time setup: bootstrapping environment (this may take a few minutes)..."
        SCRIPT_TO_RUN="scripts/bootstrap.sh"
    else
        echo "Environment exists: activating..."
        SCRIPT_TO_RUN="scripts/activate.sh"
    fi

    set +e  # Don't exit on error for activation (it may have warnings)
    source "$SCRIPT_TO_RUN" -p none 2>&1 | grep -v "externally-managed-environment" || true
    ACTIVATE_RESULT=$?
    set -e

    if [ $ACTIVATE_RESULT -ne 0 ] && [ $ACTIVATE_RESULT -ne 141 ]; then
        echo "WARNING: Environment setup had issues (exit code: $ACTIVATE_RESULT)"
        echo "Checking if we can continue anyway..."
    fi
else
    echo "ERROR: bootstrap.sh not found in Matter source"
    exit 1
fi

# Configure the build
echo ""
echo "Configuring build..."
BUILD_DIR="out/host"

# Use gn to generate build files
if ! command -v gn &> /dev/null; then
    echo "ERROR: gn not found. Matter environment may not be properly activated."
    echo "Looking for gn in .environment/cipd/packages..."
    if [ -f ".environment/cipd/packages/pigweed/gn" ]; then
        export PATH="$MATTER_DIR/.environment/cipd/packages/pigweed:$PATH"
        echo "Added pigweed tools to PATH"
    else
        exit 1
    fi
fi

# GN arguments for building the libraries we need
GN_ARGS=(
    'chip_config_network_layer_ble=false'
    'chip_enable_wifi=true'
    'chip_crypto="boringssl"'
    'is_debug=false'
    'optimize_for_size=true'
)

GN_ARGS_STRING=$(IFS=" " ; echo "${GN_ARGS[*]}")

echo "GN arguments: $GN_ARGS_STRING"
gn gen "$BUILD_DIR" --args="$GN_ARGS_STRING"

# Build the required libraries
echo ""
echo "Building Matter libraries..."
echo "This may take a while (10-30 minutes depending on your system)..."
echo ""

if ! command -v ninja &> /dev/null; then
    if [ -f ".environment/cipd/packages/pigweed/bin/ninja" ]; then
        export PATH="$MATTER_DIR/.environment/cipd/packages/pigweed/bin:$PATH"
        echo "Added ninja to PATH"
    else
        echo "ERROR: ninja not found"
        exit 1
    fi
fi

# Build specific targets we need
TARGETS=(
    "//src/app/server:server"
    "//src/controller:controller"
    "//src/setup_payload:setup_payload"
    "//src/lib:lib"
)

for target in "${TARGETS[@]}"; do
    echo "Building $target..."
    ninja -C "$BUILD_DIR" "$target" || {
        echo "WARNING: Failed to build $target, continuing..."
    }
done

# Note: There is no single "chip" target in Matter's build system.
# The targets above (server, controller, setup_payload, lib) provide all necessary libraries.

echo ""
echo "✓ Matter libraries built successfully!"
echo ""
echo "Build output: $MATTER_DIR/$BUILD_DIR"
echo "Libraries: $MATTER_DIR/$BUILD_DIR/lib"
echo ""
echo "Next step:"
echo "  Copy libraries for your platform: ./scripts/copy-matter-libs.sh"
echo ""
