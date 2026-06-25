//! Session token persisted at `~/.yogurt/session-token` (mode 0600).
//!
//! On first boot we generate a 32-byte URL-safe random token and write it to
//! disk. Subsequent boots read the existing token so it survives restarts
//! (CONTEXT D-21). The WS handler requires this token as either:
//!   - `?token=<token>` query param, OR
//!   - `Sec-WebSocket-Protocol: yogurt.<token>` subprotocol header.
//!
//! Comparison is constant-time via the `subtle` crate.

use anyhow::{Context, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use directories::BaseDirs;
use rand::RngCore;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use subtle::ConstantTimeEq;

/// A loaded (or freshly generated) session token. The inner string is the
/// raw URL-safe base64 encoding of 32 random bytes.
#[derive(Clone)]
pub struct SessionToken(pub String);

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
pub fn load_or_create(path: &Path) -> Result<SessionToken> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating token parent {}", parent.display()))?;
    }

    if path.exists() {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading session token at {}", path.display()))?;
        let token = raw.trim().to_string();
        if token.is_empty() {
            // File exists but is empty — treat as missing and regenerate.
            return generate_and_persist(path);
        }
        return Ok(SessionToken(token));
    }

    generate_and_persist(path)
}

fn generate_and_persist(path: &Path) -> Result<SessionToken> {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("creating session token at {}", path.display()))?;
        file.write_all(token.as_bytes())
            .with_context(|| format!("writing session token at {}", path.display()))?;
    }

    #[cfg(not(unix))]
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("creating session token at {}", path.display()))?;
        file.write_all(token.as_bytes())
            .with_context(|| format!("writing session token at {}", path.display()))?;
    }

    Ok(SessionToken(token))
}
