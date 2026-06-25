//! Manual smoke test for Screen Recording permission detection.
//!
//! This test is `#[ignore]` by default — CI cannot grant TCC permissions,
//! so we run it locally with `cargo test -p yogurt-audio --test permission --ignored`.

#![cfg(target_os = "macos")]

use yogurt_audio::{has_screen_recording_permission, PermissionStatus};

#[test]
#[ignore = "manual smoke — requires a real Mac with TCC interaction"]
fn manual_smoke_permission_detection() {
    println!();
    println!("=== Manual smoke: Screen Recording permission ===");
    println!();
    println!("Run this test in two passes:");
    println!();
    println!("  PASS 1 — without permission");
    println!("    1. Open System Settings → Privacy & Security → Screen Recording");
    println!("    2. If `yogurt` (or the cargo test binary) is listed, toggle it OFF and quit it.");
    println!("    3. Run: cargo test -p yogurt-audio --test permission --ignored -- --nocapture");
    println!("    4. Expect: 'CURRENT STATUS: Denied' printed below.");
    println!();
    println!("  PASS 2 — with permission");
    println!("    1. Toggle the cargo test binary ON in System Settings.");
    println!("    2. Quit the test runner (Cmd-Q if any window is open).");
    println!("    3. Re-run the same cargo command.");
    println!("    4. Expect: 'CURRENT STATUS: Granted'.");
    println!();
    println!("  ALSO VERIFY:");
    println!("    [ ] Apple Silicon (M-series) Mac — run both passes.");
    println!("    [ ] Intel Mac (if available) — run both passes.");
    println!("    [ ] macOS 13 (minimum supported) — at least one pass.");
    println!("    [ ] macOS 14 + 15 — at least one pass each.");
    println!();

    let status = has_screen_recording_permission();
    println!("CURRENT STATUS: {:?}", status);
    println!();

    // No assertion on the value — both Granted and Denied are valid outcomes
    // depending on what the human just configured. We assert only that we
    // returned *some* valid macOS-side variant.
    assert!(matches!(
        status,
        PermissionStatus::Granted | PermissionStatus::Denied
    ));
}
