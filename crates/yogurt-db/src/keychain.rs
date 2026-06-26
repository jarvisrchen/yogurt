//! Keychain wrapper: the `ApiKeyStore` trait + production `KeychainStore`
//! (macOS Keychain via the `keyring` crate) + in-memory `MemoryKeyStore`
//! for tests.
//!
//! All Keychain entries are namespaced under `service="yogurt"` (the
//! [`SERVICE`] constant). Account name = provider ULID. This ensures
//! `brew uninstall && brew install` doesn't leak keys across reinstalls.
//!
//! ## Why the trait?
//!
//! `keyring::Entry::get_password()` touches the real macOS Keychain on every
//! call and prompts the user the first time. Production handlers must NEVER
//! block on it during a request (SET-10 cold-boot mitigation). Tests need a
//! deterministic fake. The trait makes both possible from the same code path.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Mutex;

/// All Keychain entries are namespaced under this service name.
pub const SERVICE: &str = "yogurt";

/// Storage abstraction over the macOS Keychain.
///
/// Implementations MUST be cheap to clone (or wrapped in `Arc`) since
/// `AppState` holds `Arc<dyn ApiKeyStore>` and is cloned per-request.
pub trait ApiKeyStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;

    /// Returns `Some("••••XXXX")` (last 4 chars) when a key exists, `None`
    /// otherwise. Default impl is implemented in terms of `get`.
    ///
    /// The `••••` prefix is the unicode bullet (U+2022) × 4. This is the
    /// canonical mask format used by the Settings UI footer.
    fn masked(&self, account: &str) -> Result<Option<String>> {
        Ok(self.get(account)?.map(|s| mask_suffix(&s)))
    }
}

/// Mask a secret as `"••••XXXX"` where `XXXX` is the last 4 unicode chars.
/// For secrets shorter than 4 chars, returns the whole secret prefixed by
/// the bullets (so the caller can still detect "key set" vs "no key").
fn mask_suffix(secret: &str) -> String {
    let collected: Vec<char> = secret.chars().collect();
    let n = collected.len();
    let tail: String = if n >= 4 {
        collected[n - 4..].iter().collect()
    } else {
        collected.iter().collect()
    };
    format!("••••{tail}")
}

/// In-memory fake for tests and CI. NEVER use in production.
#[derive(Default)]
pub struct MemoryKeyStore {
    inner: Mutex<HashMap<String, String>>,
}

impl ApiKeyStore for MemoryKeyStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.inner.lock().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.inner
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.inner.lock().unwrap().remove(account);
        Ok(())
    }
}

/// Real macOS Keychain implementation. All calls go through the `keyring`
/// crate, which synchronously prompts the user on first access.
///
/// # Backend init (Phase 5 BLOCKER fix)
///
/// `keyring` 3.6.x silently no-op'd `set_password()` on macOS in 2026
/// toolchains — set returned `Ok` but nothing landed in Keychain. The
/// `keyring` crate refactored in 2026: high-level surface stayed in
/// `keyring`/`keyring-core`, but the actual platform backends moved
/// into separate crates (`apple-native-keyring-store` for macOS).
///
/// We bump to `keyring = "4"` AND explicitly register
/// `apple_native_keyring_store::keychain::Store` as the default
/// credential store via `keyring_core::set_default_store`. The keyring
/// 4 `v1` default feature would also init this on first `Entry::new`
/// via an internal `Once`, but doing it explicitly in our constructor
/// (idempotent under `OnceLock`) is strictly more reliable and surfaces
/// any backend-init error eagerly.
pub struct KeychainStore {
    // Marker — the real work is the one-shot init in `new()`. Field is
    // private and zero-sized so future evolution (e.g., holding a
    // per-instance store handle) doesn't break the API surface.
    _priv: (),
}

#[cfg(target_os = "macos")]
fn init_macos_backend() -> Result<()> {
    use std::sync::OnceLock;
    static INIT: OnceLock<Result<(), String>> = OnceLock::new();
    let outcome = INIT.get_or_init(|| {
        // `Store::new()` returns a boxed credential store ready to
        // register with keyring-core. Errors here are catastrophic
        // (Keychain API unavailable) — bail loudly so we don't silently
        // fall through to a broken backend.
        match apple_native_keyring_store::keychain::Store::new() {
            Ok(store) => {
                keyring_core::set_default_store(store);
                tracing::info!(
                    "Registered apple-native-keyring-store as default credential backend"
                );
                Ok(())
            }
            Err(e) => Err(format!("apple-native-keyring-store init failed: {e}")),
        }
    });
    match outcome {
        Ok(()) => Ok(()),
        Err(msg) => Err(anyhow::anyhow!("{msg}")),
    }
}

#[cfg(not(target_os = "macos"))]
fn init_macos_backend() -> Result<()> {
    // Non-macOS (CI Linux): keyring 4's `v1` default feature handles
    // backend selection (Secret Service / linux-keyutils) on first
    // Entry::new. Nothing to do here.
    Ok(())
}

impl KeychainStore {
    /// Construct a KeychainStore. On macOS, performs a one-shot
    /// registration of the apple-native-keyring-store backend with
    /// keyring-core (idempotent across all calls process-wide).
    pub fn new() -> Result<Self> {
        init_macos_backend()?;
        Ok(Self { _priv: () })
    }
}

impl ApiKeyStore for KeychainStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        entry.set_password(secret)?;
        Ok(())
    }
    fn delete(&self, account: &str) -> Result<()> {
        let entry = keyring::Entry::new(SERVICE, account)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
