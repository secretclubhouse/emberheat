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
MATTER_VERSION="${MATTER_VERSION:-v1.4.0.0}"
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

# Download the release tarball
echo "Downloading Matter $MATTER_VERSION..."
TARBALL_URL="$MATTER_REPO/archive/refs/tags/$MATTER_VERSION.tar.gz"
TEMP_TAR="$PROJECT_ROOT/matter-$MATTER_VERSION.tar.gz"

if command -v wget > /dev/null; then
    wget -O "$TEMP_TAR" "$TARBALL_URL"
elif command -v curl > /dev/null; then
    curl -L -o "$TEMP_TAR" "$TARBALL_URL"
else
    echo "ERROR: Neither wget nor curl found. Please install one of them."
    exit 1
fi

# Extract the tarball
echo "Extracting archive..."
mkdir -p "$MATTER_DIR"
# GitHub archives have a top-level directory named <repo>-<version>
# Strip that component and extract directly into MATTER_DIR
tar -xzf "$TEMP_TAR" -C "$MATTER_DIR" --strip-components=1

# Clean up tarball
rm "$TEMP_TAR"

echo ""
echo "✓ Matter source code downloaded successfully!"
echo ""
echo "Next steps:"
echo "  1. Build Matter libraries: make matter-build"
echo "  2. Build thermos-rust: make build"
echo ""
