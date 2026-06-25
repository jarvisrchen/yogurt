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

    // (d) Phase 0 must NOT contain enriched_doc_json — that's Phase 4.
    let meetings_cols: Vec<String> = conn
        .prepare("PRAGMA table_info(meetings)")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(
        !meetings_cols.contains(&"enriched_doc_json".to_string()),
        "enriched_doc_json must be deferred to Phase 4; current cols: {meetings_cols:?}"
    );

    // (e) Idempotency: a second init on the same path must succeed.
    let _again = Storage::init_at(&db_path).expect("second init is idempotent");
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

        let write_attempt =
            conn.execute("DELETE FROM meetings", []);
        assert!(
            write_attempt.is_err(),
            "read connection should reject writes (query_only=ON)"
        );
    }
}
