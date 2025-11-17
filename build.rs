//! Build script for compiling Matter C wrapper and linking Connected Home IP library
//!
//! This script:
//! 1. Compiles the Matter C wrapper (matter_wrapper.cpp)
//! 2. Links against the Connected Home IP library (when available)
//! 3. Generates Rust bindings using bindgen

#![allow(unexpected_cfgs)]

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Detect the current platform and return a string like "darwin-arm64" or "linux-x64"
fn get_platform_string() -> String {
    let os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());

    // Normalize architecture names
    let arch_normalized = match arch.as_str() {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };

    format!("{}-{}", os, arch_normalized)
}

/// Get GLib include paths from pkg-config
/// Returns a Vec of include paths needed for GLib/GIO headers
fn get_glib_includes() -> Vec<PathBuf> {
    let mut includes = Vec::new();

    // Try to get GLib include paths from pkg-config
    if let Ok(output) = Command::new("pkg-config")
        .args(["--cflags", "glib-2.0", "gio-2.0"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for flag in stdout.split_whitespace() {
                if let Some(path) = flag.strip_prefix("-I") {
                    includes.push(PathBuf::from(path));
                }
            }
        }
    }

    // Fallback: common GLib include paths if pkg-config fails
    if includes.is_empty() {
        eprintln!("WARNING: pkg-config failed to find GLib includes, using fallback paths");
        includes.push(PathBuf::from("/usr/include/glib-2.0"));

        // Try to detect the architecture-specific glib include path
        let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());
        let multiarch = match arch.as_str() {
            "aarch64" => "aarch64-linux-gnu",
            "x86_64" => "x86_64-linux-gnu",
            "arm" => "arm-linux-gnueabihf",
            _ => "unknown-linux-gnu",
        };
        includes.push(PathBuf::from(format!(
            "/usr/lib/{}/glib-2.0/include",
            multiarch
        )));
    }

    includes
}

/// Check if a directory contains the required Matter libraries
/// These library names match the actual output from Matter's build system:
/// - libCHIP.a: Main Matter library (from src/lib:lib target)
/// - libCHIPAppServer.a: App server (from src/app/server:server target)
/// - libChipController.a: Controller (from src/controller:controller target)
/// - libSetupPayload.a: Setup payload (from src/setup_payload:setup_payload target)
fn has_required_libraries(lib_dir: &Path) -> bool {
    // At minimum, we need the main CHIP library
    // The other libraries are linked as dependencies within libCHIP.a when built with complete_static_lib
    let required_libs = ["libCHIP.a"];

    required_libs.iter().all(|lib| lib_dir.join(lib).exists())
}

fn main() {
    let _out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=src/matter/ffi/matter_wrapper.h");
    println!("cargo:rerun-if-changed=src/matter/ffi/matter_wrapper.cpp");

    // =========================================================================
    // Step 1: Check for Connected Home IP library
    // =========================================================================

    // Check for pre-built libraries first (preferred for cross-compilation)
    let platform = get_platform_string();
    let prebuilt_lib_dir = PathBuf::from("lib/connectedhomeip").join(&platform);
    let has_prebuilt = prebuilt_lib_dir.exists() && has_required_libraries(&prebuilt_lib_dir);

    // Check for submodule source code (needed for headers even with pre-built libs)
    let chip_root = PathBuf::from("third_party/connectedhomeip");
    let has_submodule = chip_root.exists() && chip_root.join("src").exists();

    // SDK is available if we have BOTH headers AND libraries
    // Pre-built libraries still require submodule headers for compilation
    let chip_available = has_submodule && (has_prebuilt || chip_root.join("out/host/lib").exists());

    if !chip_available {
        if has_prebuilt && !has_submodule {
            eprintln!("WARNING: Pre-built Matter libraries found but headers are missing.");
            eprintln!("         The Matter C++ wrapper requires headers from the submodule.");
            eprintln!("         To enable full Matter support:");
            eprintln!("           git submodule update --init --recursive");
            eprintln!();
        } else if !has_submodule {
            eprintln!("WARNING: Connected Home IP library not found.");
            eprintln!("         The Matter integration will use stub implementation.");
            eprintln!("         To enable full Matter support:");
            eprintln!("           git submodule update --init --recursive");
            eprintln!();
        } else {
            eprintln!("WARNING: Matter submodule found but no libraries available.");
            eprintln!(
                "         Pre-built libraries are at lib/connectedhomeip/{}",
                platform
            );
            eprintln!("         Or build from source with: cd third_party/connectedhomeip && gn gen out/host && ninja -C out/host");
            eprintln!();
        }
    } else if has_prebuilt {
        eprintln!(
            "INFO: Using pre-built Matter libraries from lib/connectedhomeip/{}",
            platform
        );
        eprintln!("INFO: Using Matter headers from third_party/connectedhomeip");
    } else {
        eprintln!("INFO: Using Matter submodule source at third_party/connectedhomeip");
    }

    // =========================================================================
    // Step 2: Check for real ZAP data
    // =========================================================================

    // Check if real ZAP-generated data should be used
    // Set USE_REAL_ZAP_DATA=1 environment variable to enable
    let use_real_zap_data =
        env::var("USE_REAL_ZAP_DATA").unwrap_or_else(|_| "0".to_string()) == "1";

    if use_real_zap_data {
        eprintln!("INFO: Real ZAP data enabled (USE_REAL_ZAP_DATA=1)");

        // Verify ZAP-generated files exist
        let zap_gen_dir = prebuilt_lib_dir.join("include/zap-generated");
        if !zap_gen_dir.join("gen_config.h").exists()
            || !zap_gen_dir.join("endpoint_config.h").exists()
        {
            eprintln!(
                "WARNING: USE_REAL_ZAP_DATA=1 but ZAP files not found at {}",
                zap_gen_dir.display()
            );
            eprintln!("         Expected files: gen_config.h, endpoint_config.h");
            eprintln!(
                "         See docs/ZAP_INTEGRATION.md for instructions on generating ZAP data"
            );
            eprintln!("         Falling back to stub mode");
        } else {
            eprintln!(
                "INFO: Found ZAP-generated files at {}",
                zap_gen_dir.display()
            );
        }
    } else {
        eprintln!("INFO: Using stub ZAP data (set USE_REAL_ZAP_DATA=1 to enable real ZAP data)");
    }

    // =========================================================================
    // Step 3: Compile the C wrapper
    // =========================================================================

    let mut build = cc::Build::new();

    build
        .cpp(true)
        .file("src/matter/ffi/matter_wrapper.cpp")
        .include("src/matter/ffi")
        .flag_if_supported("-std=c++17")
        .flag_if_supported("-Wall")
        .flag_if_supported("-Wextra");

    // Add Connected Home IP includes if available
    if chip_available {
        // Add include paths from submodule (required for both pre-built and source builds)
        build
            .include(&chip_root) // Root for zap-generated includes
            .include(chip_root.join("src"))
            .include(chip_root.join("src/include"))
            .include(chip_root.join("src/lib"))
            .include(chip_root.join("config")) // Build configuration headers
            .include(chip_root.join("config/standalone")) // Standalone project config
            .include(chip_root.join("out/unified/gen/include")) // Generated build config headers
            .include(chip_root.join("zzz_generated/app-common"))
            .include(chip_root.join("third_party/nlassert/repo/include"))
            .include(chip_root.join("third_party/nlio/repo/include"))
            .include(chip_root.join("third_party/inipp/repo/inipp"));

        // Add GLib system includes (required by Matter platform layer on Linux)
        // These are needed for src/platform/GLibTypeDeleter.h and BlueZ support
        for glib_include in get_glib_includes() {
            build.include(&glib_include);
        }

        // Define that we have the full SDK (either pre-built or from source)
        build.define("MATTER_SDK_AVAILABLE", "1");

        // Add required CHIP configuration defines for Linux
        build.define("CHIP_HAVE_CONFIG_H", "1");
        build.define("CHIP_SYSTEM_CONFIG_USE_SOCKETS", "1");
        build.define(
            "CHIP_ADDRESS_RESOLVE_IMPL_INCLUDE_HEADER",
            "<lib/address_resolve/AddressResolve_DefaultImpl.h>",
        );

        // Enable real ZAP data if requested and files exist
        if use_real_zap_data {
            let zap_gen_dir = if has_prebuilt {
                prebuilt_lib_dir.join("include/zap-generated")
            } else {
                chip_root.join("zzz_generated/thermostat/zap-generated")
            };

            if zap_gen_dir.join("gen_config.h").exists()
                && zap_gen_dir.join("endpoint_config.h").exists()
            {
                build.define("USE_REAL_ZAP_DATA", "1");
                eprintln!("INFO: Compiling with real ZAP data support");
                println!("cargo:rustc-cfg=use_real_zap_data");
            }
        }

        println!("cargo:rustc-cfg=matter_sdk_available");
    } else {
        // Stub implementation
        build.define("MATTER_SDK_STUB", "1");
        println!("cargo:rustc-cfg=matter_sdk_stub");
    }

    build.compile("matter_wrapper");

    // =========================================================================
    // Step 4: Link against Connected Home IP library (if available)
    // =========================================================================

    if chip_available {
        // Try to find libraries in the following order:
        // 1. Pre-built libs in lib/connectedhomeip/{platform}/ (already checked)
        // 2. Submodule build at third_party/connectedhomeip/out/host/lib/

        let chip_lib_dir = chip_root.join("out/host/lib");

        let lib_dir = if has_prebuilt {
            eprintln!(
                "INFO: Using pre-built Matter libraries from lib/connectedhomeip/{}",
                platform
            );
            prebuilt_lib_dir
        } else if has_submodule && chip_lib_dir.exists() {
            eprintln!("INFO: Using Matter libraries from submodule build");
            chip_lib_dir
        } else if has_submodule {
            eprintln!("WARNING: Matter submodule found but libraries not built.");
            eprintln!("         To build the libraries:");
            eprintln!("           cd third_party/connectedhomeip");
            eprintln!("           source scripts/activate.sh");
            eprintln!("           gn gen out/host");
            eprintln!("           ninja -C out/host");
            eprintln!();
            PathBuf::new() // Empty path
        } else {
            // Should not reach here since chip_available requires has_prebuilt || has_submodule
            PathBuf::new() // Empty path
        };

        if !lib_dir.as_os_str().is_empty() {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());

            // Link the main Matter library
            // libCHIP.a is built with complete_static_lib = true, which includes all dependencies
            // (controller, app server, setup payload, etc.) in a single archive
            println!("cargo:rustc-link-lib=static=CHIP");

            // System libraries required by Matter
            println!("cargo:rustc-link-lib=dylib=pthread");
            println!("cargo:rustc-link-lib=dylib=stdc++");

            // GLib libraries required by Matter platform layer on Linux
            // The Matter library uses GLib types (like GError) for platform abstraction
            println!("cargo:rustc-link-lib=dylib=glib-2.0");
            println!("cargo:rustc-link-lib=dylib=gobject-2.0");
            println!("cargo:rustc-link-lib=dylib=gio-2.0");
        }
    }

    // =========================================================================
    // Step 5: Generate Rust bindings
    // =========================================================================

    #[cfg(feature = "bindgen")]
    {
        let bindings = bindgen::Builder::default()
            .header("src/matter/ffi/matter_wrapper.h")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            // Generate bindings for our wrapper API
            .allowlist_function("matter_.*")
            .allowlist_type("Matter.*")
            .allowlist_var("MATTER_.*")
            // Use core instead of std for no_std compatibility (future-proofing)
            .use_core()
            // Generate Debug trait implementations
            .derive_debug(true)
            // Generate Default trait implementations where possible
            .derive_default(true)
            .generate()
            .expect("Unable to generate bindings");

        bindings
            .write_to_file(out_dir.join("matter_bindings.rs"))
            .expect("Couldn't write bindings!");
    }

    // =========================================================================
    // Step 6: Output build information
    // =========================================================================

    let build_type = if chip_available {
        if use_real_zap_data {
            "full-with-zap"
        } else {
            "full-stub-zap"
        }
    } else {
        "stub"
    };

    println!("cargo:rustc-env=MATTER_BUILD_TYPE={}", build_type);

    // Rerun build script if ZAP data environment variable changes
    println!("cargo:rerun-if-env-changed=USE_REAL_ZAP_DATA");
}
