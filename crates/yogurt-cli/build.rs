//! Build script for the `yogurt` CLI binary.
//!
//! The CLI transitively links `screencapturekit` 8.x via the
//! `yogurt-server` → `yogurt-audio` dependency chain. `cargo:rustc-link-arg`
//! emitted by a dependency's build script does NOT propagate to the final
//! binary's `LC_RPATH` table — each final-binary-producing package must
//! emit the rpath flag itself.
//!
//! Without this the `yogurt` binary dies at load with:
//!
//!     dyld: Library not loaded: @rpath/libswift_Concurrency.dylib
//!
//! See `docs/archive/superpowers/notes/2026-06-25-sck-spike-result.md` ("What
//! didn't work" #1) for the full discovery story.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }

    // Phase 9 (Plan 09-03): capture the compiling rustc's version string so
    // `yogurt doctor` can report it without shelling out to `rustc` at
    // runtime (which may not even be installed on a brew user's machine).
    let rustc_version = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=YOGURT_RUSTC_VERSION={rustc_version}");
}
