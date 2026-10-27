//! HOME and the `--data-dir` override are different tempdirs, so assertions can tell whether the database moved.

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
    // The data dir is created even though doctor never opens the DB.
    assert!(data_dir.path().is_dir());
}

#[tokio::test(flavor = "multi_thread")]
async fn data_dir_flag_relocates_keys_json() {
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
        .env_remove("YOGURT_MEMORY_KEYSTORE")
        .env("YOGURT_DEEPGRAM_API_KEY", "dg-test-key")
        .kill_on_drop(true)
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

    child.kill().await.ok();

    let keys = std::fs::read_to_string(data_dir.path().join("keys.json"))
        .expect("keys.json should exist under --data-dir");
    assert!(keys.contains("dg-test-key"), "seeded key missing: {keys}");
    assert!(
        !home.path().join(".yogurt").join("keys.json").exists(),
        "keys.json must NOT exist under the temp HOME's .yogurt when --data-dir is given"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn data_dir_moves_notes_and_token_and_ctl_follows() {
    let home = tempfile::tempdir().expect("create tempdir for HOME");
    let data_dir = tempfile::tempdir().expect("create tempdir for YOGURT_DATA_DIR");

    let probe = StdTcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = probe.local_addr().expect("local_addr").port();
    drop(probe);

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", &port.to_string(), "--no-open"])
        .env("HOME", home.path())
        .env("YOGURT_DATA_DIR", data_dir.path())
        .env("YOGURT_MEMORY_KEYSTORE", "1")
        .kill_on_drop(true)
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

    assert!(data_dir.path().join("session-token").exists());
    assert!(data_dir.path().join("notes").is_dir());

    // `ctl meeting new` needs the token, which only exists under the data dir.
    let ctl = |data_dir_env: bool| {
        let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
        cmd.args([
            "ctl",
            "--port",
            &port.to_string(),
            "meeting",
            "new",
            "--json",
        ])
        .env("HOME", home.path());
        if data_dir_env {
            cmd.env("YOGURT_DATA_DIR", data_dir.path());
        } else {
            cmd.env_remove("YOGURT_DATA_DIR");
        }
        cmd
    };
    ctl(true).assert().success();
    ctl(false).assert().failure();

    let token = std::fs::read_to_string(data_dir.path().join("session-token")).unwrap();
    let settings: serde_json::Value = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/api/settings"))
        .bearer_auth(token.trim())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let dir = data_dir.path().display();
    assert_eq!(settings["paths"]["data_dir"], format!("{dir}"));
    assert_eq!(settings["paths"]["keys"], format!("{dir}/keys.json"));
    assert_eq!(settings["paths"]["notes"], format!("{dir}/notes"));

    child.kill().await.ok();
    assert!(
        !home.path().join(".yogurt").join("session-token").exists(),
        "session-token must NOT land under HOME when YOGURT_DATA_DIR is set"
    );
    assert!(
        !home.path().join(".yogurt").join("notes").exists(),
        "notes must NOT land under HOME when YOGURT_DATA_DIR is set"
    );
}
