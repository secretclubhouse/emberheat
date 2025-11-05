#!/bin/bash
#
# Test script to verify Matter download and setup
#

set -e

echo "Testing Matter download and setup..."
echo ""

# Test 1: Check scripts exist
echo "[1/5] Checking scripts..."
for script in download-matter.sh build-matter.sh copy-matter-libs.sh; do
    if [ -f "scripts/$script" ]; then
        echo "  ✓ $script exists"
    else
        echo "  ✗ $script missing!"
        exit 1
    fi
done

# Test 2: Check if Matter source exists
echo ""
echo "[2/5] Checking Matter source..."
if [ -d "third_party/connectedhomeip" ]; then
    echo "  ✓ Matter source directory exists"
    if [ -f "third_party/connectedhomeip/BUILD.gn" ]; then
        echo "  ✓ Matter source appears valid (BUILD.gn found)"
    else
        echo "  ⚠ Matter source may be incomplete"
    fi
else
    echo "  ℹ Matter source not downloaded yet"
    echo "  Run: make matter-download"
fi

# Test 3: Check build environment
echo ""
echo "[3/5] Checking build environment..."
if [ -d "third_party/connectedhomeip/.environment" ]; then
    echo "  ✓ Matter build environment exists"
else
    echo "  ℹ Matter build environment not initialized yet"
    echo "  Run: make matter-build"
fi

# Test 4: Check built libraries
echo ""
echo "[4/5] Checking built libraries..."
if [ -d "third_party/connectedhomeip/out/host/lib" ]; then
    lib_count=$(find third_party/connectedhomeip/out/host/lib -name "*.a" 2>/dev/null | wc -l)
    echo "  ✓ Build output exists ($lib_count .a files)"
else
    echo "  ℹ No build output yet"
    echo "  Run: make matter-build"
fi

# Test 5: Check pre-built libraries
echo ""
echo "[5/5] Checking pre-built libraries..."
if [ -d "lib/connectedhomeip" ]; then
    for platform_dir in lib/connectedhomeip/*; do
        if [ -d "$platform_dir" ]; then
            platform=$(basename "$platform_dir")
            lib_count=$(find "$platform_dir" -name "*.a" 2>/dev/null | wc -l)
            echo "  ✓ $platform ($lib_count .a files)"
        fi
    done
else
    echo "  ℹ No pre-built libraries yet"
    echo "  Run: make matter-libs"
fi

echo ""
echo "Test complete!"
echo ""
echo "Next steps:"
if [ ! -d "third_party/connectedhomeip" ]; then
    echo "  1. make matter-download"
    echo "  2. make matter-build"
    echo "  3. make matter-libs"
elif [ ! -d "third_party/connectedhomeip/.environment" ]; then
    echo "  1. make matter-build"
    echo "  2. make matter-libs"
elif [ ! -d "lib/connectedhomeip" ] || [ -z "$(ls -A lib/connectedhomeip 2>/dev/null)" ]; then
    echo "  1. make matter-libs"
else
    echo "  ✓ Everything looks good!"
    echo "  Run: make build"
fi
