//! Integration tests for the SQLite storage layer.
//!
//! Uses per-test tempdirs so we never touch the developer's real `~/.yogurt/`.

use rusqlite::Connection;
use tempfile::TempDir;
use yogurt_server::Storage;

#[tokio::test]
async fn it_initializes_db_with_wal_and_tables() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");

    // First init: creates DB, sets WAL, runs migrations.
    let storage = Storage::init_at(&db_path).expect("storage init");
    // Hold the handle so the DB file stays valid through the assertions.
    drop(storage);

    // Open a fresh connection to the same file and verify state.
    let conn = Connection::open(&db_path).expect("reopen db");

    // (a) journal mode is WAL.
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .expect("query journal_mode");
    assert_eq!(mode.to_lowercase(), "wal", "expected WAL journal mode");

    // (b) both tables exist.
    let mut tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    tables.retain(|t| !t.starts_with("sqlite_"));
    assert!(
        tables.contains(&"meetings".to_string()),
        "expected meetings table, got {tables:?}"
    );
    assert!(
        tables.contains(&"chat_messages".to_string()),
        "expected chat_messages table, got {tables:?}"
    );

    // (c) both indexes exist.
    let indexes: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='index'")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        indexes.contains(&"idx_meetings_started_at".to_string()),
        "expected idx_meetings_started_at, got {indexes:?}"
    );
    assert!(
        indexes.contains(&"idx_chat_messages_meeting_id".to_string()),
        "expected idx_chat_messages_meeting_id, got {indexes:?}"
    );

    // (d) Phase 4 (STORE-01 / CONTEXT D-11): meetings table MUST contain
    //     enriched_doc_json after V0004 runs.
    let meetings_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(meetings)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        meetings_cols.contains(&"enriched_doc_json".to_string()),
        "Phase 4 V0004 must add enriched_doc_json; current cols: {meetings_cols:?}"
    );
    // Phase 0 schema columns must still be present (no regression).
    for required in ["id", "notes_md", "enriched_md", "transcript_json"] {
        assert!(
            meetings_cols.contains(&required.to_string()),
            "Phase 0 column {required} regressed; current cols: {meetings_cols:?}"
        );
    }

    // (e) Idempotency: a second init on the same path must succeed.
    //     This also exercises the V0004 guard (column already present →
    //     migration must skip the ALTER without erroring).
    let _again = Storage::init_at(&db_path).expect("second init is idempotent");
}

#[tokio::test]
async fn it_adds_enriched_doc_json_only_once_across_multiple_inits() {
    // STORE-01 / Phase 4: the V0004 migration must be idempotent — calling
    // `Storage::init_at` repeatedly must NOT raise "duplicate column" from
    // SQLite (which it would if the runner blindly re-ALTERed).
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");

    for _ in 0..3 {
        let _s = Storage::init_at(&db_path).expect("idempotent init");
    }

    let conn = Connection::open(&db_path).expect("reopen");
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('meetings') \
             WHERE name = 'enriched_doc_json'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1, "enriched_doc_json must appear exactly once");
}

#[cfg(unix)]
#[tokio::test]
async fn it_creates_db_file_at_mode_0600_and_parent_at_0700() {
    // MD-06 regression: SQLite file + ~/.yogurt/ must not be world-readable.
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().expect("tempdir");
    let parent = tmp.path().join("yogurt-storage");
    let db_path = parent.join("db.sqlite");

    let _storage = Storage::init_at(&db_path).expect("init");

    let dir_mode = std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "parent dir must be 0700");

    let file_mode = std::fs::metadata(&db_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(file_mode, 0o600, "sqlite db file must be 0600");
}

#[cfg(unix)]
#[tokio::test]
async fn it_tightens_a_preexisting_loose_storage_parent_dir() {
    // MD-06: if ~/.yogurt/ pre-exists at 0755, init_at must tighten to 0700.
    use std::os::unix::fs::PermissionsExt;
    let tmp = TempDir::new().expect("tempdir");
    let parent = tmp.path().join("preexisting");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let db_path = parent.join("db.sqlite");

    let _storage = Storage::init_at(&db_path).expect("init tightens parent");

    let dir_mode = std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        dir_mode, 0o700,
        "loose parent dir must be tightened to 0700"
    );
}

#[tokio::test]
async fn it_applies_wal_pragma_to_read_pool_connections() {
    // HI-02 regression: every read connection must explicitly set
    // journal_mode=WAL. We can't introspect the pool directly, but we can
    // verify reads work after init -- and combined with the in-storage
    // pragma_update assertion in init_at, that's sufficient.
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");
    let storage = Storage::init_at(&db_path).expect("init");

    // Drive a read through the pool to prove the connection works after
    // all pragmas were applied.
    let r = storage.read();
    let conn = r.lock().unwrap();
    let mode: String = conn
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .expect("read pragma from read-pool conn");
    assert_eq!(
        mode.to_lowercase(),
        "wal",
        "read-pool connection must report WAL journal mode"
    );
}

#[tokio::test]
async fn it_exposes_both_read_and_writer_handles() {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("db.sqlite");
    let storage = Storage::init_at(&db_path).expect("storage init");

    // Writer is usable for writes.
    {
        let w = storage.writer();
        let conn = w.lock().unwrap();
        conn.execute(
            "INSERT INTO meetings (id, started_at) VALUES (?1, ?2)",
            rusqlite::params!["m_test_1", 1_700_000_000_i64],
        )
        .expect("insert via writer");
    }

    // Read connection sees the row, and rejects writes (query_only=ON).
    {
        let r = storage.read();
        let conn = r.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM meetings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let write_attempt = conn.execute("DELETE FROM meetings", []);
        assert!(
            write_attempt.is_err(),
            "read connection should reject writes (query_only=ON)"
        );
    }
}
