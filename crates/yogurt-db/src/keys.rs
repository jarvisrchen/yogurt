//! API-key storage: the `ApiKeyStore` trait + production `FileKeyStore`
//! (`~/.yogurt/keys.json`, mode 0600) + in-memory `MemoryKeyStore` for tests.
//!
//! Account name = provider ULID (or `stt-deepgram` for the STT singleton).
//!
//! ## Why a plaintext file and not the macOS Keychain?
//!
//! Keychain ACLs are bound to the binary's code signature, so every
//! unsigned dev rebuild re-prompted for every key, a wedged prompt hung
//! boot, and machines without Keychain write access could not run yogurt
//! at all. A 0600 JSON file next to the SQLite database has the same
//! posture as `~/.aws/credentials` or `gh`'s `hosts.yml`, is encrypted at
//! rest by FileVault, and needs no OS consent dialogs.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

/// Storage abstraction over the on-disk key file.
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

/// Production store: a `{account: secret}` JSON object at `path`, held in
/// memory and rewritten atomically (temp file + rename, mode 0600) on every
/// mutation. Reads never touch disk after construction.
pub struct FileKeyStore {
    path: PathBuf,
    map: RwLock<BTreeMap<String, String>>,
}

impl FileKeyStore {
    /// Open (or lazily create on first write) the key file at `path`.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let map = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing key file {}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => BTreeMap::new(),
            Err(e) => return Err(e).with_context(|| format!("reading {}", path.display())),
        };
        Ok(Self {
            path,
            map: RwLock::new(map),
        })
    }

    /// Open `~/.yogurt/keys.json`.
    pub fn open_default() -> Result<Self> {
        Self::open(crate::paths::keys_path()?)
    }

    fn persist(&self, map: &BTreeMap<String, String>) -> Result<()> {
        write_private(&self.path, &serde_json::to_vec_pretty(map)?)
    }
}

/// Write `bytes` to `path` with mode 0600 via a same-directory temp file and
/// an atomic rename, so a crash mid-write never leaves a truncated key file.
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    {
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut f = opts
            .open(&tmp)
            .with_context(|| format!("creating {}", tmp.display()))?;
        std::io::Write::write_all(&mut f, bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).with_context(|| format!("replacing {}", path.display()))
}

impl ApiKeyStore for FileKeyStore {
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.map.read().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        let mut map = self.map.write().unwrap();
        map.insert(account.to_string(), secret.to_string());
        self.persist(&map)
    }
    fn delete(&self, account: &str) -> Result<()> {
        let mut map = self.map.write().unwrap();
        if map.remove(account).is_some() {
            self.persist(&map)?;
        }
        Ok(())
    }
}
