# Matter Setup - Current Status and Next Steps

## What We've Built

I've created a complete build system to replace the git submodule approach with downloadable Matter source code.

### New Scripts Created

1. **[scripts/download-matter.sh](scripts/download-matter.sh)** - Downloads Matter source from GitHub
2. **[scripts/build-matter.sh](scripts/build-matter.sh)** - Builds Matter libraries
3. **[scripts/copy-matter-libs.sh](scripts/copy-matter-libs.sh)** - Copies libraries to `lib/`
4. **[scripts/test-matter-download.sh](scripts/test-matter-download.sh)** - Tests the setup

### Makefile Targets Added

- `make matter-download` - Download Matter source
- `make matter-build` - Build Matter libraries
- `make matter-libs` - Copy to lib/ directory
- `make matter-setup` - Complete setup (all three steps)
- `make matter-clean` - Remove downloaded source

### Documentation Created

- [docs/BUILDING_MATTER.md](docs/BUILDING_MATTER.md) - Complete build guide
- [docs/MIGRATION_FROM_SUBMODULE.md](docs/MIGRATION_FROM_SUBMODULE.md) - Migration guide
- [third_party/README.md](third_party/README.md) - Third-party deps overview

## Current Issue

The Matter build system has complex nested dependencies (Pigweed with its own submodules). The shallow clone approach was missing files.

### Latest Fix

The download script now does a full (not shallow) clone of:
1. Matter v1.4.0.0
2. Pigweed submodule
3. Pigweed's dependencies (fuzztest, googletest, nanopb)
4. Matter dependencies (nlassert, nlio)

## To Complete the Setup

### Option 1: Clean Start (Recommended)

```bash
# 1. Remove current incomplete download
rm -rf third_party/connectedhomeip

# 2. Download fresh with updated script
./scripts/download-matter.sh

# 3. Try building
make matter-build
```

This will download ~500MB-1GB but should work completely.

### Option 2: Using Pre-Built Libraries

If you don't want to build Matter yourself:

1. Get pre-built libraries from another developer
2. Place them in `lib/connectedhomeip/<platform>/`
3. Just run `make build`

## Files Changed

### Modified
- [.gitignore](.gitignore) - Exclude downloaded Matter source
- [.gitmodules](.gitmodules) - Removed Matter submodule
- [Makefile](Makefile) - Added matter-* targets

### Created
- [scripts/download-matter.sh](scripts/download-matter.sh)
- [scripts/build-matter.sh](scripts/build-matter.sh)
- [scripts/test-matter-download.sh](scripts/test-matter-download.sh)
- [docs/BUILDING_MATTER.md](docs/BUILDING_MATTER.md)
- [docs/MIGRATION_FROM_SUBMODULE.md](docs/MIGRATION_FROM_SUBMODULE.md)
- [third_party/README.md](third_party/README.md)
- [third_party/.gitkeep](third_party/.gitkeep)

### Removed
- Git submodule: `third_party/connectedhomeip` (now downloaded instead)

## Benefits of New System

1. **No Submodule Issues** - ARM GCC bug automatically worked around
2. **Version Control** - Easy to switch Matter versions
3. **Cleaner Git** - No nested submodule complexity
4. **Pre-built Libraries** - Team can share compiled libraries
5. **Optional Build** - Can use pre-built libs without building Matter

## Alternative: Skip Matter Build Entirely

If the Matter build is too complex, you can:

1. Focus on the Rust application logic first
2. Use stub Matter implementation ([build.rs](build.rs) already supports this)
3. Add Matter integration later when you have access to pre-built libraries

The `build.rs` will automatically use stubs if no Matter libraries are found.

## Recommendations

Given the build complexity, I recommend:

### For Development
- Use the stub implementation initially
- Focus on the Rust application logic
- Add full Matter support when needed

### For Production
- Build Matter once on a reference platform
- Commit the libraries to `lib/connectedhomeip/<platform>/`
- All developers use pre-built libraries

This matches the original design where pre-built libraries are committed to git.

## Next Actions

Choose one path:

**Path A: Build Matter Now**
```bash
rm -rf third_party/connectedhomeip
./scripts/download-matter.sh  # Will take 10-15 minutes
make matter-build              # Will take 20-30 minutes
make matter-libs               # Copy to lib/
git add lib/connectedhomeip/
git commit -m "Add pre-built Matter libraries"
```

**Path B: Use Stubs for Now**
```bash
# Just build your Rust app
make build

# Matter functionality will use stubs
# Add real Matter support later
```

**Path C: Get Pre-Built Libraries**
```bash
# Get libraries from teammate/CI
# Place in lib/connectedhomeip/<platform>/
make build  # Will link against pre-built libs
```

All three paths are valid and supported by the build system!
