//! Build script for emberheat
//!
//! This is a minimal build script. Matter/Connected Home IP integration
//! has been removed. See MATTER_INTEGRATION.md for re-integration instructions.

fn main() {
    // Declare custom cfg attributes for conditional compilation
    println!("cargo::rustc-check-cfg=cfg(matter_enabled)");

    // Matter integration is currently disabled
    // When re-implementing, you'll need to:
    // 1. Compile the Matter C wrapper (matter_wrapper.cpp)
    // 2. Link against the Connected Home IP library
    // 3. Generate Rust bindings using bindgen
    //
    // See MATTER_INTEGRATION.md for detailed instructions.

    println!("cargo:rustc-env=MATTER_BUILD_TYPE=stub");
}
