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
    // Spawn `yogurt start` in the background.
    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", "17879", "--no-open"])
        .spawn()
        .expect("spawn yogurt");

    tokio::time::sleep(Duration::from_millis(400)).await;

    let body = reqwest::get("http://127.0.0.1:17879/api/health")
        .await
        .expect("server reachable")
        .text()
        .await
        .unwrap();
    assert!(body.contains("\"status\":\"ok\""));

    child.kill().await.ok();
}

#[tokio::test(flavor = "multi_thread")]
async fn it_reports_port_conflict_with_friendly_error() {
    // Occupy the port from the test process.
    let listener = StdTcpListener::bind("127.0.0.1:17883").expect("can bind 17883 in test");

    let output = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", "17883", "--no-open"])
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
