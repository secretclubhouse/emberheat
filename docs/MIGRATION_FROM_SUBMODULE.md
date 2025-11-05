# Migration Guide: Submodule to Download-Based Build

This guide helps existing developers migrate from the git submodule approach to the new download-based build system.

## What Changed?

**Before:**
- Matter source was a git submodule at `third_party/connectedhomeip/`
- Required: `git submodule update --init --recursive`
- Git tracked the entire Matter repository history
- Nested submodules caused issues

**After:**
- Matter source is downloaded from GitHub releases
- No git submodules
- Cleaner, more reliable builds
- Pre-built libraries committed to git

## Migration Steps

### For Developers with Existing Clone

If you already have the repository cloned with the submodule:

```bash
# 1. Pull the latest changes
git pull

# 2. Remove the old submodule
git submodule deinit -f third_party/connectedhomeip
git rm -f third_party/connectedhomeip
rm -rf .git/modules/third_party/connectedhomeip

# 3. Pull changes from .gitmodules, .gitignore, Makefile
git pull

# 4. If you had local Matter builds, back them up (optional)
# The pre-built libraries may already be in lib/connectedhomeip/

# 5. Clean up
rm -rf third_party/connectedhomeip

# Done! Now use the new workflow
make build  # Uses pre-built libraries
```

### Fresh Clone (New Developers)

For new developers cloning after this change:

```bash
# 1. Clone the repository
git clone <repo-url>
cd thermos-rust

# 2. Build immediately (uses pre-built libraries)
make build

# That's it! No submodule init needed
```

## New Workflow

### Building the Application

```bash
# Just build - uses pre-built libraries
make build

# Or for release
make build-release
```

### Working with Matter Libraries

```bash
# Download Matter source
make matter-download

# Build Matter libraries (15-30 minutes)
make matter-build

# Copy to lib/ directory
make matter-libs

# Or do all three at once
make matter-setup

# Clean up source (keeps pre-built libs)
make matter-clean
```

## Common Questions

### Do I need to build Matter myself?

**No!** The repository includes pre-built Matter libraries in `lib/connectedhomeip/<platform>/`. Just run:

```bash
make build
```

### When should I rebuild Matter?

Rebuild Matter only if:
- You need a different Matter version
- Your platform doesn't have pre-built libraries
- You're customizing the Matter build configuration
- You're the first on your team to build for a new platform

### What if I encounter "ARM GCC package error"?

This was an issue with the old submodule approach. The new scripts work around this automatically. You shouldn't see this error anymore.

### Can I use a specific Matter version?

Yes! Specify the version when downloading:

```bash
MATTER_VERSION=v1.3.0.0 make matter-download
make matter-build
make matter-libs
```

### What happened to the .gitmodules file?

It's been updated to remove the Matter submodule. Other submodules (if any) remain unchanged.

## Rollback (If Needed)

If you need to go back to the submodule approach:

```bash
# Checkout the previous commit
git checkout <commit-before-migration>

# Re-initialize submodule
git submodule update --init --recursive
```

## Benefits of New Approach

1. **Faster clones**: No large submodule history
2. **Easier updates**: Just change version number
3. **Better CI**: Pre-built libraries = faster CI
4. **Less fragile**: No nested submodule issues
5. **Clearer**: Explicit version instead of commit hash

## Updating Pre-Built Libraries

If you build Matter for a platform:

```bash
# Build for your platform
make matter-setup

# Check what was created
ls -lh lib/connectedhomeip/$(uname -s | tr '[:upper:]' '[:lower:]')-*/

# Commit the libraries
git add lib/connectedhomeip/
git commit -m "Add/update Matter libraries for $(uname -s)"
git push
```

Now your team members can use these pre-built libraries!

## File Changes Summary

| File | Change |
|------|--------|
| `.gitmodules` | Removed Matter submodule |
| `.gitignore` | Added `third_party/connectedhomeip/` (downloaded) |
| `Makefile` | Added `matter-*` targets |
| `scripts/download-matter.sh` | New: Downloads Matter source |
| `scripts/build-matter.sh` | New: Builds Matter libraries |
| `scripts/copy-matter-libs.sh` | Updated: Works with new layout |
| `third_party/connectedhomeip/` | Changed from submodule to download |
| `lib/connectedhomeip/` | Same: Pre-built libraries |
| `docs/BUILDING_MATTER.md` | New: Build documentation |

## Troubleshooting

### "third_party/connectedhomeip already exists"

```bash
rm -rf third_party/connectedhomeip
make matter-download
```

### "Could not find Matter libraries"

Option 1 - Use pre-built (fast):
```bash
git pull  # Get latest pre-built libraries
make build
```

Option 2 - Build yourself (slow):
```bash
make matter-setup
make build
```

### Build script fails

Check the detailed guide:
```bash
cat docs/BUILDING_MATTER.md
```

Or ask for help in the project chat/issues.

## Need Help?

- Read: [docs/BUILDING_MATTER.md](BUILDING_MATTER.md)
- Check: `make help` for available commands
- Ask: Open an issue or contact the team
