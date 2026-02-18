# GitHub Actions Workflows

This directory contains the CI/CD workflows for the EmberHeat project.

## Workflows

### 1. CI (`ci.yml`)

**Trigger**: Push to `main`/`develop` branches, Pull Requests

**Purpose**: Continuous Integration testing across multiple platforms

**Jobs**:
- **test**: Runs on Ubuntu and macOS with stable and nightly Rust
  - Format checking (`cargo fmt`)
  - Linting (`cargo clippy`)
  - Mock tests (cross-platform)
  - Full tests (Linux only, with hardware)
  - Caching for faster builds

- **security_audit**: Security vulnerability scanning
  - Uses `cargo-audit` to check for known vulnerabilities
  - Runs on Ubuntu only

- **coverage**: Code coverage reporting
  - Uses `cargo-tarpaulin` to generate coverage
  - Uploads to Codecov
  - Runs on Ubuntu only

**Matrix Testing**:
```yaml
os: [ubuntu-latest, macos-latest]
rust: [stable, nightly]
```

### 2. Release (`release.yml`)

**Trigger**: Git tags matching `v*.*.*` (e.g., `v1.0.0`) or manual dispatch

**Purpose**: Build and publish release binaries for multiple architectures

**Jobs**:
- **create-release**: Creates GitHub release
  - Extracts version from tag
  - Generates release notes
  - Creates release draft

- **build**: Builds binaries for multiple targets
  - `armv7-unknown-linux-gnueabihf` - Raspberry Pi 2/3/4 (32-bit)
  - `aarch64-unknown-linux-gnu` - Raspberry Pi 3/4 (64-bit)
  - `x86_64-unknown-linux-gnu` - x86_64 Linux
  - Uses `cross` for cross-compilation
  - Strips binaries for smaller size
  - Creates `.tar.gz` archives
  - Generates SHA256 checksums
  - Uploads to GitHub release

- **docker**: Builds multi-architecture Docker images
  - Platforms: `linux/amd64`, `linux/arm64`, `linux/arm/v7`
  - Pushes to GitHub Container Registry
  - Tags: version, major.minor, major, latest
  - Uses layer caching for faster builds

**Artifacts**:
- `emberheat-armv7-unknown-linux-gnueabihf.tar.gz` (+ `.sha256`)
- `emberheat-aarch64-unknown-linux-gnu.tar.gz` (+ `.sha256`)
- `emberheat-x86_64-unknown-linux-gnu.tar.gz` (+ `.sha256`)
- Docker images: `ghcr.io/OWNER/emberheat:VERSION`

### 3. Dependencies (`dependencies.yml`)

**Trigger**: Weekly schedule (Mondays at 9 AM UTC) or manual dispatch

**Purpose**: Automated dependency updates

**Jobs**:
- **update-dependencies**: Updates Cargo dependencies
  - Runs `cargo update`
  - Runs mock tests to verify
  - Creates Pull Request with updates
  - Auto-labels with `dependencies`

## Usage

### Running CI Locally

You can test the workflows locally before pushing:

```bash
# Install act (GitHub Actions local runner)
brew install act  # macOS
# or
curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash

# Run CI workflow
act push

# Run specific job
act -j test

# Run with different event
act pull_request
```

### Creating a Release

To create a new release:

1. **Tag the release**:
   ```bash
   git tag -a v1.0.0 -m "Release v1.0.0"
   git push origin v1.0.0
   ```

2. **Or use GitHub UI**:
   - Go to Releases → Draft a new release
   - Create a new tag (e.g., `v1.0.0`)
   - Publish release

3. **Or manually trigger**:
   - Go to Actions → Release → Run workflow
   - Enter tag name (e.g., `v1.0.0`)

The workflow will:
- Build binaries for all platforms
- Create release with binaries
- Build and push Docker images

### Manual Workflow Dispatch

Some workflows support manual triggering:

1. Go to **Actions** tab
2. Select the workflow
3. Click **Run workflow**
4. Fill in required inputs
5. Click **Run workflow**

## Secrets Required

The workflows use the following secrets:

| Secret | Required For | Description |
|--------|-------------|-------------|
| `GITHUB_TOKEN` | All workflows | Auto-provided by GitHub Actions |

No additional secrets needed! All authentication uses the built-in `GITHUB_TOKEN`.

## Caching

The workflows use caching to speed up builds:

- **Cargo registry**: `~/.cargo/registry`
- **Cargo git index**: `~/.cargo/git`
- **Build artifacts**: `target/`
- **Docker layers**: GitHub Actions cache

Caches are scoped by:
- OS (`runner.os`)
- Rust version (`matrix.rust`)
- Target architecture (`matrix.target`)
- `Cargo.lock` hash

## Cross-Compilation

For ARM targets, we use [`cross`](https://github.com/cross-rs/cross):

```yaml
- name: Install cross-compilation tools
  run: cargo install cross --git https://github.com/cross-rs/cross

- name: Build release binary
  run: cross build --release --target armv7-unknown-linux-gnueabihf
```

`cross` uses Docker containers with the necessary cross-compilation toolchains.

## Platform Support

### Tested Platforms (CI)
- ✅ Ubuntu (latest)
- ✅ macOS (latest)
- ✅ Rust stable
- ✅ Rust nightly

### Release Targets
- ✅ Raspberry Pi 2/3/4 (32-bit ARM)
- ✅ Raspberry Pi 3/4 (64-bit ARM)
- ✅ x86_64 Linux
- ✅ Docker (multi-arch)

## Workflow Files

```
.github/workflows/
├── ci.yml              # Main CI pipeline
├── release.yml         # Release builds
├── dependencies.yml    # Dependency updates
└── README.md          # This file
```

## Best Practices

### 1. Testing Before Release

Always ensure CI passes before creating a release:
```bash
# Check locally
make test
make lint
make fmt

# Push and verify CI
git push
# Wait for CI to complete
```

### 2. Semantic Versioning

Use semantic versioning for tags:
- `v1.0.0` - Major release
- `v1.1.0` - Minor release (new features)
- `v1.1.1` - Patch release (bug fixes)

### 3. Release Notes

The release workflow generates basic notes, but you should edit them:
- List major changes
- Mention breaking changes
- Include upgrade instructions
- Link to documentation

### 4. Security

- Review security audit results regularly
- Update dependencies promptly
- Monitor Dependabot alerts

## Troubleshooting

### Build Failures

**Problem**: Cross-compilation fails
```
Solution: Check that the target is supported by cross
- Update cross: cargo install cross --git https://github.com/cross-rs/cross
- Check cross documentation for platform support
```

**Problem**: Tests fail on CI but pass locally
```
Solution: Platform-specific issue
- Check if tests are Linux-only
- Review test logs for platform differences
- Use make test-mock for cross-platform tests
```

### Release Issues

**Problem**: Release asset upload fails
```
Solution: Check GitHub token permissions
- Ensure GITHUB_TOKEN has write access
- Verify release is created before uploading assets
```

**Problem**: Docker build fails
```
Solution: Check Dockerfile and dependencies
- Verify Dockerfile builds locally
- Check for platform-specific dependencies
- Review Docker buildx setup
```

### Cache Issues

**Problem**: Builds are slow despite caching
```
Solution: Clear and rebuild cache
- Change cache key in workflow
- Or manually clear cache in GitHub Actions settings
```

## Monitoring

### CI Status Badge

Add to README.md:
```markdown
[![CI](https://github.com/OWNER/emberheat/workflows/CI/badge.svg)](https://github.com/OWNER/emberheat/actions?query=workflow%3ACI)
```

### Release Status

Check latest release:
```bash
gh release view --json tagName,publishedAt,assets
```

### View Workflow Runs

```bash
# List recent runs
gh run list --limit 10

# View specific run
gh run view RUN_ID

# Watch run in progress
gh run watch
```

## Resources

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [cross Documentation](https://github.com/cross-rs/cross)
- [cargo-tarpaulin](https://github.com/xd009642/tarpaulin)
- [Rust CI/CD Best Practices](https://doc.rust-lang.org/cargo/guide/continuous-integration.html)

## Contributing

When adding new workflows:

1. Test locally with `act` if possible
2. Use caching for frequently accessed resources
3. Add clear job/step names
4. Document any required secrets
5. Update this README
