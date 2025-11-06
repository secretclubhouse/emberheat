#!/bin/bash
#
# Internal script that runs INSIDE the Docker container
# Downloads Matter source, builds libraries, and copies them to the output directory
#

set -e

# Configuration from environment variables
MATTER_VERSION="${MATTER_VERSION:-v1.4.2.0}"
TARGET_PLATFORM="${TARGET_PLATFORM:-linux-x64}"
MATTER_REPO="https://github.com/project-chip/connectedhomeip"

echo "============================================"
echo "Matter Build (Inside Container)"
echo "============================================"
echo "Version: $MATTER_VERSION"
echo "Platform: $TARGET_PLATFORM"
echo ""

# Working directories
BUILD_DIR="/build"
MATTER_DIR="$BUILD_DIR/connectedhomeip"
OUTPUT_BASE="/output"

cd "$BUILD_DIR"

# Step 1: Download Matter source
echo "Step 1: Downloading Matter source..."
echo "This will download ~500MB..."
echo ""

if [ -d "$MATTER_DIR" ]; then
    echo "Removing existing Matter directory..."
    rm -rf "$MATTER_DIR"
fi

git clone --depth=1 --branch "$MATTER_VERSION" --single-branch \
    "$MATTER_REPO" "$MATTER_DIR"

cd "$MATTER_DIR"

# Step 2: Initialize submodules
echo ""
echo "Step 2: Initializing submodules..."
echo "This will download additional dependencies..."
echo ""

# Use Matter's checkout script to get required submodules
python3 scripts/checkout_submodules.py --shallow --platform linux

# Step 3: Bootstrap the build environment
echo ""
echo "Step 3: Bootstrapping Matter build environment..."
echo "This may take 5-10 minutes (downloads build tools)..."
echo ""

# Bootstrap creates a Python virtual environment and downloads build tools
source scripts/bootstrap.sh -p none

# Step 4: Configure the build
echo ""
echo "Step 4: Configuring build..."
echo ""

BUILD_OUT="out/unified"

# Determine architecture for GN
case "$TARGET_PLATFORM" in
    *-x64)
        TARGET_ARCH="x64"
        ;;
    *-arm64)
        TARGET_ARCH="arm64"
        ;;
    *-arm)
        TARGET_ARCH="arm"
        ;;
    *)
        echo "Warning: Unknown platform $TARGET_PLATFORM, using x64"
        TARGET_ARCH="x64"
        ;;
esac

# GN arguments for building libraries
GN_ARGS=(
    "target_cpu=\"$TARGET_ARCH\""
    'chip_config_network_layer_ble=false'
    'chip_enable_wifi=true'
    'chip_crypto="boringssl"'
    'is_debug=false'
    'optimize_for_size=true'
)

GN_ARGS_STRING=$(IFS=" " ; echo "${GN_ARGS[*]}")
echo "GN arguments: $GN_ARGS_STRING"

gn gen "$BUILD_OUT" --args="$GN_ARGS_STRING"

# Step 5: Build the libraries
echo ""
echo "Step 5: Building Matter libraries..."
echo "This may take 20-40 minutes depending on system resources..."
echo ""

# Build key targets
TARGETS=(
    "src/app/server:server"
    "src/controller:controller"
    "src/setup_payload:setup_payload"
    "src/lib:lib"
)

for target in "${TARGETS[@]}"; do
    echo "Building $target..."
    ninja -C "$BUILD_OUT" "$target" 2>&1 | grep -v "warning:" || true
done

# Note: There is no single "chip" target in Matter's build system.
# The targets above (server, controller, setup_payload, lib) provide all necessary libraries.

# Step 6: Copy libraries to output
echo ""
echo "Step 6: Copying libraries to output directory..."
echo ""

SOURCE_LIB_DIR="$MATTER_DIR/$BUILD_OUT/lib"
DEST_LIB_DIR="$OUTPUT_BASE/lib/connectedhomeip/$TARGET_PLATFORM"

if [ ! -d "$SOURCE_LIB_DIR" ]; then
    echo "ERROR: Build output directory not found: $SOURCE_LIB_DIR"
    exit 1
fi

# Create destination directory
mkdir -p "$DEST_LIB_DIR"

# Copy all static libraries
echo "Copying all .a files from $SOURCE_LIB_DIR..."
LIB_COUNT=0
TOTAL_SIZE=0

for lib in "$SOURCE_LIB_DIR"/*.a; do
    if [ -f "$lib" ]; then
        lib_name=$(basename "$lib")
        cp "$lib" "$DEST_LIB_DIR/"
        size=$(stat -c%s "$lib" 2>/dev/null || echo 0)
        TOTAL_SIZE=$((TOTAL_SIZE + size))
        LIB_COUNT=$((LIB_COUNT + 1))
        echo "  → $lib_name"
    fi
done

# Convert size to MB
SIZE_MB=$((TOTAL_SIZE / 1048576))

echo ""
echo "✓ Build completed successfully!"
echo ""
echo "Libraries copied: $LIB_COUNT files"
echo "Total size: ${SIZE_MB} MB"
echo "Output location: lib/connectedhomeip/$TARGET_PLATFORM/"
echo ""
