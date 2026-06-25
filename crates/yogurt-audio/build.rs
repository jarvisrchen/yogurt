//! Build script for `yogurt-audio`.
//!
//! Bakes `/usr/lib/swift` into the consuming binary's `LC_RPATH` table so the
//! Swift Concurrency runtime (`libswift_Concurrency.dylib`) loads at runtime.
//! The `screencapturekit` 8.x crate transitively links against the Swift
//! Concurrency library; its own `build.rs` emits a rustc link arg, but those
//! flags don't propagate to the final binary's rpath table when SCK is a
//! transitive dependency. Without this, downstream binaries die at load with:
//!
//!     dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
//!       Reason: no LC_RPATH's found
//!
//! See `docs/superpowers/notes/2026-06-25-sck-spike-result.md` ("What didn't
//! work" #1) for the full discovery story.
//!
//! Do **not** add Xcode's swift-5.5 toolchain path as a second fallback —
//! combining the two loads two copies of the dylib, triggers
//! `objc[]: Class … is implemented in both …` warnings, and causes a
//! spurious `SCShareableContent::get()` "TCC declined" failure.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }
}
