use yogurt_db::keychain::{ApiKeyStore, MemoryKeyStore};

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
