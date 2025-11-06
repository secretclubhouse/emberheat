# Matter Library Build Guide

This guide covers building the Matter/Connected Home IP libraries for the thermos-rust project.

## TL;DR - Quick Start

**Recommended**: Use Docker to build (avoids all local config issues)

```bash
make matter-setup-docker
```

That's it! The libraries will be built and placed in `lib/connectedhomeip/<platform>/`.

## Why Docker Build?

Building Matter libraries locally often fails due to:
- Missing system dependencies
- Python environment conflicts
- Incorrect compiler versions
- Platform-specific configuration issues

**Docker solves this** by providing a clean, reproducible build environment.

## Build Methods

### Method 1: Docker Build (Recommended)

**When to use**: Always, unless you need to modify Matter source frequently

**Pros**:
- No local setup required
- Works on any system with Docker
- Reproducible builds
- Isolated from your system

**Cons**:
- Requires Docker (~2-3 GB)
- Slightly longer first-time setup

**Steps**:
```bash
# Build everything in one command
make matter-setup-docker

# Or build step-by-step
make matter-build-docker

# For specific platform
./scripts/docker-build-matter.sh linux-arm64
```

**Time**: ~40-60 minutes total (5-10 min Docker image build + 30-45 min Matter build)

### Method 2: Local Build

**When to use**: When you're actively developing/modifying Matter source code

**Pros**:
- Faster rebuilds after changes
- Direct access to source

**Cons**:
- Complex setup
- Platform-specific issues
- May conflict with local environment

**Steps**:
```bash
# Complete setup
make matter-setup

# Or step-by-step
make matter-download  # Download source
make matter-build     # Build libraries
make matter-libs      # Copy to lib/
```

**Time**: ~45-90 minutes (setup varies widely by system)

## After Building

### 1. Verify Build

```bash
# Check libraries exist
ls -lh lib/connectedhomeip/*/

# Should see ~50+ .a files totaling 150-300 MB
```

### 2. Test with Cargo

```bash
cargo build
```

If this succeeds, the Matter libraries are correctly built and linked.

### 3. Commit Libraries

The built libraries should be committed so other developers don't need to rebuild:

```bash
git add lib/connectedhomeip/
git commit -m "Add pre-built Matter libraries for <platform>"
git push
```

## Troubleshooting

### Docker Build Issues

**Problem**: Docker daemon not running
**Solution**: Start Docker Desktop

**Problem**: Out of disk space
**Solution**:
```bash
docker system prune -a  # Clean old images
```

**Problem**: Build fails in container
**Solution**: Check you have 4+ GB RAM allocated to Docker in Docker Desktop settings

### Local Build Issues

**Problem**: Python environment errors
**Solution**: Use Docker build instead, or create a fresh Python venv

**Problem**: Missing dependencies
**Solution**: See [docs/matter-docker-build.md](docs/matter-docker-build.md) for dependency list, or use Docker build

**Problem**: Build takes forever
**Solution**: This is normal - Matter is a large project. Docker build is not slower.

## Build Targets Reference

| Command | Description | Time | When to Use |
|---------|-------------|------|-------------|
| `make matter-setup-docker` | Complete Docker build | ~50 min | First time, or anytime |
| `make matter-setup` | Complete local build | ~60+ min | When modifying Matter source |
| `make matter-build-docker` | Just Docker build | ~45 min | After downloading source |
| `make matter-build` | Just local build | ~45 min | Rebuilding after changes |
| `make matter-download` | Download source only | ~5 min | Before local build |
| `make matter-clean` | Remove source | ~1 min | Clean up after building |

## Architecture Support

Currently supported platforms:
- **linux-x64**: Linux x86_64 (most servers, desktops)
- **linux-arm64**: Linux ARM64 (Raspberry Pi 4, AWS Graviton)

The Docker build automatically detects your system and builds for Linux (even on macOS).

## File Locations

```
thermos-rust/
├── Dockerfile.matter-build          # Docker image for building Matter
├── scripts/
│   ├── docker-build-matter.sh       # Host script (run this)
│   ├── docker-build-matter-internal.sh  # Container script (auto-run)
│   ├── build-matter.sh              # Local build script
│   ├── download-matter.sh           # Download Matter source
│   └── copy-matter-libs.sh          # Copy libs after local build
├── third_party/
│   └── connectedhomeip/             # Matter source (temporary, not committed)
│       └── out/host/lib/            # Build output (temporary)
└── lib/
    └── connectedhomeip/
        └── linux-x64/               # Final libraries (COMMITTED)
            └── *.a                  # ~50+ library files
```

## FAQ

**Q: How often do I need to rebuild?**
A: Only when updating to a new Matter version, or when building for a new platform.

**Q: Do I commit the Matter source code?**
A: No. Only commit the built libraries in `lib/connectedhomeip/`. The source in `third_party/connectedhomeip/` is temporary.

**Q: Can I build on macOS?**
A: Use Docker build. It will build Linux libraries that can be used in containers/deployment.

**Q: What if the build fails?**
A: Try Docker build first. If that fails, check disk space and Docker memory settings.

**Q: Can I build for multiple platforms?**
A: Yes:
```bash
./scripts/docker-build-matter.sh linux-x64
./scripts/docker-build-matter.sh linux-arm64
```

**Q: How do I update to a new Matter version?**
A:
```bash
MATTER_VERSION=v1.4.2.0 make matter-setup-docker
```

## Getting Help

- **Detailed Docker build docs**: [docs/matter-docker-build.md](docs/matter-docker-build.md)
- **Local build issues**: Check Matter's [official docs](https://github.com/project-chip/connectedhomeip)
- **Disk space**: Run `docker system df` to see Docker usage

## Recommended Workflow

For most developers:

1. **First time**: `make matter-setup-docker` (once, ~50 min)
2. **Development**: `cargo build`, `cargo test` (normal Rust development)
3. **Update Matter**: Re-run `make matter-setup-docker` (rarely needed)
4. **New platform**: `./scripts/docker-build-matter.sh <platform>` (as needed)

The Matter libraries are stable dependencies that rarely change, so you typically only build them once.
