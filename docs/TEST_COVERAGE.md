# Test Coverage for ZAP Integration

This document describes the test coverage for the ZAP data integration features.

## Test Suites

### 1. ZAP Configuration Tests (`tests/zap_config_tests.rs`)

**Purpose**: Validate the ZAP configuration file and ensure it meets Matter specification requirements.

Tests (11 total):
- `test_zap_config_file_exists` - Verifies `matter/thermostat.zap` exists
- `test_zap_config_is_valid_json` - Validates JSON syntax
- `test_zap_config_contains_thermostat` - Verifies thermostat device type (0x0301) is defined
- `test_zap_config_has_thermostat_cluster` - Verifies Thermostat cluster (0x0201) is included
- `test_zap_config_has_required_attributes` - Validates required attributes:
  - LocalTemperature (0x0000)
  - OccupiedHeatingSetpoint (0x0012)
  - SystemMode (0x001C)
  - ControlSequenceOfOperation (0x001B)
- `test_zap_config_has_endpoints` - Verifies endpoint configuration (root + thermostats)
- `test_matter_build_type_env_var` - Validates MATTER_BUILD_TYPE environment variable
- `test_zap_data_cfg_flag` - Verifies `use_real_zap_data` cfg flag consistency
- `test_zap_documentation_exists` - Checks for required documentation files
- `test_zap_docs_mention_docker_build` - Validates documentation content
- `test_matter_readme_mentions_auto_generation` - Verifies README mentions Docker workflow

**Coverage**:
- ✅ ZAP file validation
- ✅ Matter specification compliance
- ✅ Build configuration flags
- ✅ Documentation completeness

### 2. Build System Tests (`tests/build_system_tests.rs`)

**Purpose**: Verify the build system correctly handles ZAP data integration.

Tests (11 total):
- `test_build_script_declares_custom_cfgs` - Verifies build.rs declares all cfg attributes
- `test_build_script_checks_zap_env_var` - Validates USE_REAL_ZAP_DATA handling
- `test_build_script_verifies_zap_files` - Checks for gen_config.h/endpoint_config.h verification
- `test_matter_wrapper_has_zap_conditional` - Validates C++ conditional compilation
- `test_docker_build_includes_zap_generation` - Verifies Docker build ZAP generation step
- `test_gitignore_allows_zap_files` - Validates .gitignore configuration
- `test_makefile_has_zap_target` - Checks for `zap-generate` target
- `test_matter_build_type_is_set` - Validates MATTER_BUILD_TYPE is set correctly
- `test_build_type_cfg_consistency` - Verifies build type matches cfg flags
- `test_docker_script_handles_missing_zap_file` - Tests error handling
- `test_documentation_mentions_key_files` - Validates documentation references

**Coverage**:
- ✅ Build script logic (build.rs)
- ✅ Docker build integration
- ✅ Makefile targets
- ✅ Git configuration
- ✅ C++ FFI conditional compilation
- ✅ Error handling

### 3. Matter Tests (`emberheat-matter/`)

**Purpose**: Unit tests for Matter protocol types and cluster definitions.

Tests covering:
- Temperature conversion (Celsius ↔ centi-degrees)
- Matter constants (cluster IDs, device types)
- System mode mapping
- Running state flags
- Zone thermostat state management

**Coverage**:
- ✅ Matter protocol constants
- ✅ Temperature conversions
- ✅ State management
- ✅ Atomic operations

### 4. Mock Integration Tests (`emberheat-linux/tests/mock_integration_tests.rs`)

**Purpose**: End-to-end testing without hardware dependencies.

Tests covering (22 total):
- Hysteresis control logic
- Multi-zone coordination
- GPIO relay control
- Sensor error recovery
- State persistence
- Timing requirements
- Matter bidirectional callbacks
- Configuration validation
- Metrics reporting

**Coverage**:
- ✅ Control loop logic
- ✅ Hardware abstraction
- ✅ Multi-zone scenarios
- ✅ Error recovery

## Test Execution

### Run All Tests

```bash
# Run all tests (including unit and integration)
cargo test

# Run specific test suites
cargo test --test zap_config_tests
cargo test --test build_system_tests
cargo test --test mock_integration_tests
```

### Run with Coverage

```bash
# Generate coverage report (requires tarpaulin)
cargo tarpaulin --out Html --output-dir coverage/

# View coverage report
open coverage/index.html
```

## Coverage Summary

| Area | Test Suite | Tests | Coverage |
|------|------------|-------|----------|
| ZAP Configuration | `zap_config_tests.rs` | 11 | Full |
| Build System | `build_system_tests.rs` | 11 | Full |
| Matter Protocol | `emberheat-matter/` | 20+ | High |
| Integration | `emberheat-linux/tests/mock_integration_tests.rs` | 22 | High |
| **Total** | | **64+** | **High** |

## What's Tested

### Configuration Files
- ✅ `matter/thermostat.zap` - JSON validity, structure, Matter spec compliance
- ✅ `.gitignore` - ZAP file tracking configuration
- ✅ `Makefile` - ZAP generation targets
- ✅ Documentation - Completeness and accuracy

### Build System
- ✅ Environment variable handling (`USE_REAL_ZAP_DATA`)
- ✅ Conditional compilation (`#[cfg(use_real_zap_data)]`)
- ✅ Build type determination (`MATTER_BUILD_TYPE`)
- ✅ ZAP file verification
- ✅ Cfg flag declarations

### C++ Integration
- ✅ Conditional compilation (`#ifdef USE_REAL_ZAP_DATA`)
- ✅ CodegenDataModelProvider inclusion
- ✅ ZAP header inclusion

### Docker Build
- ✅ ZAP generation step inclusion
- ✅ Project ZAP file detection
- ✅ Error handling for missing files
- ✅ Generated file copying

### Documentation
- ✅ Presence of required files
- ✅ Key workflow documentation
- ✅ Docker build instructions
- ✅ Environment variable documentation

## What's NOT Tested (Requires Manual Testing)

### Runtime Behavior
- ❌ Actual ZAP code generation (requires Node.js/Java)
- ❌ Real Matter controller interaction (requires HomeKit/Google Home)
- ❌ Full Docker build execution (requires ~30min build)
- ❌ Cross-platform library builds (requires multiple platforms)

### Rationale
These aspects require:
- External dependencies (Node.js, Java, ZAP tool)
- Long build times (Matter SDK compilation)
- Physical hardware (controllers, Raspberry Pi)
- Multiple platforms (Linux/macOS/ARM64/x64)

They are tested through:
1. Manual testing during development
2. CI/CD pipeline builds (when available)
3. Integration testing on target hardware

## Continuous Integration

To integrate these tests into CI/CD:

```yaml
# .github/workflows/test.yml example
- name: Run ZAP configuration tests
  run: cargo test --test zap_config_tests

- name: Run build system tests
  run: cargo test --test build_system_tests

- name: Run all mock tests
  run: cargo test --test mock_integration_tests

- name: Generate coverage
  run: cargo tarpaulin --out Xml

- name: Upload coverage
  uses: codecov/codecov-action@v3
```

## Test Maintenance

When making changes to ZAP integration:

1. **Update tests if you modify**:
   - `matter/thermostat.zap` → Update `zap_config_tests.rs`
   - Build configuration → Update `build_system_tests.rs`
   - Documentation → Update documentation tests

2. **Run all tests before committing**:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```

3. **Verify tests still pass** with both modes:
   ```bash
   # Test with stub mode (default)
   cargo test

   # Test with real ZAP (if generated files exist)
   USE_REAL_ZAP_DATA=1 cargo test
   ```

## Future Test Additions

Potential areas for additional coverage:

1. **ZAP Generation Integration**
   - Script to run ZAP generator in test environment
   - Validate generated files match expected structure
   - Compare stub vs. real generated files

2. **Multi-Platform Tests**
   - Cross-compilation tests
   - Platform-specific library detection
   - Architecture-specific path handling

3. **Error Scenario Tests**
   - Missing ZAP files
   - Corrupted ZAP JSON
   - Incompatible Matter SDK versions
   - Missing dependencies (Node.js, Java)

4. **Performance Tests**
   - ZAP generation time benchmarks
   - Build time comparisons (stub vs. real)
   - Docker build performance

## References

- [Rust Testing Guide](https://doc.rust-lang.org/book/ch11-00-testing.html)
- [Cargo Test Documentation](https://doc.rust-lang.org/cargo/commands/cargo-test.html)
- [Matter ZAP Tool](https://github.com/project-chip/zap)
- [Project Test Strategy](../TESTING.md)
