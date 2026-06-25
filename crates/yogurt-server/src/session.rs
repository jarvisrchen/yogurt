//! Session token persisted at `~/.yogurt/session-token` (mode 0600).
//!
//! On first boot we generate a 32-byte URL-safe random token and write it to
//! disk. Subsequent boots read the existing token so it survives restarts
//! (CONTEXT D-21). The WS handler requires this token as either:
//!   - `?token=<token>` query param, OR
//!   - `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header.
//!
//! Comparison is constant-time via the `subtle` crate.

use anyhow::{anyhow, Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use directories::BaseDirs;
use rand::RngCore;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

/// Expected length of a session token: 32 random bytes encoded as URL-safe
/// base64 with no padding = 43 characters.
pub const EXPECTED_TOKEN_LEN: usize = 43;

/// A loaded (or freshly generated) session token. The inner string is the
/// raw URL-safe base64 encoding of 32 random bytes.
///
/// `Debug` is implemented manually to redact the token value in any logging
/// path (BL-02). Never print the raw token to stdout/stderr.
#[derive(Clone)]
pub struct SessionToken(pub String);

impl std::fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("SessionToken").field(&"<REDACTED>").finish()
    }
}

impl SessionToken {
    /// Constant-time equality check between the stored token and a candidate.
    pub fn validate(&self, candidate: &str) -> bool {
        // Length differences leak the stored token length anyway, but using
        // `ct_eq` on equal-length byte slices is the standard recipe; the
        // explicit length gate makes the constant-time region clean.
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

/// Default token path: `<home>/.yogurt/session-token`.
pub fn default_token_path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("could not resolve home directory")?;
    Ok(base.home_dir().join(".yogurt").join("session-token"))
}

/// Read the token from disk, or generate + persist a new one if missing.
///
/// On Unix the file is opened with mode `0600` BEFORE the token bytes are
/// written, so even a momentary peek by another local user is impossible.
///
/// **Fail-loud on corruption (BL-01, MD-05):** if the file exists but is empty
/// or malformed (wrong length / wrong charset), this function returns an error
/// instead of silently regenerating a new token. Silent regeneration would
/// invalidate every active session after a crash mid-write — a stealthy DoS.
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
            // BL-01: Do NOT silently regenerate on empty. A crash mid-write
            // leaves a zero-length file; rotating the token here would
            // invalidate every active session without explanation.
            return Err(anyhow!(
                "session token at {} is empty (possibly a crash mid-write); \
                 delete it and restart yogurt to regenerate",
                path.display()
            ));
        }
        // MD-05: validate length + charset. Reject corrupted files loudly.
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

/// Create a directory at mode 0700 (Unix) or default (non-Unix).
///
/// MD-06: `~/.yogurt/` must NOT be world-readable — the SQLite DB and
/// session-token live here and contain meeting transcripts and chat history.
/// `create_dir_all` honors the process umask (usually 0022 → 0755 dirs).
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
        // If the dir pre-existed at a looser mode, tighten it.
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

/// Generate a new token and atomically persist it.
///
/// BL-01: write to `<path>.tmp` with mode 0600, fsync, then rename over the
/// final path. Rename is atomic on the same filesystem, so a crash either
/// leaves the old (valid) token or the new (valid) token — never an empty
/// or partially-written file.
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
