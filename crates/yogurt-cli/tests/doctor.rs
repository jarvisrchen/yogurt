//! Integration tests for `yogurt doctor` (Phase 9, Plan 09-03 / DIST-08).
//!
//! SET-12 follow-up pattern (see tests/cli.rs): redirect HOME to a tempdir
//! so the spawned `yogurt` subprocess never touches the developer's real
//! `~/.yogurt/db.sqlite`.

use assert_cmd::Command;

#[test]
fn it_runs_doctor_and_prints_diagnostics() {
    let tmp = tempfile::tempdir().expect("create tempdir for HOME");
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("doctor").env("HOME", tmp.path());
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    for label in [
        "yogurt doctor",
        "rust:",
        "macos:",
        "screen recording:",
        "db path:",
        "providers:",
        "stt:",
        "models:",
    ] {
        assert!(
            stdout.contains(label),
            "doctor output missing {label:?}; got:\n{stdout}"
        );
    }
}

#[test]
fn it_runs_doctor_with_json_flag() {
    let tmp = tempfile::tempdir().expect("create tempdir for HOME");
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(["doctor", "--json"]).env("HOME", tmp.path());
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --json must emit valid JSON");
    assert_eq!(json["service"], "yogurt-doctor");
    assert!(json["rust"].is_string());
    assert!(json["macos"].is_string());
}

#[test]
fn it_runs_doctor_check_port() {
    let tmp = tempfile::tempdir().expect("create tempdir for HOME");
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(["doctor", "--check-port"]).env("HOME", tmp.path());
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(
        stdout.contains("port 7878 is free") || stdout.contains("port 7878 is in use"),
        "expected a free/in-use port status line; got:\n{stdout}"
    );
}
