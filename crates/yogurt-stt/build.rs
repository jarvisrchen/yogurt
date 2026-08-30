//! Link the Clang runtime library alongside whisper.cpp's Metal backend.
//!
//! `ggml-metal.m` guards newer Metal APIs with `@available`. Because our
//! deployment target is macOS 13 (the project floor) while those guards probe
//! macOS 14+, Clang cannot fold them at compile time and lowers each one to a
//! call to `__isPlatformVersionAtLeast`. That symbol lives in
//! `libclang_rt.osx.a`, but rustc links with `-nodefaultlibs` and Rust's
//! `compiler_builtins` does not carry it - so the final binary ends up with an
//! undefined reference unless we pull the Clang runtime in ourselves.
//!
//! Only relevant when `local-stt` is on (that is what builds whisper.cpp) and
//! only on macOS, which is the only platform yogurt targets anyway.

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
    // `-bundle` matters: the default (`+bundle`) copies every member of the
    // archive into our rlib, including the `compiler_rt` builtins (`adddf3`,
    // `ashldi3`, ...) that Rust's own `compiler_builtins` already provides.
    // With `-bundle` the archive is left on the final link line instead, so the
    // linker pulls only the members that resolve a genuinely undefined symbol -
    // usually just `os_version_check`, and nothing at all on toolchains that
    // never emit the availability call.
    println!("cargo:rustc-link-lib=static:-bundle=clang_rt.osx");
}

/// Ask the C compiler where its runtime libraries live, honouring `CC` so this
/// tracks whatever toolchain actually compiled whisper.cpp.
fn clang_runtime_dir() -> Option<PathBuf> {
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let output = Command::new(cc).arg("--print-runtime-dir").output().ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8(output.stdout).ok()?.trim());
    path.is_dir().then_some(path)
}
