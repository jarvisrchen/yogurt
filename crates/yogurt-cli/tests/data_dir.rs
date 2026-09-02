//! Integration tests for CLI-7 (`yogurt start --data-dir` / `$YOGURT_DATA_DIR`).
//!
//! Shaped like `tests/cli.rs`: spawn the real binary with a temp `HOME`
//! (every `~/.yogurt` path resolves through `BaseDirs`, so `HOME` is the
//! shared-database isolation mechanism the existing tests already use) on
//! an ephemeral port. Here `HOME` and the `--data-dir` override are
//! DIFFERENT tempdirs, so the assertions can tell whether the database
//! actually moved.

use assert_cmd::Command;
use std::net::TcpListener as StdTcpListener;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn data_dir_flag_relocates_the_database() {
    let home = tempfile::tempdir().expect("create tempdir for HOME");
    let data_dir = tempfile::tempdir().expect("create tempdir for --data-dir");

    let probe = StdTcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = probe.local_addr().expect("local_addr").port();
    drop(probe);

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args([
            "start",
            "--port",
            &port.to_string(),
            "--no-open",
            "--data-dir",
        ])
        .arg(data_dir.path())
        .env("HOME", home.path())
        .env("YOGURT_MEMORY_KEYSTORE", "1")
        .spawn()
        .expect("spawn yogurt start");

    let mut healthy = false;
    for _ in 0..100 {
        if let Ok(resp) = reqwest::get(format!("http://127.0.0.1:{port}/api/health")).await {
            if resp.status().is_success() {
                healthy = true;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(healthy, "server on port {port} never reported healthy");

    assert!(
        data_dir.path().join("db.sqlite").exists(),
        "db.sqlite should exist under --data-dir"
    );
    assert!(
        !home.path().join(".yogurt").join("db.sqlite").exists(),
        "db.sqlite must NOT exist under the temp HOME's .yogurt when --data-dir is given"
    );

    child.kill().await.ok();

    // `yogurt doctor --json` with $YOGURT_DATA_DIR set (no server running)
    // must report the same relocated path.
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(["doctor", "--json"])
        .env("HOME", home.path())
        .env("YOGURT_DATA_DIR", data_dir.path());
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --json must emit valid JSON");
    let expected = data_dir.path().join("db.sqlite");
    assert_eq!(
        json["db_path"],
        expected.display().to_string(),
        "doctor --json db_path should be the --data-dir path; got {json}"
    );
}

#[test]
fn env_var_relocates_doctor_db_path_with_no_flag() {
    let home = tempfile::tempdir().expect("create tempdir for HOME");
    let data_dir = tempfile::tempdir().expect("create tempdir for YOGURT_DATA_DIR");

    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(["doctor", "--json"])
        .env("HOME", home.path())
        .env("YOGURT_DATA_DIR", data_dir.path());
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --json must emit valid JSON");
    let expected = data_dir.path().join("db.sqlite");
    assert_eq!(json["db_path"], expected.display().to_string());
    // YOGURT_DATA_DIR must be created even though the DB is never opened
    // (doctor never creates db.sqlite as a side effect).
    assert!(data_dir.path().is_dir());
}
