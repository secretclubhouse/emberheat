//! Build script for compiling Matter C wrapper and linking Connected Home IP library
//!
//! This script:
//! 1. Compiles the Matter C wrapper (matter_wrapper.cpp)
//! 2. Links against the Connected Home IP library (when available)
//! 3. Generates Rust bindings using bindgen

#![allow(unexpected_cfgs)]

use std::env;
use std::path::PathBuf;

fn main() {
    let _out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=src/matter/ffi/matter_wrapper.h");
    println!("cargo:rerun-if-changed=src/matter/ffi/matter_wrapper.cpp");

    // =========================================================================
    // Step 1: Check for Connected Home IP library
    // =========================================================================

    let chip_root = PathBuf::from("third_party/connectedhomeip");
    let chip_available = chip_root.exists() && chip_root.join("src").exists();

    if !chip_available {
        eprintln!("WARNING: Connected Home IP library not found at third_party/connectedhomeip");
        eprintln!("         The Matter integration will use stub implementation.");
        eprintln!("         To enable full Matter support, run:");
        eprintln!("         git submodule update --init --recursive");
        eprintln!("");
    }

    // =========================================================================
    // Step 2: Compile the C wrapper
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
        build
            .include(chip_root.join("src"))
            .include(chip_root.join("src/include"))
            .include(chip_root.join("src/lib"))
            .include(chip_root.join("zzz_generated/app-common"))
            .include(chip_root.join("third_party/nlassert/include"))
            .include(chip_root.join("third_party/nlio/include"));

        // Define that we have the full SDK
        build.define("MATTER_SDK_AVAILABLE", "1");

        println!("cargo:rustc-cfg=matter_sdk_available");
    } else {
        // Stub implementation
        build.define("MATTER_SDK_STUB", "1");
        println!("cargo:rustc-cfg=matter_sdk_stub");
    }

    build.compile("matter_wrapper");

    // =========================================================================
    // Step 3: Link against Connected Home IP library (if available)
    // =========================================================================

    if chip_available {
        // The Connected Home IP library is typically built separately using GN/Ninja
        // We expect it to be pre-built in third_party/connectedhomeip/out/host
        let chip_lib_dir = chip_root.join("out/host/lib");

        if chip_lib_dir.exists() {
            println!("cargo:rustc-link-search=native={}", chip_lib_dir.display());

            // Link required Matter libraries
            // Note: The exact libraries depend on the build configuration
            println!("cargo:rustc-link-lib=static=CHIPAppServerLib");
            println!("cargo:rustc-link-lib=static=chip");
            println!("cargo:rustc-link-lib=static=CHIPController");
            println!("cargo:rustc-link-lib=static=CHIPSetupPayload");

            // System libraries required by Matter
            println!("cargo:rustc-link-lib=dylib=pthread");
            println!("cargo:rustc-link-lib=dylib=stdc++");
        } else {
            eprintln!("WARNING: Connected Home IP library not built.");
            eprintln!("         Please build it first:");
            eprintln!("         cd third_party/connectedhomeip");
            eprintln!("         source scripts/activate.sh");
            eprintln!("         gn gen out/host");
            eprintln!("         ninja -C out/host");
            eprintln!("");
        }
    }

    // =========================================================================
    // Step 4: Generate Rust bindings
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
    // Step 5: Output build information
    // =========================================================================

    println!("cargo:rustc-env=MATTER_BUILD_TYPE={}",
             if chip_available { "full" } else { "stub" });
}
