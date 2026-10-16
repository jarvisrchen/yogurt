//! Session token persisted at `~/.yogurt/session-token` (mode 0600).
//!
//! The WS handler requires the token as a `?token=<token>` query param, because the browser
//! `WebSocket` API cannot set arbitrary headers. Comparison is constant-time via `subtle`.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use directories::BaseDirs;
use rand::RngCore;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

/// 32 random bytes as unpadded URL-safe base64.
pub const EXPECTED_TOKEN_LEN: usize = 43;

/// `Debug` is manual so the token never reaches a log.
#[derive(Clone)]
pub struct SessionToken(pub String);

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SessionToken").field(&"<REDACTED>").finish()
    }
}

impl SessionToken {
    pub fn validate(&self, candidate: &str) -> bool {
        // `ct_eq` needs equal-length slices.
        let a = self.0.as_bytes();
        let b = candidate.as_bytes();
        if a.len() != b.len() {
            return false;
        }
        a.ct_eq(b).into()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

pub fn default_token_path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("could not resolve home directory")?;
    Ok(base.home_dir().join(".yogurt").join("session-token"))
}

/// Reads the token, or generates one if the file is missing.
///
/// An empty or malformed file is an error rather than a regeneration, because a crash
/// mid-write would otherwise silently invalidate every active session.
pub fn load_or_create(path: &Path) -> Result<SessionToken> {
    if let Some(parent) = path.parent() {
        create_private_dir(parent)
            .with_context(|| format!("creating token parent {}", parent.display()))?;
    }

    if path.exists() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading session token at {}", path.display()))?;
        let token = raw.trim().to_string();
        if token.is_empty() {
            return Err(anyhow!(
                "session token at {} is empty (possibly a crash mid-write); \
                 delete it and restart yogurt to regenerate",
                path.display()
            ));
        }
        if token.len() != EXPECTED_TOKEN_LEN
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(anyhow!(
                "session token at {} is malformed (expected {}-char URL-safe base64); \
                 delete it and restart yogurt to regenerate",
                path.display(),
                EXPECTED_TOKEN_LEN
            ));
        }
        return Ok(SessionToken(token));
    }

    generate_and_persist(path)
}

/// `~/.yogurt/` holds transcripts and chat history, so it must not be world-readable.
/// `create_dir_all` would honor the umask (usually 0755).
fn create_private_dir(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        let mut b = std::fs::DirBuilder::new();
        b.recursive(true).mode(0o700);
        match b.create(p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(p)?;
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o700 {
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(p)
    }
}

/// Writes `<path>.tmp` at mode 0600, fsyncs, then renames, so a crash never leaves a partial file.
fn generate_and_persist(path: &Path) -> Result<SessionToken> {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);

    let tmp = path.with_extension("tmp");

    {
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(&tmp)
                .with_context(|| format!("creating tmp session token at {}", tmp.display()))?
        };

        #[cfg(not(unix))]
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp)
            .with_context(|| format!("creating tmp session token at {}", tmp.display()))?;

        file.write_all(token.as_bytes())
            .with_context(|| format!("writing tmp session token at {}", tmp.display()))?;
        file.sync_all()
            .with_context(|| format!("fsync tmp session token at {}", tmp.display()))?;
    }

    std::fs::rename(&tmp, path).with_context(|| {
        format!(
            "atomic-rename session token from {} to {}",
            tmp.display(),
            path.display()
        )
    })?;

    Ok(SessionToken(token))
}
