//! Integration tests for the Phase 6 `chat_messages` CRUD surface on `Db`.
//!
//! V002 of the yogurt-db migration adds `chat_messages` alongside the
//! Phase 5 providers + settings tables. Phase 6's REST handler in
//! yogurt-server is the production consumer; these tests exercise the
//! storage surface in isolation against `Db::open_in_memory()`.
//!
//! The `meetings(id)` FK target lives in `yogurt-server::storage::migrations`
//! (Phase 0). When `Db` is opened in-memory here, that table does NOT exist,
//! so the V002 migration uses `meetings(id)` as a soft reference — the FK is
//! defined but not enforced (sqlite parses FK syntax without requiring the
//! parent table to exist). In production both runners apply against the same
//! file, so the constraint is real at runtime.

use yogurt_db::chat::{ChatMessage, Role};
use yogurt_db::Db;

fn seed_meeting(db: &Db, id: &str) {
    // Create a minimal meetings table inside the in-memory db so the FK
    // (when foreign_keys=ON) has a valid parent row. Schema matches
    // Phase 0 storage::migrations exactly so insert columns line up.
    db.conn()
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT,
                started_at INTEGER NOT NULL,
                ended_at INTEGER,
                notes_md TEXT,
                enriched_md TEXT,
                transcript_json TEXT
            );
            "#,
        )
        .expect("create meetings table");
    db.conn()
        .execute(
            "INSERT OR IGNORE INTO meetings (id, title, started_at) VALUES (?, ?, ?)",
            rusqlite::params![id, "test meeting", 0i64],
        )
        .expect("seed meeting row");
}

#[test]
fn it_inserts_and_lists_messages_in_chronological_order() {
    let db = Db::open_in_memory().expect("open in-memory db");
    let meeting_id = "01HXMEETINGAAAAAAAAAAAAAAA";
    seed_meeting(&db, meeting_id);

    let m1 = ChatMessage::new(meeting_id, Role::User, "hello");
    db.insert_chat_message(&m1).expect("insert user msg");

    // Bump the next message's timestamp to guarantee chronological order
    // even when the test runs faster than 1 ms — `chrono::now()` resolution
    // can collapse two `new()` calls onto the same millisecond.
    std::thread::sleep(std::time::Duration::from_millis(2));

    let m2 = ChatMessage::new(meeting_id, Role::Assistant, "hi there");
    db.insert_chat_message(&m2).expect("insert assistant msg");

    let listed = db
        .list_chat_messages(meeting_id)
        .expect("list chat messages");
    assert_eq!(listed.len(), 2, "expected 2 messages");
    assert_eq!(listed[0].id, m1.id, "first message is the user message");
    assert_eq!(
        listed[1].id, m2.id,
        "second message is the assistant message"
    );
    assert_eq!(listed[0].role, Role::User);
    assert_eq!(listed[1].role, Role::Assistant);
    assert_eq!(listed[0].content, "hello");
    assert_eq!(listed[1].content, "hi there");

    // get_chat_message returns the exact row by id.
    let fetched = db
        .get_chat_message(&m1.id)
        .expect("get_chat_message ok")
        .expect("row exists");
    assert_eq!(fetched.content, "hello");

    // update_chat_message_content replaces content for the row.
    db.update_chat_message_content(&m2.id, "hi there, friend")
        .expect("update content");
    let after = db
        .get_chat_message(&m2.id)
        .expect("get after update")
        .expect("row exists");
    assert_eq!(after.content, "hi there, friend");
}

#[test]
fn it_scopes_messages_by_meeting() {
    let db = Db::open_in_memory().expect("open in-memory db");
    let a = "01HXMEETINGBBBBBBBBBBBBBBB";
    let b = "01HXMEETINGCCCCCCCCCCCCCCC";
    seed_meeting(&db, a);
    seed_meeting(&db, b);

    db.insert_chat_message(&ChatMessage::new(a, Role::User, "for a"))
        .unwrap();
    db.insert_chat_message(&ChatMessage::new(b, Role::User, "for b"))
        .unwrap();
    db.insert_chat_message(&ChatMessage::new(b, Role::Assistant, "reply for b"))
        .unwrap();

    let list_a = db.list_chat_messages(a).unwrap();
    let list_b = db.list_chat_messages(b).unwrap();
    assert_eq!(list_a.len(), 1, "meeting a sees only its own row");
    assert_eq!(list_a[0].content, "for a");
    assert_eq!(list_b.len(), 2, "meeting b sees its 2 rows");
    assert!(list_b.iter().all(|m| m.meeting_id == b));
}
