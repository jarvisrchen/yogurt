use yogurt_db::keys::{ApiKeyStore, MemoryKeyStore};

#[test]
fn memory_store_roundtrips() {
    let store = MemoryKeyStore::default();
    assert_eq!(store.get("prov_abc").unwrap(), None);
    store.set("prov_abc", "sk-test-1234").unwrap();
    assert_eq!(
        store.get("prov_abc").unwrap().as_deref(),
        Some("sk-test-1234")
    );
    store.delete("prov_abc").unwrap();
    assert_eq!(store.get("prov_abc").unwrap(), None);
}

#[test]
fn memory_store_returns_masked_last_four() {
    let store = MemoryKeyStore::default();
    store.set("prov_abc", "sk-supersecret-9876").unwrap();
    let mask = store.masked("prov_abc").unwrap();
    assert_eq!(mask.as_deref(), Some("••••9876"));
}

#[test]
fn masked_returns_none_when_no_key() {
    let store = MemoryKeyStore::default();
    assert_eq!(store.masked("prov_xyz").unwrap(), None);
}

#[cfg(unix)]
#[test]
fn file_store_persists_with_0600_and_survives_reopen() {
    use std::os::unix::fs::PermissionsExt;
    use yogurt_db::keys::FileKeyStore;

    let dir = std::env::temp_dir().join(format!("yogurt-keys-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("keys.json");
    let _ = std::fs::remove_file(&path);

    let store = FileKeyStore::open(&path).unwrap();
    assert_eq!(store.get("a").unwrap(), None);
    assert!(!path.exists(), "no file until the first write");

    store.set("a", "sk-one-1111").unwrap();
    store.set("b", "sk-two-2222").unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);

    store.delete("a").unwrap();
    drop(store);

    let reopened = FileKeyStore::open(&path).unwrap();
    assert_eq!(reopened.get("a").unwrap(), None);
    assert_eq!(reopened.get("b").unwrap().as_deref(), Some("sk-two-2222"));
    assert_eq!(reopened.masked("b").unwrap().as_deref(), Some("••••2222"));
    std::fs::remove_dir_all(&dir).unwrap();
}
