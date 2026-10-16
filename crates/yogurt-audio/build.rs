//! Adds `/usr/lib/swift` to the binary's `LC_RPATH` so `libswift_Concurrency.dylib` loads.
//! The `screencapturekit` crate's own link arg does not propagate to the final binary.
//! Do not add Xcode's swift-5.5 toolchain path as a second fallback: two copies of the dylib
//! load, which breaks `SCShareableContent::get()` with a spurious TCC failure.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
