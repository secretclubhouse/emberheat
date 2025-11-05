#!/usr/bin/env bash
#
# Copy pre-built Matter libraries from the Connected Home IP submodule build
# to the lib/connectedhomeip directory for version control.
#
# Usage:
#   ./scripts/copy-matter-libs.sh [platform]
#
# If platform is not specified, it will be auto-detected.
#
# Supported platforms:
#   - darwin-arm64  (macOS Apple Silicon)
#   - darwin-x64    (macOS Intel)
#   - linux-arm64   (Linux ARM64, e.g., Raspberry Pi)
#   - linux-x64     (Linux x86_64)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Detect platform
detect_platform() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)

    # Normalize OS name
    case "$os" in
        darwin)
            os="darwin"
            ;;
        linux)
            os="linux"
            ;;
        *)
            echo -e "${RED}Error: Unsupported OS: $os${NC}" >&2
            exit 1
            ;;
    esac

    # Normalize architecture
    case "$arch" in
        x86_64|amd64)
            arch="x64"
            ;;
        aarch64|arm64)
            arch="arm64"
            ;;
        *)
            echo -e "${RED}Error: Unsupported architecture: $arch${NC}" >&2
            exit 1
            ;;
    esac

    echo "${os}-${arch}"
}

# Main script
main() {
    # Get project root (script is in scripts/)
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
    PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

    cd "$PROJECT_ROOT"

    # Determine platform
    if [ -n "$1" ]; then
        PLATFORM="$1"
        echo -e "${BLUE}Using specified platform: ${PLATFORM}${NC}"
    else
        PLATFORM=$(detect_platform)
        echo -e "${BLUE}Auto-detected platform: ${PLATFORM}${NC}"
    fi

    # Paths
    SOURCE_DIR="third_party/connectedhomeip/out/host/lib"
    DEST_DIR="lib/connectedhomeip/${PLATFORM}"

    # Check if source directory exists
    if [ ! -d "$SOURCE_DIR" ]; then
        echo -e "${RED}Error: Source directory not found: ${SOURCE_DIR}${NC}" >&2
        echo -e "${YELLOW}Please build the Connected Home IP library first:${NC}"
        echo "  cd third_party/connectedhomeip"
        echo "  source scripts/activate.sh"
        echo "  gn gen out/host"
        echo "  ninja -C out/host"
        exit 1
    fi

    # Create destination directory
    mkdir -p "$DEST_DIR"

    # Required libraries
    REQUIRED_LIBS=(
        "libCHIPAppServerLib.a"
        "libchip.a"
        "libCHIPController.a"
        "libCHIPSetupPayload.a"
    )

    # Check for required libraries
    echo -e "${BLUE}Checking for required libraries...${NC}"
    missing_libs=()
    for lib in "${REQUIRED_LIBS[@]}"; do
        if [ ! -f "$SOURCE_DIR/$lib" ]; then
            missing_libs+=("$lib")
        fi
    done

    if [ ${#missing_libs[@]} -gt 0 ]; then
        echo -e "${RED}Error: Missing required libraries in ${SOURCE_DIR}:${NC}"
        for lib in "${missing_libs[@]}"; do
            echo "  - $lib"
        done
        exit 1
    fi

    # Copy libraries
    echo -e "${GREEN}Copying libraries to ${DEST_DIR}...${NC}"
    total_size=0
    for lib in "${REQUIRED_LIBS[@]}"; do
        echo -e "  ${BLUE}→${NC} $lib"
        cp "$SOURCE_DIR/$lib" "$DEST_DIR/"
        size=$(stat -f%z "$SOURCE_DIR/$lib" 2>/dev/null || stat -c%s "$SOURCE_DIR/$lib" 2>/dev/null)
        total_size=$((total_size + size))
    done

    # Copy additional libraries that might be needed
    echo -e "${BLUE}Copying additional dependencies...${NC}"
    additional_libs=$(find "$SOURCE_DIR" -name "*.a" ! -name "libCHIPAppServerLib.a" ! -name "libchip.a" ! -name "libCHIPController.a" ! -name "libCHIPSetupPayload.a" | head -20)

    for lib in $additional_libs; do
        lib_name=$(basename "$lib")
        echo -e "  ${BLUE}→${NC} $lib_name"
        cp "$lib" "$DEST_DIR/"
        size=$(stat -f%z "$lib" 2>/dev/null || stat -c%s "$lib" 2>/dev/null)
        total_size=$((total_size + size))
    done

    # Convert total size to human readable
    if [ $total_size -gt 1048576 ]; then
        size_mb=$((total_size / 1048576))
        size_str="${size_mb} MB"
    else
        size_kb=$((total_size / 1024))
        size_str="${size_kb} KB"
    fi

    echo ""
    echo -e "${GREEN}✓ Successfully copied Matter libraries${NC}"
    echo -e "${BLUE}Platform:${NC} ${PLATFORM}"
    echo -e "${BLUE}Location:${NC} ${DEST_DIR}"
    echo -e "${BLUE}Total size:${NC} ${size_str}"
    echo ""
    echo -e "${YELLOW}Next steps:${NC}"
    echo "  1. Test the build: cargo build"
    echo "  2. Commit the libraries: git add lib/connectedhomeip/${PLATFORM}/"
    echo "  3. Create commit: git commit -m \"Add pre-built Matter libraries for ${PLATFORM}\""
}

main "$@"
