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
use std::sync::{Arc, Mutex, RwLock};

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

/// Write-through in-memory cache over an inner `ApiKeyStore`.
///
/// Dev-rebuild mitigation: unsigned debug binaries get a new code identity
/// every rebuild, so macOS can never remember an "Always Allow" grant for
/// them. `SessionCacheKeyStore` means the real Keychain (`KeychainStore`) is
/// only ever hit once per key per process — after a `set()` (key pasted in
/// Settings) or the first successful `get()` (key seeded via `.env.local` at
/// boot, or read from Keychain on a fresh boot), every later read for that
/// account is served from memory for the rest of the process's lifetime.
///
/// `get()` only caches successful `Some(_)` reads from the inner store. A
/// miss or an error is NOT cached, so a denied/hung Keychain read stays
/// retryable on the next call instead of being pinned to a false negative.
pub struct SessionCacheKeyStore {
    inner: Arc<dyn ApiKeyStore>,
    cache: RwLock<HashMap<String, String>>,
}

impl SessionCacheKeyStore {
    pub fn new(inner: Arc<dyn ApiKeyStore>) -> Self {
        Self {
            inner,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

impl ApiKeyStore for SessionCacheKeyStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        if let Some(cached) = self.cache.read().unwrap().get(account) {
            return Ok(Some(cached.clone()));
        }
        let fetched = self.inner.get(account)?;
        if let Some(secret) = &fetched {
            self.cache
                .write()
                .unwrap()
                .insert(account.to_string(), secret.clone());
        }
        Ok(fetched)
    }

    fn set(&self, account: &str, secret: &str) -> Result<()> {
        // Cache first so a concurrent read on another thread sees the new
        // value immediately; the inner write happening second is fine since
        // errors from it propagate to the caller as before.
        self.cache
            .write()
            .unwrap()
            .insert(account.to_string(), secret.to_string());
        self.inner.set(account, secret)
    }

    fn delete(&self, account: &str) -> Result<()> {
        self.cache.write().unwrap().remove(account);
        self.inner.delete(account)
    }

    fn masked(&self, account: &str) -> Result<Option<String>> {
        if let Some(cached) = self.cache.read().unwrap().get(account) {
            return Ok(Some(mask_suffix(cached)));
        }
        self.inner.masked(account)
    }
}

#[cfg(test)]
mod session_cache_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Probe inner store: counts calls so tests can assert the cache
    /// actually short-circuits reads instead of just happening to return
    /// the right value.
    #[derive(Default)]
    struct ProbeStore {
        get_calls: AtomicUsize,
        set_calls: AtomicUsize,
        delete_calls: AtomicUsize,
        data: Mutex<HashMap<String, String>>,
    }

    impl ApiKeyStore for ProbeStore {
        fn get(&self, account: &str) -> Result<Option<String>> {
            self.get_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.data.lock().unwrap().get(account).cloned())
        }
        fn set(&self, account: &str, secret: &str) -> Result<()> {
            self.set_calls.fetch_add(1, Ordering::SeqCst);
            self.data
                .lock()
                .unwrap()
                .insert(account.to_string(), secret.to_string());
            Ok(())
        }
        fn delete(&self, account: &str) -> Result<()> {
            self.delete_calls.fetch_add(1, Ordering::SeqCst);
            self.data.lock().unwrap().remove(account);
            Ok(())
        }
    }

    #[test]
    fn set_then_get_never_touches_inner() {
        let probe = Arc::new(ProbeStore::default());
        let cache = SessionCacheKeyStore::new(probe.clone());

        cache.set("acct", "sk-secret-1234").unwrap();
        let got = cache.get("acct").unwrap();

        assert_eq!(got.as_deref(), Some("sk-secret-1234"));
        assert_eq!(
            probe.get_calls.load(Ordering::SeqCst),
            0,
            "get after set must be served from cache, never the inner store"
        );
        assert_eq!(probe.set_calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn get_miss_falls_through_then_caches_the_hit() {
        let probe = Arc::new(ProbeStore::default());
        probe
            .data
            .lock()
            .unwrap()
            .insert("acct".to_string(), "from-inner".to_string());
        let cache = SessionCacheKeyStore::new(probe.clone());

        let first = cache.get("acct").unwrap();
        assert_eq!(first.as_deref(), Some("from-inner"));
        assert_eq!(probe.get_calls.load(Ordering::SeqCst), 1);

        // Second read for the same account must be served from cache.
        let second = cache.get("acct").unwrap();
        assert_eq!(second.as_deref(), Some("from-inner"));
        assert_eq!(
            probe.get_calls.load(Ordering::SeqCst),
            1,
            "successful Some() reads must be cached after the first hit"
        );
    }

    #[test]
    fn get_miss_on_unknown_account_stays_uncached() {
        let probe = Arc::new(ProbeStore::default());
        let cache = SessionCacheKeyStore::new(probe.clone());

        assert_eq!(cache.get("ghost").unwrap(), None);
        assert_eq!(cache.get("ghost").unwrap(), None);
        assert_eq!(
            probe.get_calls.load(Ordering::SeqCst),
            2,
            "misses must not be cached — every miss retries the inner store"
        );
    }

    #[test]
    fn masked_from_cache_matches_inner_format() {
        let probe = Arc::new(ProbeStore::default());
        let cache = SessionCacheKeyStore::new(probe.clone());
        cache.set("acct", "sk-abcd1234").unwrap();

        let masked = cache.masked("acct").unwrap();
        let inner_masked = probe.masked("acct").unwrap();

        assert_eq!(masked, inner_masked);
        assert_eq!(masked.as_deref(), Some("••••1234"));
    }

    #[test]
    fn delete_clears_both_cache_and_inner() {
        let probe = Arc::new(ProbeStore::default());
        let cache = SessionCacheKeyStore::new(probe.clone());
        cache.set("acct", "sk-secret").unwrap();

        cache.delete("acct").unwrap();

        assert_eq!(probe.delete_calls.load(Ordering::SeqCst), 1);
        assert_eq!(cache.get("acct").unwrap(), None);
        assert!(probe.data.lock().unwrap().get("acct").is_none());
        // The get() above is a miss on both cache and inner, so it must
        // have gone through to the (now-empty) inner store.
        assert_eq!(probe.get_calls.load(Ordering::SeqCst), 1);
    }
}
