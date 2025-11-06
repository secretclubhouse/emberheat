#!/usr/bin/env bash
#
# Build Matter/Connected Home IP libraries using Docker
# This script builds the libraries in an isolated container to avoid local configuration issues
#
# Usage:
#   ./scripts/docker-build-matter.sh [platform]
#
# Arguments:
#   platform - Optional platform override (e.g., linux-x64, linux-arm64)
#              If not specified, will be auto-detected based on host

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Get project root
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$PROJECT_ROOT"

# Configuration
MATTER_VERSION="${MATTER_VERSION:-v1.4.2.0}"
DOCKER_IMAGE="matter-builder:latest"
CONTAINER_NAME="matter-build-$$"

echo -e "${BLUE}============================================${NC}"
echo -e "${BLUE}Matter Docker Build${NC}"
echo -e "${BLUE}============================================${NC}"
echo ""

# Detect platform
detect_platform() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)

    case "$os" in
        darwin) os="linux" ;;  # We build for Linux even on macOS
        linux) os="linux" ;;
        *)
            echo -e "${RED}Error: Unsupported OS: $os${NC}" >&2
            exit 1
            ;;
    esac

    case "$arch" in
        x86_64|amd64) arch="x64" ;;
        aarch64|arm64) arch="arm64" ;;
        *)
            echo -e "${RED}Error: Unsupported architecture: $arch${NC}" >&2
            exit 1
            ;;
    esac

    echo "${os}-${arch}"
}

# Determine target platform
if [ -n "$1" ]; then
    PLATFORM="$1"
    echo -e "${BLUE}Using specified platform: ${PLATFORM}${NC}"
else
    PLATFORM=$(detect_platform)
    echo -e "${BLUE}Auto-detected platform: ${PLATFORM}${NC}"
fi

echo -e "${BLUE}Matter version: ${MATTER_VERSION}${NC}"
echo ""

# Check if Docker is available
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed or not in PATH${NC}" >&2
    echo "Please install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

# Check if Docker daemon is running
if ! docker info &> /dev/null; then
    echo -e "${RED}Error: Docker daemon is not running${NC}" >&2
    echo "Please start Docker and try again"
    exit 1
fi

# Build the Docker image
echo -e "${GREEN}Step 1: Building Docker image...${NC}"
echo "This may take a few minutes on first run (downloads ~1GB of dependencies)"
echo ""

docker build -f Dockerfile.matter-build -t "$DOCKER_IMAGE" . || {
    echo -e "${RED}Error: Failed to build Docker image${NC}" >&2
    exit 1
}

echo ""
echo -e "${GREEN}Step 2: Running Matter build in container...${NC}"
echo "This will take 15-45 minutes depending on your system"
echo ""

# Run the build in Docker
# Mount the project root so we can extract artifacts
docker run --rm \
    --name "$CONTAINER_NAME" \
    -v "$PROJECT_ROOT:/output" \
    -e "MATTER_VERSION=$MATTER_VERSION" \
    -e "TARGET_PLATFORM=$PLATFORM" \
    "$DOCKER_IMAGE" || {
    echo -e "${RED}Error: Build failed in container${NC}" >&2
    exit 1
}

echo ""
echo -e "${GREEN}Step 3: Verifying build artifacts...${NC}"

LIBS_DIR="lib/connectedhomeip/${PLATFORM}"
if [ ! -d "$LIBS_DIR" ]; then
    echo -e "${RED}Error: Libraries directory not found: $LIBS_DIR${NC}" >&2
    exit 1
fi

# Count library files
LIB_COUNT=$(find "$LIBS_DIR" -name "*.a" | wc -l)
TOTAL_SIZE=$(du -sh "$LIBS_DIR" | cut -f1)

echo -e "${GREEN}✓ Build completed successfully!${NC}"
echo ""
echo -e "${BLUE}Platform:${NC} $PLATFORM"
echo -e "${BLUE}Location:${NC} $LIBS_DIR"
echo -e "${BLUE}Libraries:${NC} $LIB_COUNT files"
echo -e "${BLUE}Total size:${NC} $TOTAL_SIZE"
echo ""
echo -e "${YELLOW}Next steps:${NC}"
echo "  1. Test the build: cargo build"
echo "  2. Commit the libraries:"
echo "     git add lib/connectedhomeip/${PLATFORM}/"
echo "     git commit -m \"Add pre-built Matter libraries for ${PLATFORM}\""
echo ""
