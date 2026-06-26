use assert_cmd::Command;
use std::net::TcpListener as StdTcpListener;
use std::time::Duration;

#[test]
fn it_prints_help() {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("--help");
    let output = cmd.assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("yogurt"),
        "help should mention the binary name"
    );
    assert!(
        stdout.contains("start"),
        "help should mention the `start` subcommand"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_starts_server_and_serves_health() {
    // SET-12 follow-up: see it_reports_port_conflict_with_friendly_error
    // for HOME-tempdir rationale.
    let tmp = tempfile::tempdir().expect("create tempdir for HOME");

    // Spawn `yogurt start` in the background.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", "17879", "--no-open"])
        .env("HOME", tmp.path())
        .spawn()
        .expect("spawn yogurt");

    // LO-04: poll for readiness rather than a fixed sleep. CI runners can be
    // very slow on first cargo invocation; the old 400ms sleep was a classic
    // flake source. ws_auth.rs uses this same pattern.
    let mut body = String::new();
    for _ in 0..100 {
        if let Ok(resp) = reqwest::get("http://127.0.0.1:17879/api/health").await {
            if let Ok(t) = resp.text().await {
                body = t;
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        body.contains("\"status\":\"ok\""),
        "server never reported healthy; got body: {body:?}"
    );

    child.kill().await.ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_reports_port_conflict_with_friendly_error() {
    // Occupy the port from the test process.
    let listener = StdTcpListener::bind("127.0.0.1:17883").expect("can bind 17883 in test");

    // SET-12 follow-up: redirect HOME to a tempdir so the spawned `yogurt`
    // subprocess doesn't touch the developer's real ~/.yogurt/db.sqlite
    // (which may be at a higher migration version than the test binary
    // knows about, causing DatabaseTooFarAhead errors that mask the actual
    // port-conflict assertion).
    let tmp = tempfile::tempdir().expect("create tempdir for HOME");

    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", "17883", "--no-open"])
        .env("HOME", tmp.path())
        .output()
        .await
        .expect("spawn yogurt");

    drop(listener); // release port for other tests

    assert!(
        !output.status.success(),
        "expected non-zero exit; got {:?}",
        output.status
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already in use"),
        "stderr should contain 'already in use'; got: {stderr}"
    );
    assert!(
        stderr.contains("--port 17884"),
        "stderr should suggest --port 17884; got: {stderr}"
    );
    assert!(
        stderr.contains("lsof -i :17883"),
        "stderr should suggest lsof -i :17883; got: {stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn it_does_not_suggest_port_0_at_upper_boundary() {
    // HI-01 regression: --port 65535 must NOT suggest --port 0 (which on Unix
    // means "ephemeral bind" -- terrible advice). Use 65535 as the held port.
    let listener = StdTcpListener::bind("127.0.0.1:65535");
    if listener.is_err() {
        // 65535 may be unavailable in CI sandbox; skip rather than fail.
        eprintln!("skipping: cannot bind 127.0.0.1:65535 in this environment");
        return;
    }
    let _listener = listener.unwrap();

    // SET-12 follow-up: see it_reports_port_conflict_with_friendly_error
    // for rationale on the HOME tempdir.
    let tmp = tempfile::tempdir().expect("create tempdir for HOME");

    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", "65535", "--no-open"])
        .env("HOME", tmp.path())
        .output()
        .await
        .expect("spawn yogurt");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already in use"),
        "stderr should contain 'already in use'; got: {stderr}"
    );
    assert!(
        !stderr.contains("--port 0"),
        "stderr must NOT suggest --port 0 (kernel ephemeral); got: {stderr}"
    );
    assert!(
        stderr.contains("lsof -i :65535"),
        "stderr should suggest lsof at boundary; got: {stderr}"
    );
}
