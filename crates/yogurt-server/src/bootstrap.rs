//! Bootstrap stub — Plan 05-02 Task 3 implements `seed_from_env`.
//!
//! This module is created in Task 2 only so that `lib.rs` can call
//! `bootstrap::seed_from_env(&state).await` at startup without a
//! compile-time gap. Task 3 replaces this stub with the full
//! `YOGURT_*_API_KEY` → providers + Keychain mapping.

use crate::state::AppState;
use anyhow::Result;

/// Report of which env-var-backed providers were seeded vs already present.
#[derive(Debug, Default)]
pub struct SeedReport {
    /// Provider names newly inserted on this call.
    pub seeded: Vec<String>,
    /// Env-var-present names skipped because a same-named row already exists.
    pub skipped: Vec<String>,
}

/// Placeholder — Task 3 implements the real env-var → provider mapping.
pub async fn seed_from_env(_state: &AppState) -> Result<SeedReport> {
    Ok(SeedReport::default())
}
