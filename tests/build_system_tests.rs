// Build system tests for ZAP integration
//
// These tests verify the build system correctly handles ZAP data configuration

use std::env;
use std::fs;

/// Test that build.rs declares all custom cfg attributes
#[test]
fn test_build_script_declares_custom_cfgs() {
    let build_rs = fs::read_to_string("build.rs").expect("Should be able to read build.rs");

    // Verify that build.rs declares the custom cfg attributes
    assert!(
        build_rs.contains("cargo::rustc-check-cfg=cfg(use_real_zap_data)"),
        "build.rs should declare use_real_zap_data cfg"
    );

    assert!(
        build_rs.contains("cargo::rustc-check-cfg=cfg(matter_sdk_available)"),
        "build.rs should declare matter_sdk_available cfg"
    );

    assert!(
        build_rs.contains("cargo::rustc-check-cfg=cfg(matter_sdk_stub)"),
        "build.rs should declare matter_sdk_stub cfg"
    );
}

/// Test that build.rs checks for ZAP data environment variable
#[test]
fn test_build_script_checks_zap_env_var() {
    let build_rs = fs::read_to_string("build.rs").expect("Should be able to read build.rs");

    assert!(
        build_rs.contains("USE_REAL_ZAP_DATA"),
        "build.rs should check USE_REAL_ZAP_DATA environment variable"
    );

    assert!(
        build_rs.contains("cargo:rerun-if-env-changed=USE_REAL_ZAP_DATA"),
        "build.rs should rerun when USE_REAL_ZAP_DATA changes"
    );
}

/// Test that build.rs verifies ZAP file existence
#[test]
fn test_build_script_verifies_zap_files() {
    let build_rs = fs::read_to_string("build.rs").expect("Should be able to read build.rs");

    assert!(
        build_rs.contains("gen_config.h"),
        "build.rs should check for gen_config.h"
    );

    assert!(
        build_rs.contains("endpoint_config.h"),
        "build.rs should check for endpoint_config.h"
    );

    assert!(
        build_rs.contains("zap-generated"),
        "build.rs should reference zap-generated directory"
    );
}

/// Test that matter_wrapper.cpp includes ZAP conditional compilation
#[test]
fn test_matter_wrapper_has_zap_conditional() {
    let wrapper_cpp = fs::read_to_string("src/matter/ffi/matter_wrapper.cpp")
        .expect("Should be able to read matter_wrapper.cpp");

    assert!(
        wrapper_cpp.contains("#ifdef USE_REAL_ZAP_DATA"),
        "matter_wrapper.cpp should have USE_REAL_ZAP_DATA conditional"
    );

    assert!(
        wrapper_cpp.contains("CodegenDataModelProvider"),
        "matter_wrapper.cpp should reference CodegenDataModelProvider for real ZAP data"
    );

    assert!(
        wrapper_cpp.contains("#include <zap-generated/gen_config.h>"),
        "matter_wrapper.cpp should include ZAP-generated headers"
    );
}

/// Test that Docker build script includes ZAP generation
#[test]
fn test_docker_build_includes_zap_generation() {
    let docker_script = fs::read_to_string("scripts/docker-build-matter-internal.sh")
        .expect("Should be able to read docker build script");

    assert!(
        docker_script.contains("Step 5.5: Generating ZAP data"),
        "Docker build should have ZAP generation step"
    );

    assert!(
        docker_script.contains("matter/thermostat.zap"),
        "Docker build should reference the project ZAP file"
    );

    assert!(
        docker_script.contains("scripts/tools/zap/generate.py"),
        "Docker build should use ZAP generation script"
    );
}

/// Test that .gitignore allows tracking ZAP generated files
#[test]
fn test_gitignore_allows_zap_files() {
    let gitignore = fs::read_to_string(".gitignore").expect("Should be able to read .gitignore");

    assert!(
        gitignore.contains("!/lib/connectedhomeip/**/zap-generated/"),
        ".gitignore should allow tracking ZAP directory"
    );

    assert!(
        gitignore.contains("!/lib/connectedhomeip/**/zap-generated/*.h"),
        ".gitignore should allow tracking ZAP header files"
    );

    // But should still ignore temporary generation directories
    assert!(
        gitignore.contains("zzz_generated/"),
        ".gitignore should ignore temporary ZAP generation directory"
    );
}

/// Test that Makefile includes ZAP generation target
#[test]
fn test_makefile_has_zap_target() {
    let makefile = fs::read_to_string("Makefile").expect("Should be able to read Makefile");

    assert!(
        makefile.contains("zap-generate:"),
        "Makefile should have zap-generate target"
    );

    assert!(
        makefile.contains("scripts/tools/zap/generate.py"),
        "Makefile zap-generate should use ZAP tool"
    );

    assert!(
        makefile.contains("matter/thermostat.zap"),
        "Makefile should reference the ZAP configuration file"
    );
}

/// Test that build system provides MATTER_BUILD_TYPE
#[test]
fn test_matter_build_type_is_set() {
    // This environment variable is set by build.rs
    let build_type = env!("MATTER_BUILD_TYPE");

    // Should be a non-empty string
    assert!(!build_type.is_empty(), "MATTER_BUILD_TYPE should be set");

    // Should be one of the valid values
    let valid_types = ["stub", "full-stub-zap", "full-with-zap"];
    assert!(
        valid_types.contains(&build_type),
        "MATTER_BUILD_TYPE should be one of: {:?}, got: {}",
        valid_types,
        build_type
    );
}

/// Test consistency between build type and cfg flags
#[test]
fn test_build_type_cfg_consistency() {
    let build_type = env!("MATTER_BUILD_TYPE");

    #[cfg(use_real_zap_data)]
    {
        // If use_real_zap_data is set, build type must be full-with-zap
        assert_eq!(
            build_type, "full-with-zap",
            "When use_real_zap_data cfg is set, build type should be 'full-with-zap'"
        );
    }

    #[cfg(all(matter_sdk_stub, not(use_real_zap_data)))]
    {
        // If using stub SDK, build type should be stub
        assert_eq!(
            build_type, "stub",
            "When using stub SDK, build type should be 'stub'"
        );
    }

    #[cfg(all(matter_sdk_available, not(use_real_zap_data)))]
    {
        // If SDK available but not using real ZAP, should be full-stub-zap
        assert_eq!(
            build_type, "full-stub-zap",
            "When SDK available but not using real ZAP, build type should be 'full-stub-zap'"
        );
    }
}

/// Test that scripts have proper error handling for missing ZAP files
#[test]
fn test_docker_script_handles_missing_zap_file() {
    let docker_script = fs::read_to_string("scripts/docker-build-matter-internal.sh")
        .expect("Should be able to read docker build script");

    assert!(
        docker_script.contains("if [ -f \"$REPO_ZAP_FILE\" ]"),
        "Docker script should check if ZAP file exists"
    );

    assert!(
        docker_script.contains("No project ZAP file found")
            || docker_script.contains("Skipping ZAP generation"),
        "Docker script should handle missing ZAP file gracefully"
    );
}

/// Test that documentation is kept in sync
#[test]
fn test_documentation_mentions_key_files() {
    let zap_integration_doc = fs::read_to_string("docs/ZAP_INTEGRATION.md")
        .expect("Should be able to read ZAP_INTEGRATION.md");

    // Should mention all the key components
    assert!(
        zap_integration_doc.contains("matter/thermostat.zap"),
        "Documentation should mention the ZAP file location"
    );

    assert!(
        zap_integration_doc.contains("USE_REAL_ZAP_DATA"),
        "Documentation should mention the environment variable"
    );

    assert!(
        zap_integration_doc.contains("lib/connectedhomeip"),
        "Documentation should mention where generated files are stored"
    );
}
