//! Build script for `yogurt-server`.
//!
//! `yogurt-server` transitively links the `screencapturekit` 8.x crate
//! through its dependency on `yogurt-audio`. `cargo:rustc-link-arg`
//! emitted by a dependency's build script does NOT propagate to a
//! downstream binary's `LC_RPATH` table — each final-binary-producing
//! package must emit the rpath flag itself.
//!
//! Without this, every test binary, every server binary, and every
//! example that links `yogurt-server` dies at load with:
//!
//!     dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
//!
//! See `docs/superpowers/notes/2026-06-25-sck-spike-result.md` ("What
//! didn't work" #1) and `crates/yogurt-audio/build.rs` for the full
//! discovery story.
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
