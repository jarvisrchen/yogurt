//! A dependency's `cargo:rustc-link-arg` does not reach a downstream binary's `LC_RPATH`,
//! so this crate emits the Swift rpath itself or every binary linking it dies with
//! `dyld: Library not loaded: @rpath/libswift_Concurrency.dylib`.
//!
//! Do not add Xcode's swift-5.5 toolchain path as a second fallback. Two copies of the dylib
//! load, which logs `objc[]: Class ... is implemented in both` and fails
//! `SCShareableContent::get()` with a spurious "TCC declined".

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
