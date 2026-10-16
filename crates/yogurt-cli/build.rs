//! Emits the rpath the final binary needs. A dependency's `cargo:rustc-link-arg` (screencapturekit) does not reach this binary's LC_RPATH,
//! so without it `yogurt` dies at load with `dyld: Library not loaded: @rpath/libswift_Concurrency.dylib`.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-link-arg=-Wl,-rpath,/usr/lib/swift");
    }

    // Lets `yogurt doctor` report the rustc version without shelling out, since rustc may not be installed on a brew user's machine.
    let rustc_version = std::process::Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=YOGURT_RUSTC_VERSION={rustc_version}");
}
