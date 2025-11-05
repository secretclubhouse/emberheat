#!/bin/bash
#
# Download and extract Matter/Connected Home IP source code
# This script downloads a specific release from GitHub instead of using git submodules
#

set -e  # Exit on error

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MATTER_DIR="$PROJECT_ROOT/third_party/connectedhomeip"

# Default to latest stable release
# You can override with: MATTER_VERSION=v1.2.0 ./scripts/download-matter.sh
MATTER_VERSION="${MATTER_VERSION:-v1.4.2.0}"
MATTER_REPO="https://github.com/project-chip/connectedhomeip"

echo "============================================"
echo "Matter/Connected Home IP Download Script"
echo "============================================"
echo ""
echo "Version: $MATTER_VERSION"
echo "Target:  $MATTER_DIR"
echo ""

# Check if directory already exists
if [ -d "$MATTER_DIR" ]; then
    echo "Matter directory already exists: $MATTER_DIR"
    read -p "Do you want to re-download? This will delete the existing directory. (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Skipping download."
        exit 0
    fi
    echo "Removing existing directory..."
    rm -rf "$MATTER_DIR"
fi

# Create third_party directory if it doesn't exist
mkdir -p "$(dirname "$MATTER_DIR")"

# Use a specific-commit clone for Matter source
# We clone the full repository but checkout a specific version
# This ensures all build files are present
echo "Cloning Matter $MATTER_VERSION..."
echo "This will download ~500MB of data (one-time download)..."
echo ""
git clone --depth=1 --branch "$MATTER_VERSION" --single-branch \
    "$MATTER_REPO" "$MATTER_DIR"

echo ""
echo "Initializing required submodules..."
echo "This will download additional dependencies..."
echo ""

cd "$MATTER_DIR"

# Download required platforms
echo "Initializing platform submodules..."
python3 scripts/checkout_submodules.py --shallow --platform linux


# Initialize the minimal set of submodules needed for host builds
# We avoid --recursive to prevent downloading platform-specific dependencies
#echo "Initializing Pigweed (build system)..."
#git submodule update --init third_party/pigweed/repo

# Initialize Pigweed's essential dependencies
#echo "Initializing Pigweed dependencies..."
#cd third_party/pigweed/repo
# Pigweed needs these for its build system
#git submodule update --init \
#    third_party/fuzztest \
#    third_party/googletest \
#    pw_third_party/nanopb 2>/dev/null || true
#cd "$MATTER_DIR"

# Other Matter dependencies
#echo "Initializing Matter dependencies..."
#git submodule update --init \
#    third_party/nlassert/repo \
#    third_party/nlio/repo \
#    2>/dev/null || true

echo ""
echo "✓ Matter source code downloaded and initialized successfully!"
echo ""
echo "Next steps:"
echo "  1. Build Matter libraries: make matter-build"
echo "  2. Build thermos-rust: make build"
echo ""
