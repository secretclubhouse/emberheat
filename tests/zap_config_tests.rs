// Test suite for ZAP configuration validation
//
// These tests verify the ZAP configuration file and build system integration
// for real ZAP data support.

use std::fs;
use std::path::Path;

/// Test that the ZAP configuration file exists and is valid JSON
#[test]
fn test_zap_config_file_exists() {
    let zap_file = Path::new("matter/thermostat.zap");
    assert!(
        zap_file.exists(),
        "ZAP configuration file should exist at matter/thermostat.zap"
    );
}

/// Test that the ZAP file is valid JSON
#[test]
fn test_zap_config_is_valid_json() {
    let zap_file = Path::new("matter/thermostat.zap");
    let content =
        fs::read_to_string(zap_file).expect("Should be able to read ZAP configuration file");

    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("ZAP file should be valid JSON");

    // Verify it has the expected top-level structure
    assert!(parsed.is_object(), "ZAP file should be a JSON object");
}

/// Test that the ZAP file contains required Matter thermostat configuration
#[test]
fn test_zap_config_contains_thermostat() {
    let zap_file = Path::new("matter/thermostat.zap");
    let content =
        fs::read_to_string(zap_file).expect("Should be able to read ZAP configuration file");

    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("ZAP file should be valid JSON");

    // Check for endpoint types
    let endpoint_types = parsed
        .get("endpointTypes")
        .expect("ZAP file should have endpointTypes");
    assert!(
        endpoint_types.is_array(),
        "endpointTypes should be an array"
    );

    // Verify thermostat endpoint type exists (device type 769 = 0x0301)
    let has_thermostat = endpoint_types.as_array().unwrap().iter().any(|et| {
        et.get("deviceTypeCode")
            .and_then(|code| code.as_u64())
            .map(|code| code == 769) // 0x0301 = Thermostat device type
            .unwrap_or(false)
    });

    assert!(
        has_thermostat,
        "ZAP file should contain a thermostat endpoint type (device type 0x0301)"
    );
}

/// Test that the ZAP file defines the Thermostat cluster
#[test]
fn test_zap_config_has_thermostat_cluster() {
    let zap_file = Path::new("matter/thermostat.zap");
    let content =
        fs::read_to_string(zap_file).expect("Should be able to read ZAP configuration file");

    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("ZAP file should be valid JSON");

    // Find thermostat endpoint type
    let endpoint_types = parsed
        .get("endpointTypes")
        .and_then(|et| et.as_array())
        .expect("Should have endpointTypes array");

    let thermostat_endpoint = endpoint_types
        .iter()
        .find(|et| {
            et.get("deviceTypeCode")
                .and_then(|code| code.as_u64())
                .map(|code| code == 769)
                .unwrap_or(false)
        })
        .expect("Should have thermostat endpoint type");

    // Check for thermostat cluster (0x0201 = 513)
    let clusters = thermostat_endpoint
        .get("clusters")
        .and_then(|c| c.as_array())
        .expect("Thermostat endpoint should have clusters");

    let has_thermostat_cluster = clusters.iter().any(|cluster| {
        cluster
            .get("code")
            .and_then(|code| code.as_u64())
            .map(|code| code == 513) // 0x0201 = Thermostat cluster
            .unwrap_or(false)
    });

    assert!(
        has_thermostat_cluster,
        "Thermostat endpoint should include Thermostat cluster (0x0201)"
    );
}

/// Test that required thermostat attributes are defined
#[test]
fn test_zap_config_has_required_attributes() {
    let zap_file = Path::new("matter/thermostat.zap");
    let content =
        fs::read_to_string(zap_file).expect("Should be able to read ZAP configuration file");

    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("ZAP file should be valid JSON");

    // Navigate to thermostat cluster
    let endpoint_types = parsed
        .get("endpointTypes")
        .and_then(|et| et.as_array())
        .expect("Should have endpointTypes array");

    let thermostat_endpoint = endpoint_types
        .iter()
        .find(|et| {
            et.get("deviceTypeCode")
                .and_then(|code| code.as_u64())
                .map(|code| code == 769)
                .unwrap_or(false)
        })
        .expect("Should have thermostat endpoint type");

    let clusters = thermostat_endpoint
        .get("clusters")
        .and_then(|c| c.as_array())
        .expect("Thermostat endpoint should have clusters");

    let thermostat_cluster = clusters
        .iter()
        .find(|cluster| {
            cluster
                .get("code")
                .and_then(|code| code.as_u64())
                .map(|code| code == 513)
                .unwrap_or(false)
        })
        .expect("Should have thermostat cluster");

    let attributes = thermostat_cluster
        .get("attributes")
        .and_then(|a| a.as_array())
        .expect("Thermostat cluster should have attributes");

    // Required attributes (by Matter spec)
    let required_attrs = [
        ("LocalTemperature", 0),            // 0x0000
        ("OccupiedHeatingSetpoint", 18),    // 0x0012
        ("SystemMode", 28),                 // 0x001C
        ("ControlSequenceOfOperation", 27), // 0x001B
    ];

    for (name, code) in required_attrs {
        let has_attr = attributes.iter().any(|attr| {
            attr.get("code")
                .and_then(|c| c.as_u64())
                .map(|c| c == code)
                .unwrap_or(false)
        });

        assert!(
            has_attr,
            "Thermostat cluster should have required attribute: {} (0x{:04X})",
            name, code
        );
    }
}

/// Test that the ZAP file defines at least one endpoint
#[test]
fn test_zap_config_has_endpoints() {
    let zap_file = Path::new("matter/thermostat.zap");
    let content =
        fs::read_to_string(zap_file).expect("Should be able to read ZAP configuration file");

    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("ZAP file should be valid JSON");

    let endpoints = parsed
        .get("endpoints")
        .and_then(|e| e.as_array())
        .expect("ZAP file should have endpoints array");

    assert!(
        !endpoints.is_empty(),
        "ZAP file should define at least one endpoint"
    );

    // Verify endpoint 0 (root node) exists
    let has_root_endpoint = endpoints.iter().any(|ep| {
        ep.get("endpointId")
            .and_then(|id| id.as_u64())
            .map(|id| id == 0)
            .unwrap_or(false)
    });

    assert!(
        has_root_endpoint,
        "ZAP file should define endpoint 0 (root node)"
    );

    // Verify at least one thermostat endpoint (ID >= 1)
    let has_thermostat_endpoint = endpoints.iter().any(|ep| {
        ep.get("endpointId")
            .and_then(|id| id.as_u64())
            .map(|id| id >= 1)
            .unwrap_or(false)
    });

    assert!(
        has_thermostat_endpoint,
        "ZAP file should define at least one thermostat endpoint (ID >= 1)"
    );
}

/// Test build type environment variable detection
#[test]
fn test_matter_build_type_env_var() {
    // This is set by build.rs based on configuration
    let build_type = env!("MATTER_BUILD_TYPE");

    // Should be one of: "stub", "full-stub-zap", "full-with-zap"
    assert!(
        build_type == "stub" || build_type == "full-stub-zap" || build_type == "full-with-zap",
        "MATTER_BUILD_TYPE should be a valid build type, got: {}",
        build_type
    );
}

/// Test that USE_REAL_ZAP_DATA cfg is correctly set (or not set)
#[test]
fn test_zap_data_cfg_flag() {
    // This test verifies the cfg flag is either set or not set
    // It doesn't require it to be set, just that the build system works

    #[cfg(use_real_zap_data)]
    {
        // If real ZAP data is enabled, verify we're in the right build mode
        assert_eq!(
            env!("MATTER_BUILD_TYPE"),
            "full-with-zap",
            "When use_real_zap_data is set, MATTER_BUILD_TYPE should be 'full-with-zap'"
        );
    }

    #[cfg(not(use_real_zap_data))]
    {
        // If real ZAP data is not enabled, we should be in stub or full-stub-zap mode
        let build_type = env!("MATTER_BUILD_TYPE");
        assert!(
            build_type == "stub" || build_type == "full-stub-zap",
            "When use_real_zap_data is not set, MATTER_BUILD_TYPE should be 'stub' or 'full-stub-zap', got: {}",
            build_type
        );
    }
}

/// Test README documentation exists
#[test]
fn test_zap_documentation_exists() {
    // Verify key documentation files exist
    assert!(
        Path::new("matter/README.md").exists(),
        "matter/README.md should exist"
    );

    assert!(
        Path::new("docs/ZAP_INTEGRATION.md").exists(),
        "docs/ZAP_INTEGRATION.md should exist"
    );
}

/// Test that ZAP documentation mentions the Docker build workflow
#[test]
fn test_zap_docs_mention_docker_build() {
    let docs = fs::read_to_string("docs/ZAP_INTEGRATION.md")
        .expect("Should be able to read ZAP_INTEGRATION.md");

    assert!(
        docs.contains("make matter-build-docker"),
        "ZAP documentation should mention Docker build workflow"
    );

    assert!(
        docs.contains("Quick Start"),
        "ZAP documentation should have a Quick Start section"
    );
}

/// Test that matter README mentions automatic ZAP generation
#[test]
fn test_matter_readme_mentions_auto_generation() {
    let readme =
        fs::read_to_string("matter/README.md").expect("Should be able to read matter/README.md");

    assert!(
        readme.contains("Automatic Generation"),
        "matter/README.md should mention automatic ZAP generation"
    );

    assert!(
        readme.contains("matter-build-docker"),
        "matter/README.md should reference the Docker build command"
    );
}
