//! Account name = provider ULID (or `stt-deepgram` for the STT singleton).
//!
//! Plaintext 0600 file rather than the macOS Keychain: Keychain ACLs bind to the code signature, so every unsigned dev rebuild re-prompted for every key,
//! a wedged prompt hung boot, and machines without Keychain write access could not run yogurt.
//! The file has the same posture as `~/.aws/credentials`, is encrypted at rest by FileVault, and needs no consent dialogs.

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};

/// Implementations must be cheap to clone (or wrapped in `Arc`) since `AppState` holds `Arc<dyn ApiKeyStore>` and is cloned per request.
pub trait ApiKeyStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> Result<()>;
    fn delete(&self, account: &str) -> Result<()>;

    /// The value as it is on disk now, for a secret another process may have rotated. In-memory stores have no other writer.
    fn get_fresh(&self, account: &str) -> Result<Option<String>> {
        self.get(account)
    }

    /// Blocks until this process holds the machine-wide lock for the store, then holds it until the guard drops.
    /// Callers run it on a blocking thread. In-memory stores have no other process and return at once.
    fn lock_exclusive(&self) -> Result<ProcessLock> {
        Ok(ProcessLock { _file: None })
    }

    /// `Some("••••XXXX")` (last 4 chars) when a key exists. The Settings UI footer relies on this mask format.
    fn masked(&self, account: &str) -> Result<Option<String>> {
        Ok(self.get(account)?.map(|s| mask_suffix(&s)))
    }
}

/// An exclusive advisory lock on a file, released when dropped.
pub struct ProcessLock {
    _file: Option<std::fs::File>,
}

/// Mask a secret as `"••••XXXX"` (last 4 unicode chars). A shorter secret is kept whole after the bullets so callers can still detect "key set".
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

/// In-memory fake for tests. Never use in production.
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

/// A `{account: secret}` JSON object at `path`, held in memory and rewritten atomically (temp file + rename, mode 0600) on every mutation.
pub struct FileKeyStore {
    path: PathBuf,
    map: RwLock<BTreeMap<String, String>>,
}

impl FileKeyStore {
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

    pub fn open_default() -> Result<Self> {
        Self::open(crate::paths::keys_path()?)
    }

    /// The file as it is now, so a write never drops entries another process added since this one booted.
    fn read_file(&self) -> Result<BTreeMap<String, String>> {
        match std::fs::read(&self.path) {
            Ok(bytes) => serde_json::from_slice(&bytes)
                .with_context(|| format!("parsing key file {}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(e).with_context(|| format!("reading {}", self.path.display())),
        }
    }

    /// Re-reads the file, applies one change, writes atomically, and only then updates the in-memory map.
    /// Two processes changing the file at the same instant can still lose one write, because there is no file lock.
    fn update(&self, change: impl FnOnce(&mut BTreeMap<String, String>) -> bool) -> Result<()> {
        let mut map = self.map.write().unwrap();
        let mut fresh = self.read_file()?;
        if change(&mut fresh) {
            write_private(&self.path, &serde_json::to_vec_pretty(&fresh)?)?;
        }
        *map = fresh;
        Ok(())
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
    fn get_fresh(&self, account: &str) -> Result<Option<String>> {
        let mut map = self.map.write().unwrap();
        *map = self.read_file()?;
        Ok(map.get(account).cloned())
    }
    fn lock_exclusive(&self) -> Result<ProcessLock> {
        let path = self.path.with_file_name("cloud.lock");
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        file.lock()
            .with_context(|| format!("locking {}", path.display()))?;
        Ok(ProcessLock { _file: Some(file) })
    }
    fn get(&self, account: &str) -> Result<Option<String>> {
        Ok(self.map.read().unwrap().get(account).cloned())
    }
    fn set(&self, account: &str, secret: &str) -> Result<()> {
        self.update(|m| {
            m.insert(account.to_string(), secret.to_string());
            true
        })
    }
    fn delete(&self, account: &str) -> Result<()> {
        self.update(|m| m.remove(account).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_fresh_sees_another_stores_write_and_the_lock_excludes() {
        let dir = std::env::temp_dir().join(format!("yogurt-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys.json");
        let (a, b) = (
            FileKeyStore::open(&path).unwrap(),
            FileKeyStore::open(&path).unwrap(),
        );
        a.set("acct", "v1").unwrap();
        assert_eq!(
            b.get("acct").unwrap(),
            None,
            "plain get is the boot snapshot"
        );
        assert_eq!(b.get_fresh("acct").unwrap().as_deref(), Some("v1"));

        let held = a.lock_exclusive().unwrap();
        let file = std::fs::File::open(dir.join("cloud.lock")).unwrap();
        assert!(matches!(
            file.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ));
        drop(held);
        assert!(file.try_lock().is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn two_stores_on_one_file_do_not_drop_each_others_keys() {
        let dir = std::env::temp_dir().join(format!("yogurt-keys-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("keys.json");
        let _ = std::fs::remove_file(&path);
        let a = FileKeyStore::open(&path).unwrap();
        let b = FileKeyStore::open(&path).unwrap();
        a.set("one", "1").unwrap();
        b.set("two", "2").unwrap();
        a.set("yogurt-cloud", "rotated").unwrap();
        b.delete("one").unwrap();
        let c = FileKeyStore::open(&path).unwrap();
        assert_eq!(c.get("one").unwrap(), None);
        assert_eq!(c.get("two").unwrap().as_deref(), Some("2"));
        assert_eq!(c.get("yogurt-cloud").unwrap().as_deref(), Some("rotated"));
        assert_eq!(a.get("two").unwrap().as_deref(), Some("2"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_write_leaves_the_in_memory_map_alone() {
        let dir = std::env::temp_dir().join(format!("yogurt-keys-ro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = FileKeyStore::open(dir.join("missing-dir").join("keys.json")).unwrap();
        assert!(store.set("k", "v").is_err());
        assert_eq!(store.get("k").unwrap(), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
