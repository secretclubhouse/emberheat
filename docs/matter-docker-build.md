# Building Matter Libraries with Docker

This guide explains how to build the Matter/Connected Home IP libraries using Docker, which avoids local system configuration issues.

## Overview

Building Matter libraries locally can be challenging due to:

- Complex dependency requirements
- Platform-specific configuration issues
- Python environment conflicts
- Missing build tools

The Docker-based build solves these problems by:

- Using a clean, reproducible build environment
- Handling all dependencies automatically
- Running the build in isolation from your local system
- Extracting only the final built artifacts

## Prerequisites

- Docker installed and running
- ~2-3 GB of free disk space
- 30-60 minutes for the build (depending on system)

## Quick Start

### Build for Your Current Platform

```bash
make matter-setup-docker
```

This command will:

1. Build the Docker image with all required dependencies (~5-10 minutes)
2. Download Matter source code inside the container
3. Build the Matter libraries (~20-40 minutes)
4. Copy the built libraries to `lib/connectedhomeip/<platform>/`

### Build for a Specific Platform

```bash
./scripts/docker-build-matter.sh linux-arm64
```

Supported platforms:

- `linux-x64` - Linux x86_64 (default on most systems)
- `linux-arm64` - Linux ARM64 (e.g., Raspberry Pi 4, AWS Graviton)

## What Gets Built

The build process creates static libraries (`.a` files) including:

- `libCHIPAppServerLib.a` - Matter application server
- `libchip.a` - Core Matter/CHIP library
- `libCHIPController.a` - Matter controller functionality
- `libCHIPSetupPayload.a` - Device commissioning
- Additional dependencies (~50+ libraries total)

Total size: ~150-300 MB depending on platform

## After Building

### 1. Verify the Build

Check that libraries were created:

```bash
ls -lh lib/connectedhomeip/*/
```

### 2. Test with Rust

Build your Rust project to ensure everything links correctly:

```bash
cargo build
```

### 3. Commit the Libraries

The built libraries should be committed to version control so other developers don't need to rebuild:

```bash
git add lib/connectedhomeip/
git commit -m "Add pre-built Matter libraries for $(uname -s | tr '[:upper:]' '[:lower:]')-$(uname -m)"
```

## Build Architecture

### Files

- `Dockerfile.matter-build` - Docker image definition with all build dependencies
- `scripts/docker-build-matter.sh` - Host script that orchestrates the build
- `scripts/docker-build-matter-internal.sh` - Script that runs inside the container

### Process Flow

```
┌─────────────────────────────────────────────────────────────┐
│ Host System                                                  │
│                                                              │
│  1. Run: make matter-setup-docker                           │
│     └─> scripts/docker-build-matter.sh                      │
│                                                              │
│  2. Build Docker Image                                       │
│     └─> docker build -f Dockerfile.matter-build             │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐ │
│  │ Docker Container                                        │ │
│  │                                                         │ │
│  │  3. Download Matter Source                             │ │
│  │     └─> git clone connectedhomeip                      │ │
│  │                                                         │ │
│  │  4. Initialize Submodules                              │ │
│  │     └─> python3 checkout_submodules.py                 │ │
│  │                                                         │ │
│  │  5. Bootstrap Build Environment                        │ │
│  │     └─> source scripts/bootstrap.sh                    │ │
│  │                                                         │ │
│  │  6. Configure with GN                                  │ │
│  │     └─> gn gen out/host                                │ │
│  │                                                         │ │
│  │  7. Build with Ninja                                   │ │
│  │     └─> ninja -C out/host (20-40 minutes)              │ │
│  │                                                         │ │
│  │  8. Copy Libraries to /output                          │ │
│  │     └─> cp out/host/lib/*.a /output/lib/...            │ │
│  │                                                         │ │
│  └────────────────────────────────────────────────────────┘ │
│                                                              │
│  9. Libraries Available in lib/connectedhomeip/             │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

## Troubleshooting

### Docker Daemon Not Running

**Error**: "Cannot connect to the Docker daemon"

**Solution**: Start Docker Desktop or run `sudo systemctl start docker` on Linux

### Insufficient Disk Space

**Error**: "No space left on device"

**Solution**:

```bash
docker system prune -a  # Clean up old images
```

Make sure you have at least 3 GB free.

### Build Fails in Container

**Error**: Build exits with error code

**Solution**: Check the build logs. Common issues:

- Network timeout during git clone - retry the build
- Out of memory - increase Docker memory limit in Docker Desktop settings (recommend 4+ GB)

### Wrong Architecture Built

**Problem**: Built for x64 but need arm64 or vice versa

**Solution**: Specify the platform explicitly:

```bash
./scripts/docker-build-matter.sh linux-arm64
```

## Advanced Usage

### Custom Matter Version

Build a different version of Matter:

```bash
MATTER_VERSION=v1.3.0.0 make matter-setup-docker
```

### Keep Docker Container for Debugging

Modify `scripts/docker-build-matter.sh` and remove the `--rm` flag from the docker run command to keep the container after it exits.

### Build Multiple Platforms

Build for both x64 and arm64:

```bash
./scripts/docker-build-matter.sh linux-x64
./scripts/docker-build-matter.sh linux-arm64
```

## Comparison with Host Build

| Aspect | Docker Build | Host Build |
|--------|-------------|------------|
| Setup Time | ~10 min (one-time) | Varies (can be hours) |
| Dependencies | Automatic | Manual installation |
| Reproducibility | High | Low |
| Platform Issues | Isolated | Can affect host |
| Disk Space | ~3 GB | ~2 GB |
| Build Time | 30-60 min | 30-60 min |

**Recommendation**: Use Docker build unless you need to modify Matter source code frequently.

## CI/CD Integration

For automated builds in CI/CD pipelines:

```yaml
# GitHub Actions example
- name: Build Matter Libraries
  run: make matter-setup-docker

- name: Commit Libraries
  run: |
    git config user.name "CI Bot"
    git config user.email "ci@example.com"
    git add lib/connectedhomeip/
    git commit -m "Update Matter libraries"
```

## Cleanup

Remove the Docker image to free up space:

```bash
docker rmi matter-builder:latest
```

The built libraries in `lib/connectedhomeip/` are preserved.
