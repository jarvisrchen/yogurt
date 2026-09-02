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
//! Hardware paths (`ctl windows`, a real `ctl meeting start`) live in the
//! `hw_*` tests at the bottom of this file: `#[ignore]` plus a
//! `YOGURT_HW_TESTS=1` check at the top of each, run only via `just
//! test-hw` (see the recipe comment in `justfile`). Never under `just
//! test` or CI -- a background `cargo test` that starts recording is the
//! failure to avoid. Design: docs/.planning/agent-workflow.md section 4D,
//! D4.

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

    // DX-1: this file spawns 21 real servers per full run, one per test
    // (see the module doc comment) -- under the full workspace's default
    // thread-per-core parallelism that is enough concurrent `cargo test`
    // + `yogurt start` boots to occasionally blow the original 5s budget
    // and flake. 10s gives real headroom without slowing a healthy run
    // (the loop still breaks the instant health responds).
    let mut healthy = false;
    for _ in 0..200 {
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

fn ctl_cmd(port: u16, home: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("ctl")
        .arg("--port")
        .arg(port.to_string())
        .args(args)
        .env("HOME", home);
    cmd
}

fn ctl(port: u16, home: &Path, args: &[&str]) -> assert_cmd::assert::Assert {
    ctl_cmd(port, home, args).assert()
}

/// Run a `ctl` subprocess with a hard wall-clock bound, so a hardware test
/// can never hang the suite -- `Command::output()`/`.ok()` block forever
/// with no timeout of their own. Polls `try_wait()` (non-blocking) rather
/// than spawning a watchdog thread; on timeout, kills the child and
/// returns an error naming the two things that have actually caused this:
/// a pending TCC permission dialog for THIS binary's first run at this
/// worktree path (TCC grants are keyed per-path for unsigned dev builds --
/// see `docs/.planning/agent-workflow.md` section 8), or a stale SCK/mic
/// capture session left open by a prior hardware test that was killed
/// with SIGKILL (which skips `Drop`, so the OS-level session outlives the
/// process and blocks the next `start` until it's reclaimed).
fn ctl_run_bounded(
    port: u16,
    home: &Path,
    args: &[&str],
    bound: Duration,
) -> Result<std::process::Output, String> {
    // assert_cmd::Command privatizes `spawn` (only `.output()`/`.ok()`,
    // both unbounded, are exposed) -- build the std::process::Command
    // directly so we can poll it instead of blocking on it.
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_yogurt"))
        .arg("ctl")
        .arg("--port")
        .arg(port.to_string())
        .args(args)
        .env("HOME", home)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn `ctl {args:?}` failed: {e}"))?;
    let started = std::time::Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("wait on `ctl {args:?}` failed: {e}"))?
        {
            use std::io::Read;
            let mut stdout = Vec::new();
            let mut stderr = Vec::new();
            if let Some(mut o) = child.stdout.take() {
                let _ = o.read_to_end(&mut stdout);
            }
            if let Some(mut e) = child.stderr.take() {
                let _ = e.read_to_end(&mut stderr);
            }
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }
        if started.elapsed() > bound {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "`ctl {args:?}` did not return within {bound:?} -- likely a pending Screen \
                 Recording / Microphone permission prompt for this binary's first run at this \
                 worktree path (grant it in System Settings > Privacy & Security and run once \
                 interactively), or a stale capture session left by a prior killed hardware \
                 test (wait a few seconds for macOS to reclaim it and retry)"
            ));
        }
        std::thread::sleep(Duration::from_millis(200));
    }
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

    // Nothing was ever started (the hardware `start` path lives in the
    // `hw_*` tests at the bottom of this file), so bare `stop` must be a
    // no-op rather than an error.
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
        // CLI-6: second slice --help tree.
        vec!["ctl", "meeting", "mute", "--help"],
        vec!["ctl", "meeting", "mute", "on", "--help"],
        vec!["ctl", "meeting", "mute", "off", "--help"],
        vec!["ctl", "meeting", "search", "--help"],
        vec!["ctl", "meeting", "delete", "--help"],
        vec!["ctl", "detect", "--help"],
        vec!["ctl", "windows", "--help"],
        vec!["ctl", "settings", "--help"],
        vec!["ctl", "settings", "get", "--help"],
        vec!["ctl", "settings", "set", "--help"],
        vec!["ctl", "provider", "--help"],
        vec!["ctl", "provider", "list", "--help"],
        vec!["ctl", "provider", "activate", "--help"],
        vec!["ctl", "provider", "test", "--help"],
        vec!["ctl", "models", "--help"],
        vec!["ctl", "models", "list", "--help"],
        vec!["ctl", "models", "download", "--help"],
        vec!["ctl", "models", "delete", "--help"],
        vec!["ctl", "ws", "--help"],
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

    // CLI-6: `provider list` must show `key: set` without ever printing the
    // key itself. There is no `ctl provider create`/`key set` (deliberately
    // -- see docs/.planning/agent-workflow.md section 5), so the provider
    // and its key are seeded directly over the REST surface, the same way
    // the Settings UI would.
    let fake_key = "sk-test-fake-key-should-never-leak-4f8c1e";
    let http = reqwest::Client::new();
    let provider: serde_json::Value = http
        .post(format!("http://127.0.0.1:{port}/api/settings/providers"))
        .bearer_auth(&token)
        .json(&serde_json::json!({
            "name": "ctl-smoke-provider",
            "base_url": "http://127.0.0.1:1/v1",
            "model": "test-model",
        }))
        .send()
        .await
        .expect("create provider")
        .json()
        .await
        .expect("provider json");
    let provider_id = provider["id"].as_str().expect("provider id").to_string();
    let key_status = http
        .post(format!(
            "http://127.0.0.1:{port}/api/settings/providers/{provider_id}/key"
        ))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "api_key": fake_key }))
        .send()
        .await
        .expect("set provider key")
        .status();
    assert!(key_status.is_success(), "set key: {key_status}");

    for args in [vec!["provider", "list"], vec!["provider", "list", "--json"]] {
        let output = ctl(port, tmp.path(), &args).success();
        let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        assert!(
            !stdout.contains(fake_key) && !stdout.contains("sk-test-fake-key"),
            "{args:?} leaked the provider key: {stdout}"
        );
        assert!(
            stdout.contains("ctl-smoke-provider"),
            "{args:?} did not list the seeded provider: {stdout}"
        );
        assert!(
            stdout.contains("set"),
            "{args:?} did not report key: set for the seeded provider: {stdout}"
        );
    }

    // `settings get` never carries any key material either (it only ever
    // reflects the `general` settings block -- `deepgram_key_masked` and
    // provider keys are not its concern).
    for args in [vec!["settings", "get"], vec!["settings", "get", "--json"]] {
        let output = ctl(port, tmp.path(), &args).success();
        let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
        assert!(
            !stdout.contains(fake_key) && !stdout.to_lowercase().contains("key"),
            "{args:?} mentions a key: {stdout}"
        );
    }

    // Regression: `ctl ws`'s connect-failure message used to interpolate
    // the raw ws:// URL, which carries the session token as `?token=...`
    // (the WS auth contract -- see `crate::ws`'s doc comment on the
    // server side). `models download --wait` shares the same `connect`
    // call. Both must fail cleanly without ever printing the token, on
    // stdout or stderr, text or `--json`.
    for args in [
        vec!["ws", "bogus-meeting-id"],
        vec!["ws", "bogus-meeting-id", "--json"],
        vec!["models", "download", "nope", "--wait"],
        vec!["models", "download", "nope", "--wait", "--json"],
    ] {
        let output = ctl(port, tmp.path(), &args).failure();
        let out = output.get_output();
        let stdout = String::from_utf8(out.stdout.clone()).unwrap();
        let stderr = String::from_utf8(out.stderr.clone()).unwrap();
        assert!(
            !stdout.contains(&token) && !stderr.contains(&token),
            "{args:?} leaked the session token: stdout={stdout:?} stderr={stderr:?}"
        );
    }
}

// ─── CLI-5: fixture meetings ────────────────────────────────────────────

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root")
}

fn stdout_of(output: &assert_cmd::assert::Assert) -> String {
    String::from_utf8(output.get_output().stdout.clone()).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_new_from_script_creates_ended_fixture_with_segment_count() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let script = repo_root().join("scripts/eval/conversation.txt");
    let expected_segments = std::fs::read_to_string(&script)
        .expect("conversation.txt readable")
        .lines()
        .filter(|l| l.starts_with("A: ") || l.starts_with("B: "))
        .count();

    let created = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--from-script",
            script.to_str().unwrap(),
            "--title",
            "fixture from script",
            "--json",
        ],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    let shown = ctl(port, tmp.path(), &["meeting", "show", &id, "--json"]).success();
    let shown: serde_json::Value = serde_json::from_str(&stdout_of(&shown)).unwrap();
    assert_eq!(shown["meeting"]["id"], id);
    assert!(
        shown["meeting"]["ended_at"].is_number(),
        "expected ended_at to be stamped: {shown}"
    );
    assert_eq!(
        shown["segments"].as_u64().unwrap(),
        expected_segments as u64
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_new_transcript_file_round_trips_exactly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let segments_path = tmp.path().join("segments.json");
    std::fs::write(
        &segments_path,
        r#"[
            {"ts_ms": 0, "channel": "me", "text": "hello there"},
            {"ts_ms": 4000, "channel": "them", "text": "hi yourself"},
            {"ts_ms": 8000, "channel": "me", "text": "great, thanks for asking"}
        ]"#,
    )
    .expect("write segments file");

    let created = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--transcript-file",
            segments_path.to_str().unwrap(),
            "--title",
            "fixture round trip",
            "--json",
        ],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    let transcript = ctl(port, tmp.path(), &["meeting", "transcript", &id]).success();
    let stdout = stdout_of(&transcript);
    assert!(stdout.contains("hello there"), "got:\n{stdout}");
    assert!(stdout.contains("hi yourself"), "got:\n{stdout}");
    assert!(
        stdout.contains("great, thanks for asking"),
        "got:\n{stdout}"
    );

    let shown = ctl(port, tmp.path(), &["meeting", "show", &id, "--json"]).success();
    let shown: serde_json::Value = serde_json::from_str(&stdout_of(&shown)).unwrap();
    assert_eq!(shown["segments"].as_u64().unwrap(), 3);
    assert!(shown["meeting"]["ended_at"].is_number());
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_new_malformed_transcript_file_exits_1_with_server_message() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    // Missing `ts_ms` on the one segment -- the server's own validation
    // rejects this, and `ctl` must surface the server's message rather
    // than swallowing it into a generic "server returned 400".
    let segments_path = tmp.path().join("bad-segments.json");
    std::fs::write(&segments_path, r#"[{"channel": "me", "text": "hi"}]"#)
        .expect("write bad segments file");

    let output = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--transcript-file",
            segments_path.to_str().unwrap(),
        ],
    )
    .failure();
    let stdout = stdout_of(&output);
    assert_eq!(output.get_output().status.code(), Some(1));
    assert!(stdout.contains("error:"), "got:\n{stdout}");
    assert!(
        stdout.contains("ts_ms"),
        "expected the server's field-naming message; got:\n{stdout}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_new_transcript_file_with_start_exits_2() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let segments_path = tmp.path().join("segments.json");
    std::fs::write(
        &segments_path,
        r#"[{"ts_ms": 0, "channel": "me", "text": "hi"}]"#,
    )
    .expect("write segments file");

    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.arg("ctl")
        .arg("--port")
        .arg(port.to_string())
        .args([
            "meeting",
            "new",
            "--transcript-file",
            segments_path.to_str().unwrap(),
            "--start",
        ])
        .env("HOME", tmp.path());
    let output = cmd.assert().failure();
    assert_eq!(
        output.get_output().status.code(),
        Some(2),
        "a fixture cannot be recorded into -- expected a usage-error exit"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn enhance_last_on_fixture_returns_real_output_not_too_short() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let script = repo_root().join("scripts/eval/conversation.txt");
    ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--from-script",
            script.to_str().unwrap(),
            "--title",
            "fixture enhance",
        ],
    )
    .success();

    let enhanced = ctl(port, tmp.path(), &["meeting", "enhance", "last", "--json"]).success();
    let stdout = stdout_of(&enhanced);
    let body: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(
        body["too_short"].as_bool(),
        Some(false),
        "expected real enhance output, not too_short; got:\n{stdout}"
    );
    // `MockLlm::build_mock_output` (crates/yogurt-server/src/llm_mock.rs)
    // emits one `data-ai-grey` bullet per transcript segment -- a
    // deterministic marker that enhance actually ran over the fixture's
    // transcript rather than short-circuiting.
    let enriched = body["enriched_md"].as_str().expect("enriched_md field");
    assert!(
        enriched.contains("data-ai-grey"),
        "expected the mock LLM's marker bullets; got:\n{enriched}"
    );
}

/// LLM-8: the End meeting path, headless. The browser hands enhance the
/// notes and an EMPTY transcript and relies on the row for what was said;
/// `--notes-file` + `--transcript-file` stage that exact row and
/// `ctl meeting enhance` (which sends the row's own notes and transcript)
/// drives it. Asserts the notes survive exactly once and the transcript
/// actually produced AI bullets - the two things the report said went
/// wrong.
#[tokio::test(flavor = "multi_thread")]
async fn enhance_on_seeded_notes_and_transcript_keeps_notes_once_and_adds_ai_bullets() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let notes_path = tmp.path().join("notes.md");
    std::fs::write(&notes_path, "- pricing decision\n- 247k\n").expect("write notes");
    let segments_path = tmp.path().join("segments.json");
    std::fs::write(
        &segments_path,
        r#"[
            {"ts_ms": 40000, "channel": "them", "text": "we would like to offer a base salary of 247k"},
            {"ts_ms": 57000, "channel": "them", "text": "the equity grant follows the quarterly vesting schedule"}
        ]"#,
    )
    .expect("write segments");

    let created = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--notes-file",
            notes_path.to_str().unwrap(),
            "--transcript-file",
            segments_path.to_str().unwrap(),
            "--title",
            "end meeting path",
            "--json",
        ],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    let shown = ctl(port, tmp.path(), &["meeting", "show", &id, "--json"]).success();
    let shown: serde_json::Value = serde_json::from_str(&stdout_of(&shown)).unwrap();
    assert_eq!(
        shown["meeting"]["notes_md"].as_str(),
        Some("- pricing decision\n- 247k\n"),
        "notes seeded on the row: {shown}"
    );
    assert!(
        shown["meeting"]["enriched_md"].is_null(),
        "not enhanced yet: {shown}"
    );

    let enhanced = ctl(port, tmp.path(), &["meeting", "enhance", "last", "--json"]).success();
    let body: serde_json::Value = serde_json::from_str(&stdout_of(&enhanced)).expect("valid JSON");
    assert_eq!(body["too_short"].as_bool(), Some(false), "got: {body}");
    let enriched = body["enriched_md"].as_str().expect("enriched_md field");
    assert_eq!(
        enriched.matches("pricing decision").count(),
        1,
        "each note survives exactly once. got:\n{enriched}"
    );
    assert_eq!(enriched.matches("247k").count(), 1, "got:\n{enriched}");
    assert!(
        enriched.contains(r#"data-ai-grey="" data-ts="40""#)
            && enriched.contains(r#"data-ai-grey="" data-ts="57""#),
        "one AI bullet per transcript segment, stamped with its time. got:\n{enriched}"
    );

    // What the browser reads back is the same document.
    let summary = ctl(port, tmp.path(), &["meeting", "summary", &id]).success();
    let summary = stdout_of(&summary);
    assert!(summary.contains("pricing decision"), "got:\n{summary}");
    assert!(summary.contains("base salary"), "got:\n{summary}");
}

// ─── DX-1: real-binary smoke suite gaps (hardware-free) ─────────────────

#[tokio::test(flavor = "multi_thread")]
async fn status_on_fresh_instance_reports_no_active_or_detected_meeting() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let output = ctl(port, tmp.path(), &["status", "--json"]).success();
    let body: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("valid JSON");
    assert_eq!(body["active_meeting"], serde_json::Value::Null);
    assert_eq!(body["detected_meeting"], serde_json::Value::Null);

    let text = ctl(port, tmp.path(), &["status"]).success();
    let stdout = stdout_of(&text);
    assert!(stdout.contains("active meeting: none"), "got:\n{stdout}");
    assert!(stdout.contains("detected meeting: none"), "got:\n{stdout}");
}

#[tokio::test(flavor = "multi_thread")]
async fn detect_on_fresh_instance_reports_nothing_without_hardware() {
    // `detect` reads server-tracked state (MTG-11's polling loop), not a
    // live SCK scan -- see `detect_cmd.rs`'s module doc comment -- so a
    // fresh instance answers "nothing detected" with no permission grant
    // needed.
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let output = ctl(port, tmp.path(), &["detect", "--json"]).success();
    let body: serde_json::Value = serde_json::from_str(&stdout_of(&output)).expect("valid JSON");
    assert_eq!(body["detected"], serde_json::Value::Null);

    let text = ctl(port, tmp.path(), &["detect"]).success();
    assert!(
        stdout_of(&text).contains("nothing detected"),
        "got:\n{}",
        stdout_of(&text)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_on_never_started_meeting_is_a_noop_and_stamps_ended_at() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let created = ctl(
        port,
        tmp.path(),
        &["meeting", "new", "--title", "never started", "--json"],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    let output = ctl(port, tmp.path(), &["meeting", "stop", &id]).success();
    assert_eq!(output.get_output().status.code(), Some(0));
    assert!(
        stdout_of(&output).contains(&format!("stopped {id}")),
        "got:\n{}",
        stdout_of(&output)
    );

    let shown = ctl(port, tmp.path(), &["meeting", "show", &id, "--json"]).success();
    let shown: serde_json::Value = serde_json::from_str(&stdout_of(&shown)).unwrap();
    assert!(
        shown["meeting"]["ended_at"].is_number(),
        "expected ended_at stamped even though the meeting never started: {shown}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn enhance_last_on_empty_meeting_returns_too_short() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    ctl(
        port,
        tmp.path(),
        &["meeting", "new", "--title", "empty", "--json"],
    )
    .success();

    let output = ctl(port, tmp.path(), &["meeting", "enhance", "last", "--json"]).success();
    let body: serde_json::Value = serde_json::from_str(&stdout_of(&output)).unwrap();
    assert_eq!(
        body["too_short"].as_bool(),
        Some(true),
        "expected too_short on an empty meeting (MockLlm path); got:\n{body}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn summary_is_front_matter_only_before_enhance_and_gains_content_after() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let script = repo_root().join("scripts/eval/conversation.txt");
    let created = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--from-script",
            script.to_str().unwrap(),
            "--title",
            "summary fixture",
            "--json",
        ],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    // Before enhance: `notes_md` is empty, so the exported markdown file is
    // the YAML front-matter envelope with no body.
    let before = ctl(port, tmp.path(), &["meeting", "summary", &id]).success();
    let before = stdout_of(&before);
    assert!(
        before.contains("id:") && before.contains("title:"),
        "expected front matter; got:\n{before}"
    );
    assert!(
        !before.contains('\u{21b3}'),
        "expected no enhance content before enhance ran; got:\n{before}"
    );

    ctl(port, tmp.path(), &["meeting", "enhance", &id]).success();

    // After enhance: the deep-link marker (`render::wrap_ai`'s
    // `↳ HH:MM`) is visible text, not inside a tag, so it survives
    // `ctl`'s `strip_tags` -- a deterministic signal that this is the
    // real enhanced body and not still the front-matter-only stub.
    let after = ctl(port, tmp.path(), &["meeting", "summary", &id]).success();
    let after = stdout_of(&after);
    assert!(
        after.contains("id:") && after.contains("title:"),
        "expected front matter to still be present; got:\n{after}"
    );
    assert!(
        after.contains('\u{21b3}'),
        "expected enhanced content (deep-link marker) after enhance; got:\n{after}"
    );
}

// ─── CLI-6: second slice (settings, provider, models, ws, meeting mute/search/delete) ───
//
// `provider list`/`settings get` non-leak coverage lives in
// `no_subcommand_reveals_or_sets_a_key_or_token` above (it already spawns a
// server and reads the real session token, so seeding a fake provider key
// there is cheaper than a second server boot). `ws` and a real `models
// download` are not exercised here -- see the module doc comment and D4 in
// docs/.planning/agent-workflow.md: `ws` needs a live stream to assert
// anything about, and `mute`'s real toggle-and-reflect path needs an
// actual recording, both hardware-adjacent and covered by hand (PR body)
// or by the separate `just test-hw` suite, not this hermetic one.

#[tokio::test(flavor = "multi_thread")]
async fn settings_get_set_round_trip_and_rejects_a_bad_value() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let got = ctl(port, tmp.path(), &["settings", "get", "--json"]).success();
    let got: serde_json::Value = serde_json::from_str(&stdout_of(&got)).unwrap();
    assert_eq!(
        got["meeting_detection"], true,
        "default per yogurt_db::settings::load_general (no seed row)"
    );

    let set = ctl(
        port,
        tmp.path(),
        &["settings", "set", "meeting_detection=false", "--json"],
    )
    .success();
    let set: serde_json::Value = serde_json::from_str(&stdout_of(&set)).unwrap();
    assert_eq!(set["meeting_detection"], false);

    let got2 = ctl(port, tmp.path(), &["settings", "get", "--json"]).success();
    let got2: serde_json::Value = serde_json::from_str(&stdout_of(&got2)).unwrap();
    assert_eq!(
        got2["meeting_detection"], false,
        "the set above should have persisted"
    );

    // A well-formed but invalid value: the server's `validate_stt_patch`
    // (crates/yogurt-server/src/api/settings.rs) rejects `local` STT
    // pointed at a model that isn't downloaded. An unregistered model
    // name guarantees this 422s in any environment, including a
    // developer machine that has `brew install`ed a real model package
    // (`yogurt_stt::models::lookup` finds nothing for a name that was
    // never in the registry to begin with, real or brew-installed).
    let rejected = ctl(
        port,
        tmp.path(),
        &[
            "settings",
            "set",
            "stt_provider=local",
            "stt_model=definitely-not-a-real-model",
        ],
    )
    .failure();
    let stdout = stdout_of(&rejected);
    assert_eq!(rejected.get_output().status.code(), Some(1));
    assert!(stdout.contains("error:"), "got:\n{stdout}");
    assert!(stdout.contains("not downloaded"), "got:\n{stdout}");
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_set_dry_run_sends_nothing() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let out = ctl(
        port,
        tmp.path(),
        &[
            "settings",
            "set",
            "meeting_detection=false",
            "--dry-run",
            "--json",
        ],
    )
    .success();
    let out: serde_json::Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    assert_eq!(out["dry_run"], true);

    let got = ctl(port, tmp.path(), &["settings", "get", "--json"]).success();
    let got: serde_json::Value = serde_json::from_str(&stdout_of(&got)).unwrap();
    assert_eq!(
        got["meeting_detection"], true,
        "--dry-run must not have changed anything"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn models_list_reports_the_whisper_registry() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let out = ctl(port, tmp.path(), &["models", "list", "--json"]).success();
    let out: serde_json::Value = serde_json::from_str(&stdout_of(&out)).unwrap();
    let models = out["models"].as_array().expect("models array");
    assert!(
        !models.is_empty(),
        "expected the whisper.cpp registry to be non-empty"
    );
    let names: Vec<&str> = models.iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"tiny.en"), "got: {names:?}");
    for m in models {
        assert!(m["downloaded"].is_boolean(), "got: {m}");
        assert!(m["size_mb"].as_u64().unwrap() > 0, "got: {m}");
    }
    // Not asserted: `downloaded == false` for every model. A developer
    // machine with a real `brew install yogurt-model-*` package resolves
    // as downloaded regardless of `$HOME` (`resolve_model` also checks the
    // Homebrew prefixes, crates/yogurt-stt/src/models.rs), so a hard
    // "nothing installed" assertion here would be a false failure on
    // exactly the machines this project ships a formula for.
}

#[tokio::test(flavor = "multi_thread")]
async fn models_download_of_an_unknown_model_fails_cleanly() {
    // The one download path this hermetic suite can assert on without
    // touching the network either way: an unrecognized model name is
    // rejected client-side (before any request reaches the server's
    // download endpoint), so it is fast and deterministic in every
    // environment. A real `--wait` download failing against an
    // unreachable mirror is the network-dependent case the ticket asks
    // for -- not reproduced here because this environment (and
    // potentially CI) has outbound network access to the real mirrors,
    // so asserting on that path would either be flaky or attempt an
    // actual multi-MB download in CI; verified once by hand instead (see
    // the PR body).
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let output = ctl(
        port,
        tmp.path(),
        &["models", "download", "not-a-real-model"],
    )
    .failure();
    let stdout = stdout_of(&output);
    assert_eq!(output.get_output().status.code(), Some(1));
    assert!(stdout.contains("error:"), "got:\n{stdout}");
    assert!(stdout.contains("no such model"), "got:\n{stdout}");
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_search_finds_a_fixture_by_a_transcript_word() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let script = repo_root().join("scripts/eval/conversation.txt");
    let created = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--from-script",
            script.to_str().unwrap(),
            "--title",
            "search fixture",
            "--json",
        ],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    // Pull a distinctive word straight out of the script's own spoken
    // lines (`A:`/`B:`) rather than hardcoding one, so this keeps testing
    // something even if the script's wording changes.
    let word = std::fs::read_to_string(&script)
        .expect("conversation.txt readable")
        .lines()
        .filter_map(|l| l.strip_prefix("A: ").or_else(|| l.strip_prefix("B: ")))
        .flat_map(str::split_whitespace)
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .find(|w| w.len() > 4 && w.chars().all(char::is_alphanumeric))
        .expect("script has at least one word longer than 4 chars")
        .to_string();

    let found = ctl(port, tmp.path(), &["meeting", "search", &word, "--json"]).success();
    let found: serde_json::Value = serde_json::from_str(&stdout_of(&found)).unwrap();
    let ids: Vec<&str> = found["meetings"]
        .as_array()
        .expect("meetings array")
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(
        ids.contains(&id.as_str()),
        "expected {id} among search results for {word:?}: {ids:?}"
    );

    // A query that matches nothing is zero results, not an error.
    let empty = ctl(
        port,
        tmp.path(),
        &["meeting", "search", "zzzznosuchword", "--json"],
    )
    .success();
    let empty: serde_json::Value = serde_json::from_str(&stdout_of(&empty)).unwrap();
    assert_eq!(empty["total"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_mute_against_a_non_recording_meeting_fails_cleanly() {
    // `POST /api/meetings/:id/mic-muted` (AUD-6) needs an active capture
    // thread to command -- it 409s ("not currently recording") against a
    // meeting that was never started, which is every meeting this
    // hardware-free suite can create. The real toggle-and-reflect path
    // needs an actual recording; see D4 in
    // docs/.planning/agent-workflow.md, which puts that under the
    // separate `just test-hw` suite, not this one. What this suite CAN
    // verify hermetically is that `ctl` surfaces the rejection cleanly
    // (not a hang, not a panic) and that retrying is safe.
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let script = repo_root().join("scripts/eval/conversation.txt");
    ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--from-script",
            script.to_str().unwrap(),
            "--title",
            "mute fixture",
        ],
    )
    .success();

    for args in [
        vec!["meeting", "mute", "on", "last"],
        vec!["meeting", "mute", "off", "last"],
    ] {
        let output = ctl(port, tmp.path(), &args).failure();
        let stdout = stdout_of(&output);
        assert_eq!(output.get_output().status.code(), Some(1), "{args:?}");
        assert!(stdout.contains("error:"), "{args:?} got:\n{stdout}");
        assert!(stdout.contains("recording"), "{args:?} got:\n{stdout}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_mute_with_no_meeting_given_and_nothing_recording_fails_cleanly() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let output = ctl(port, tmp.path(), &["meeting", "mute", "on"]).failure();
    let stdout = stdout_of(&output);
    assert_eq!(output.get_output().status.code(), Some(1));
    assert!(stdout.contains("error:"), "got:\n{stdout}");
    assert!(stdout.contains("no active meeting"), "got:\n{stdout}");
}

#[tokio::test(flavor = "multi_thread")]
async fn meeting_delete_dry_run_then_yes() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let (_guard, port) = spawn_server(tmp.path()).await;

    let script = repo_root().join("scripts/eval/conversation.txt");
    let created = ctl(
        port,
        tmp.path(),
        &[
            "meeting",
            "new",
            "--from-script",
            script.to_str().unwrap(),
            "--title",
            "delete fixture",
            "--json",
        ],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    // Refuses without --yes.
    let refused = ctl(port, tmp.path(), &["meeting", "delete", &id]).failure();
    assert_eq!(refused.get_output().status.code(), Some(1));
    let stdout = stdout_of(&refused);
    assert!(
        stdout.contains("error:") && stdout.contains("--yes"),
        "got:\n{stdout}"
    );

    // --dry-run deletes nothing.
    let dry = ctl(
        port,
        tmp.path(),
        &["meeting", "delete", &id, "--dry-run", "--json"],
    )
    .success();
    let dry: serde_json::Value = serde_json::from_str(&stdout_of(&dry)).unwrap();
    assert_eq!(dry["dry_run"], true);
    assert_eq!(dry["title"], "delete fixture");
    ctl(port, tmp.path(), &["meeting", "show", &id]).success();

    // --yes deletes for real; a second `show` then fails (the row is gone).
    ctl(port, tmp.path(), &["meeting", "delete", &id, "--yes"]).success();
    let shown = ctl(port, tmp.path(), &["meeting", "show", &id]).failure();
    assert_eq!(shown.get_output().status.code(), Some(1));
}

// ─── DX-1: real-binary smoke suite, hardware path ────────────────────────
//
// Gated two ways so these never run by accident: `#[ignore]`, and an
// explicit `YOGURT_HW_TESTS=1` check at the top of each test that prints
// a reason and returns early when unset. `just test-hw` sets the env var
// and passes `-- --ignored`; `just test` and CI do neither, so these
// never execute there. Each test still spawns its server with a temp
// `HOME` (same as every other test in this file), so it touches only a
// throwaway `~/.yogurt` -- the real one is never written to.
//
// `ctl meeting mute on` is NOT covered here: no `mute` subcommand exists
// on `ctl` yet (CLI-6, not landed).

fn hw_tests_enabled() -> bool {
    if std::env::var("YOGURT_HW_TESTS").is_err() {
        eprintln!("skipping: set YOGURT_HW_TESTS=1 to run this hardware test (see `just test-hw`)");
        return false;
    }
    true
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "hardware: needs a Screen Recording grant -- run via `just test-hw`"]
async fn hw_windows_reports_rows_or_denied() {
    if !hw_tests_enabled() {
        return;
    }
    let tmp = tempfile::tempdir().expect("tempdir");

    // `windows` needs no server -- it's an in-process SCK scan.
    let mut cmd = Command::cargo_bin("yogurt").expect("binary exists");
    cmd.args(["ctl", "windows", "--json"])
        .env("HOME", tmp.path());
    let output = cmd.output().expect("run yogurt ctl windows");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    eprintln!(
        "hw: ctl windows --json -> exit {:?}: {stdout}",
        output.status.code()
    );

    if output.status.success() {
        let rows: serde_json::Value =
            serde_json::from_str(stdout.trim()).expect("windows --json prints a JSON array");
        assert!(
            rows.is_array(),
            "expected a JSON array of windows; got {rows}"
        );
    } else {
        assert_eq!(output.status.code(), Some(1));
        assert!(
            stdout.contains("screen recording: denied"),
            "expected the denied message on failure; got:\n{stdout}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "hardware: opens a real SCK + mic capture pipeline -- run via `just test-hw`"]
async fn hw_meeting_start_stamps_stt_engine_then_stop_closes_pipeline() {
    if !hw_tests_enabled() {
        return;
    }

    // Local whisper needs a real model file. Reuse whatever the developer
    // already downloaded to the REAL ~/.yogurt/models (same convention as
    // whisper_smoke.rs) rather than fetching one -- symlink the model and
    // its `.sha256` marker into the test's throwaway HOME so
    // `is_downloaded` resolves instantly with no multi-GB hash.
    let spec = yogurt_stt::models::lookup("small.en").expect("small.en in registry");
    let real_model = match yogurt_stt::models::model_path(spec) {
        Ok(p) if p.exists() => p,
        _ => {
            eprintln!(
                "skipping: ~/.yogurt/models/{} not downloaded -- download it in Settings > \
                 Transcription first",
                spec.filename
            );
            return;
        }
    };

    let tmp = tempfile::tempdir().expect("tempdir");
    let models_dir = tmp.path().join(".yogurt").join("models");
    std::fs::create_dir_all(&models_dir).expect("mkdir models dir");
    std::os::unix::fs::symlink(&real_model, models_dir.join(spec.filename)).expect("symlink model");
    let real_marker = yogurt_stt::models::marker_path(&real_model);
    if real_marker.exists() {
        std::os::unix::fs::symlink(
            &real_marker,
            models_dir.join(format!("{}.sha256", spec.filename)),
        )
        .expect("symlink marker");
    }

    let (_guard, port) = spawn_server(tmp.path()).await;
    let token = std::fs::read_to_string(tmp.path().join(".yogurt/session-token"))
        .expect("session-token exists")
        .trim()
        .to_string();
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .build()
        .expect("build reqwest client");

    // Switch this instance to local whisper -- the default is "cloud",
    // which needs a Deepgram key this test has none of (AGENTS.md: audio
    // stays on-device unless the user opted into cloud STT).
    let patch = http
        .patch(format!("http://127.0.0.1:{port}/api/settings"))
        .bearer_auth(&token)
        .json(&serde_json::json!({ "stt_provider": "local", "stt_model": "small.en" }))
        .send()
        .await
        .expect("PATCH /api/settings");
    assert!(
        patch.status().is_success(),
        "PATCH /api/settings: {:?}",
        patch.status()
    );

    let created = ctl(
        port,
        tmp.path(),
        &["meeting", "new", "--title", "hw smoke", "--json"],
    )
    .success();
    let created: serde_json::Value = serde_json::from_str(&stdout_of(&created)).unwrap();
    let id = created["id"].as_str().expect("id field").to_string();

    // A wall-clock bound on every hardware-facing call -- this test must
    // never hang the suite (that is the whole reason `just test-hw` is
    // its own opt-in recipe rather than something a background `cargo
    // test` could stumble into). `ctl_run_bounded` kills and reports
    // instead of blocking forever.
    const HW_CALL_BOUND: Duration = Duration::from_secs(45);

    // Exercise + assert without panicking, so the cleanup below always
    // runs -- this is the "capture pipeline opened and closed" check
    // MTG-11 could not machine-verify, and it must never leave a live
    // recording or a stray meeting behind even if an assertion fails.
    let exercise: Result<(), String> = async {
        let start_out =
            ctl_run_bounded(port, tmp.path(), &["meeting", "start", &id], HW_CALL_BOUND)?;
        if !start_out.status.success() {
            return Err(format!(
                "start failed: {}",
                String::from_utf8_lossy(&start_out.stdout)
            ));
        }
        eprintln!(
            "hw: ctl meeting start -> {}",
            String::from_utf8_lossy(&start_out.stdout)
        );

        // Give the capture thread a moment to actually open before we
        // read back `stt_engine`.
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        let shown_out = ctl_run_bounded(
            port,
            tmp.path(),
            &["meeting", "show", &id, "--json"],
            HW_CALL_BOUND,
        )?;
        if !shown_out.status.success() {
            return Err(format!(
                "show failed: {}",
                String::from_utf8_lossy(&shown_out.stdout)
            ));
        }
        let shown: serde_json::Value =
            serde_json::from_slice(&shown_out.stdout).map_err(|e| format!("bad show JSON: {e}"))?;
        let stt_engine = shown["meeting"]["stt_engine"].as_str().unwrap_or("unknown");
        if stt_engine == "unknown" {
            return Err(format!("expected stt_engine stamped; got: {shown}"));
        }
        eprintln!("hw: stt_engine stamped as {stt_engine:?}");
        Ok(())
    }
    .await;

    let stop_result = ctl_run_bounded(port, tmp.path(), &["meeting", "stop", &id], HW_CALL_BOUND);
    match &stop_result {
        Ok(o) => eprintln!(
            "hw: ctl meeting stop -> {}",
            String::from_utf8_lossy(&o.stdout)
        ),
        Err(e) => eprintln!("hw: ctl meeting stop -> ERROR: {e}"),
    }

    let shown_after_stop: Option<serde_json::Value> = ctl_run_bounded(
        port,
        tmp.path(),
        &["meeting", "show", &id, "--json"],
        HW_CALL_BOUND,
    )
    .ok()
    .and_then(|o| serde_json::from_slice(&o.stdout).ok());
    let ended_at_stamped = shown_after_stop
        .as_ref()
        .is_some_and(|m| m["meeting"]["ended_at"].is_number());
    eprintln!("hw: ended_at stamped after stop -> {ended_at_stamped}");

    let delete = http
        .delete(format!(
            "http://127.0.0.1:{port}/api/meetings/{id}?delete_file=true"
        ))
        .bearer_auth(&token)
        .send()
        .await;
    eprintln!("hw: cleanup delete -> {delete:?}");

    if let Err(msg) = exercise {
        panic!("{msg}");
    }
    assert!(ended_at_stamped, "expected ended_at stamped after stop");
}
