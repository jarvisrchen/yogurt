//! Session-token persistence + corruption-handling tests (BL-01, MD-05, MD-06).

use std::path::PathBuf;
use tempfile::TempDir;
use yogurt_server::session;

fn tmp_token_path() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("tempdir");
    let p = tmp.path().join("subdir").join("session-token");
    (tmp, p)
}

#[test]
fn it_generates_and_persists_a_43_char_token() {
    let (_tmp, p) = tmp_token_path();
    let tok = session::load_or_create(&p).expect("first load creates");
    assert_eq!(tok.as_str().len(), session::EXPECTED_TOKEN_LEN);
    let raw = std::fs::read_to_string(&p).expect("file exists");
    assert_eq!(raw.trim(), tok.as_str());
}

#[test]
fn it_round_trips_an_existing_token() {
    let (_tmp, p) = tmp_token_path();
    let tok1 = session::load_or_create(&p).expect("first load");
    let tok2 = session::load_or_create(&p).expect("second load reads existing");
    assert_eq!(tok1.as_str(), tok2.as_str());
}

#[test]
fn it_fails_loud_on_empty_token_file() {
    // BL-01 regression: an empty file (simulating a crash mid-write) must NOT
    // silently regenerate. Silent regeneration rotates the token and DoS's
    // every active session.
    let (_tmp, p) = tmp_token_path();
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "").expect("create empty file");

    let err = session::load_or_create(&p).expect_err("must fail on empty");
    let msg = format!("{err}");
    assert!(
        msg.contains("empty"),
        "error should mention 'empty'; got: {msg}"
    );
    // Confirm the file was NOT overwritten with a new token.
    let raw = std::fs::read_to_string(&p).unwrap();
    assert!(
        raw.is_empty(),
        "empty file must not be silently regenerated"
    );
}

#[test]
fn it_fails_loud_on_malformed_token() {
    // MD-05 regression: a short / wrong-charset token must be rejected loudly.
    let (_tmp, p) = tmp_token_path();
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "hello").expect("write malformed token");

    let err = session::load_or_create(&p).expect_err("must fail on malformed");
    let msg = format!("{err}");
    assert!(
        msg.contains("malformed"),
        "error should mention 'malformed'; got: {msg}"
    );
}

#[test]
fn it_persists_with_no_tmp_file_left_behind() {
    // BL-01: the atomic write-rename pattern must clean up its tmp file.
    let (_tmp, p) = tmp_token_path();
    let _tok = session::load_or_create(&p).expect("first load");
    let tmp_path = p.with_extension("tmp");
    assert!(
        !tmp_path.exists(),
        "tmp file should be renamed to the final path"
    );
}

#[cfg(unix)]
#[test]
fn it_creates_token_file_at_mode_0600() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, p) = tmp_token_path();
    let _tok = session::load_or_create(&p).expect("first load");
    let mode = std::fs::metadata(&p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "token file must be mode 0600");
}

#[cfg(unix)]
#[test]
fn it_creates_parent_dir_at_mode_0700() {
    // MD-06 regression: ~/.yogurt/ must NOT be world-readable.
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, p) = tmp_token_path();
    let _tok = session::load_or_create(&p).expect("first load");
    let parent = p.parent().unwrap();
    let mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "token parent dir must be mode 0700");
}

#[cfg(unix)]
#[test]
fn it_tightens_a_preexisting_loose_parent_dir() {
    // If ~/.yogurt/ already exists at 0755, load_or_create must tighten it.
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().unwrap();
    let parent = tmp.path().join("yogurt-dir");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();

    let p = parent.join("session-token");
    let _tok = session::load_or_create(&p).expect("load tightens parent");

    let mode = std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode, 0o700,
        "pre-existing loose dir must be tightened to 0700"
    );
}
