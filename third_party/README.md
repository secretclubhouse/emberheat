# Third-Party Dependencies

This directory contains third-party dependencies for the Thermos-Rust project.

## Matter/Connected Home IP

The Matter (Connected Home IP) library is downloaded from GitHub releases rather than using git submodules. This approach:

- Avoids submodule complexity and nested submodule issues
- Makes builds more reliable and reproducible
- Allows you to use specific stable releases
- Reduces repository size by not tracking the full Matter git history

### Setup

To set up the Matter library:

```bash
# Download, build, and copy libraries in one command
make matter-setup

# Or run steps individually:
make matter-download    # Download Matter source from GitHub
make matter-build       # Build Matter libraries
make matter-libs        # Copy libraries to lib/ directory
```

### Customizing the Version

By default, the download script uses Matter v1.4.0.0. To use a different version:

```bash
MATTER_VERSION=v1.3.0.0 make matter-download
```

### Directory Structure

```
third_party/
  connectedhomeip/          # Downloaded source (not in git)
    out/host/lib/           # Built libraries
    ...

lib/connectedhomeip/        # Pre-built libraries (committed to git)
  darwin-arm64/             # macOS Apple Silicon
  darwin-x64/               # macOS Intel
  linux-arm64/              # Linux ARM64 (Raspberry Pi)
  linux-x64/                # Linux x86_64
```

### Cleaning Up

To remove the downloaded source (keeps pre-built libraries):

```bash
make matter-clean
```

This will remove `third_party/connectedhomeip/` but keep the pre-built libraries in `lib/connectedhomeip/`.

### Build Process Integration

The Rust build script ([build.rs](/workspace/build.rs)) automatically:

1. Checks for pre-built libraries in `lib/connectedhomeip/<platform>/`
2. Falls back to libraries in `third_party/connectedhomeip/out/host/lib/` if available
3. Uses stub implementation if no libraries are found

This means developers can:
- Use pre-built libraries (fastest, no build needed)
- Build from source if customization is needed
- Develop without Matter libraries using stubs
