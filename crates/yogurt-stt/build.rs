//! Link the Clang runtime library for whisper.cpp's Metal backend.
//!
//! `ggml-metal.m` guards macOS 14+ APIs with `@available`, and with a macOS 13 deployment target Clang lowers each guard to `__isPlatformVersionAtLeast`.
//! That symbol lives in `libclang_rt.osx.a`, which neither rustc's `-nodefaultlibs` link nor `compiler_builtins` provides.
//! Only needed when `local-stt` is on.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CC");

    if std::env::var_os("CARGO_FEATURE_LOCAL_STT").is_none() {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos") {
        return;
    }

    let Some(runtime_dir) = clang_runtime_dir() else {
        println!(
            "cargo:warning=could not resolve the Clang runtime directory; the link may fail \
             with an undefined `___isPlatformVersionAtLeast` from whisper.cpp's Metal backend"
        );
        return;
    };

    if !runtime_dir.join("libclang_rt.osx.a").is_file() {
        println!(
            "cargo:warning=libclang_rt.osx.a not found in {}; check that the Xcode command line \
             tools are installed and `xcode-select -p` points at the toolchain you build with",
            runtime_dir.display()
        );
        return;
    }

    println!("cargo:rustc-link-search=native={}", runtime_dir.display());
    // `-bundle` keeps the archive on the final link line instead of copying its members into our rlib,
    // where its `compiler_rt` builtins would duplicate the ones `compiler_builtins` already provides.
    println!("cargo:rustc-link-lib=static:-bundle=clang_rt.osx");
}

/// Ask the C compiler (honouring `CC`) where its runtime libraries live.
fn clang_runtime_dir() -> Option<PathBuf> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let output = Command::new(cc).arg("--print-runtime-dir").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    path.is_dir().then_some(path)
}
