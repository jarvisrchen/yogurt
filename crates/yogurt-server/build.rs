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
//! See `docs/archive/superpowers/notes/2026-06-25-sck-spike-result.md` ("What
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
        link_clang_runtime();
    }
}

/// Resolve clang's compiler-rt directory (`.../lib/clang/<v>/lib/darwin`).
///
/// Returns `None` if `clang` is missing or the directory does not exist.
#[cfg(target_os = "macos")]
fn clang_runtime_dir() -> Option<String> {
    let out = std::process::Command::new("clang")
        .arg("-print-resource-dir")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let dir = format!("{}/lib/darwin", String::from_utf8(out.stdout).ok()?.trim());
    std::path::Path::new(&dir).is_dir().then_some(dir)
}

/// Put clang's compiler-rt on the link line.
///
/// whisper.cpp's Metal backend (`ggml-metal-device.m`) guards its
/// residency-set code with Objective-C `@available` checks, and clang lowers
/// each one into a call to `__isPlatformVersionAtLeast`, which lives in
/// `libclang_rt.osx.a`. rustc links with `-nodefaultlibs`, so that archive is
/// never on the link line and arm64 dies with:
///
///     Undefined symbols for architecture arm64:
///       "___isPlatformVersionAtLeast", referenced from:
///           ___ggml_metal_rsets_init_block_invoke in libwhisper_rs_sys.rlib
///
/// Only arm64 builds `ggml-metal` (hence x86_64 linking fine), but the archive
/// is universal, so link it for every macOS target rather than gating on arch.
/// Whether the code is reached at all depends on the SDK: older SDKs compile
/// the residency-set path out entirely, which is why this only started failing
/// when the CI runner moved to Xcode 26.
#[cfg(target_os = "macos")]
fn link_clang_runtime() {
    match clang_runtime_dir() {
        Some(dir) => {
            println!("cargo:rustc-link-search=native={dir}");
            println!("cargo:rustc-link-lib=static=clang_rt.osx");
        }
        None => println!(
            "cargo:warning=clang compiler-rt not found; a whisper Metal build will              fail to link with an undefined ___isPlatformVersionAtLeast"
        ),
    }
}
