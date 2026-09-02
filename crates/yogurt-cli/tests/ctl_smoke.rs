//! Integration tests for `yogurt ctl` (CLI-4).
//!
//! Shaped like `tests/cli.rs`: spawn the real binary with a temp `HOME`
//! (every `~/.yogurt` path resolves through `BaseDirs`, so `HOME` is the
//! isolation mechanism) on an ephemeral port, poll `/api/health`, then
//! drive `yogurt ctl --port P ... --json` against it and assert. Each test
//! gets its own server + tempdir rather than sharing one, matching
//! `tests/cli.rs`'s existing pattern -- a `ServerGuard` kills the child on
//! drop, so a panicking assertion still cleans up.
//!
//! Hardware paths (`ctl windows`, a real `ctl meeting start`) are
//! deliberately NOT exercised here -- see AGENTS.md and the PR body for
//! how those were verified by hand.

use assert_cmd::Command;
use std::net::TcpListener as StdTcpListener;
use std::path::Path;
use std::time::Duration;

struct ServerGuard {
    child: tokio::process::Child,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        // start_kill is sync (fire-and-forget) -- Drop can't await, and a
        // leaked but SIGKILL'd child holding no listeners is harmless.
        let _ = self.child.start_kill();
    }
}

async fn spawn_server(home: &Path) -> (ServerGuard, u16) {
    let probe = StdTcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let port = probe.local_addr().expect("local_addr").port();
    drop(probe);

    let child = tokio::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .args(["start", "--port", &port.to_string(), "--no-open"])
        .env("HOME", home)
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
    (ServerGuard { child }, port)
}

fn ctl(port: u16, home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("ctl")
        .arg("--port")
        .arg(port.to_string())
        .args(args)
        .env("HOME", home);
    cmd.assert()
}

#[tokio::test(flavor = "multi_thread")]
async fn status_with_no_server_exits_1_with_help() {
    let tmp = tempfile::tempdir().expect("tempdir");
    // Bind-then-drop to name a genuinely free ephemeral port -- nothing is
    // ever started on it.
    let probe = StdTcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
    let free_port = probe.local_addr().expect("local_addr").port();
    drop(probe);

    let output = ctl(free_port, tmp.path(), &["status"]).failure();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("error:"),
        "expected an error line; got:\n{stdout}"
    );
    assert!(
        stdout.contains("help:"),
        "expected a help line; got:\n{stdout}"
    );
    assert_eq!(output.get_output().status.code(), Some(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn status_against_running_server_prints_version_and_mode() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let output = ctl(port, tmp.path(), &["status", "--json"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let body: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");

    let instances = body["instances"].as_array().expect("instances array");
    assert_eq!(instances.len(), 1);
    assert_eq!(instances[0]["port"], port);
    assert_eq!(instances[0]["mode"], "release");
    assert!(instances[0]["version"].is_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_new_then_show_and_list() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let created = ctl(
        port,
        tmp.path(),
        &["meeting", "new", "--title", "ctl smoke", "--json"],
    )
    .success();
    let stdout = String::from_utf8(created.get_output().stdout.clone()).unwrap();
    let created: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    let id = created["id"].as_str().expect("id field").to_string();

    let shown = ctl(port, tmp.path(), &["meeting", "show", &id, "--json"]).success();
    let stdout = String::from_utf8(shown.get_output().stdout.clone()).unwrap();
    let shown: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(shown["meeting"]["id"], id);
    assert_eq!(shown["meeting"]["title"], "ctl smoke");
    assert_eq!(shown["source"], "server");

    let listed = ctl(port, tmp.path(), &["meeting", "list", "--json"]).success();
    let stdout = String::from_utf8(listed.get_output().stdout.clone()).unwrap();
    let listed: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert!(listed["total"].as_u64().unwrap() >= 1);
    let ids: Vec<&str> = listed["meetings"]
        .as_array()
        .expect("meetings array")
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&id.as_str()), "expected {id} in {ids:?}");

    // `last` resolves to the newest meeting -- the one just created.
    let last = ctl(port, tmp.path(), &["meeting", "show", "last", "--json"]).success();
    let stdout = String::from_utf8(last.get_output().stdout.clone()).unwrap();
    let last: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(last["meeting"]["id"], id);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_with_no_active_meeting_is_a_noop() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    // Nothing was ever started (the hardware `start` path is out of scope
    // for this suite -- see the module doc comment), so bare `stop` must
    // be a no-op rather than an error.
    let output = ctl(port, tmp.path(), &["meeting", "stop"]).success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    assert!(
        stdout.contains("no active meeting"),
        "expected a no-op message; got:\n{stdout}"
    );
    assert_eq!(output.get_output().status.code(), Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn no_subcommand_reveals_or_sets_a_key_or_token() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    // The whole --help tree: top level, ctl, and every ctl subcommand.
    for args in [
        vec!["--help"],
        vec!["ctl", "--help"],
        vec!["ctl", "status", "--help"],
        vec!["ctl", "meeting", "--help"],
        vec!["ctl", "meeting", "new", "--help"],
        vec!["ctl", "meeting", "start", "--help"],
        vec!["ctl", "meeting", "stop", "--help"],
        vec!["ctl", "meeting", "show", "--help"],
        vec!["ctl", "meeting", "summary", "--help"],
        vec!["ctl", "meeting", "transcript", "--help"],
        vec!["ctl", "meeting", "enhance", "--help"],
        vec!["ctl", "detect", "--help"],
        vec!["ctl", "windows", "--help"],
    ] {
        let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
        cmd.args(&args).env("HOME", tmp.path());
        let output = cmd.assert().success();
        let stdout = String::from_utf8(output.get_output().stdout.clone())
            .unwrap()
            .to_lowercase();
        assert!(
            !stdout.contains("token") && !stdout.contains("key"),
            "{args:?} --help mentions a key/token: {stdout}"
        );
    }

    // The real token must never appear in `status` output, text or JSON.
    let token = std::fs::read_to_string(tmp.path().join(".yogurt/session-token"))
        .expect("session-token exists once the server has booted")
        .trim()
        .to_string();
    assert!(!token.is_empty());

    for args in [vec!["status"], vec!["status", "--json"]] {
        let output = ctl(port, tmp.path(), &args).success();
        let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        assert!(
            !stdout.contains(&token),
            "status output leaked the session token: {stdout}"
        );
    }
}
