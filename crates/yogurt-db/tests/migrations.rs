use yogurt_db::Db;

#[test]
fn it_runs_migrations_on_fresh_in_memory_db() {
    let db = Db::open_in_memory().expect("open in-memory db");
    let port: String = db
        .conn()
        .query_row(
            "SELECT value FROM settings WHERE key = 'general.port'",
            [],
            |r| r.get(0),
        )
        .expect("seeded port row");
    assert_eq!(port, "7878");
}

#[test]
fn it_is_idempotent_to_open_twice() {
    let db1 = Db::open_in_memory().expect("first open");
    drop(db1);
    let db2 = Db::open_in_memory().expect("second open");
    // re-running migrations on the same conn should be a no-op
    db2.run_migrations().expect("re-running migrations is safe");
}
