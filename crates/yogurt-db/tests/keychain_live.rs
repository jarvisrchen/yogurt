#![cfg(feature = "keychain-live")]
//! Manual integration test against the real macOS Keychain.
//!
//! Run with: `cargo test -p yogurt-db --features keychain-live -- --ignored`.
//!
//! Requires the user to approve Keychain prompts on first run. NOT run in CI.

use yogurt_db::keychain::{ApiKeyStore, KeychainStore};

#[test]
#[ignore]
fn it_roundtrips_against_real_keychain() {
    let store = KeychainStore::new().expect("KeychainStore backend init");
    let account = "yogurt-test-acct";
    store.set(account, "real-secret-XYZA").unwrap();
    assert_eq!(
        store.get(account).unwrap().as_deref(),
        Some("real-secret-XYZA")
    );
    store.delete(account).unwrap();
    assert_eq!(store.get(account).unwrap(), None);
}
