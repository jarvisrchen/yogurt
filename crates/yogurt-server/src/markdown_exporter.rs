//! Single writer to `~/.yogurt/notes/` — atomic per-meeting markdown emit.
//!
//! STORE-03 mandates that every meeting has a `.md` file at
//! `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>.md` with YAML front-matter
//! and the wire-format body. STORE-04 mandates that every `notes_md` /
//! `enriched_md` mutation funnels through this single writer.
//!
//! Atomicity: write to `<final-path>.tmp` then `std::fs::rename` to the
//! final path — POSIX rename is atomic, so a partial write cannot corrupt
//! an existing file. (The lesson from Phase 0 BL-01.)
//!
//! The actual wiring (enhance handler calls `MarkdownExporter::write` on
//! every mutation) lands in Plan 04-03; this module just exposes the
//! single-writer surface.

use anyhow::{Context, Result};
use std::io::Write;
use std::path::{Path, PathBuf};
use time::macros::format_description;
use time::OffsetDateTime;

pub struct MarkdownExporter {
    notes_dir: PathBuf,
}

pub struct Meeting<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub started_at_unix_ms: i64,
    pub ended_at_unix_ms: Option<i64>,
    pub body_md: &'a str,
}

impl MarkdownExporter {
    /// `notes_dir` is typically `~/.yogurt/notes`. Created on first use.
    pub fn new(notes_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&notes_dir).with_context(|| format!("mkdir {notes_dir:?}"))?;
        Ok(Self { notes_dir })
    }

    /// Notes directory this exporter writes into. Mostly useful for tests.
    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    /// Phase 7 (Plan 07-03) — resolve the expected on-disk path for a
    /// meeting WITHOUT writing the file. Used by `GET /:id/markdown` and
    /// `POST /:id/reveal` so we can locate (and if missing, lazily re-emit)
    /// the markdown file for any meeting in the SQLite directory.
    ///
    /// Returns the same filename `write()` would produce for the same
    /// inputs — same `<YYYY-MM-DD-HHmm>-<slug>-<id6>.md` derivation —
    /// so it's safe to use as a pure lookup. Returns an error only if
    /// `started_at_unix_ms` is outside the legal `OffsetDateTime` range.
    pub fn path_for(&self, m: &Meeting<'_>) -> Result<PathBuf> {
        let fname = filename_for(m)?;
        Ok(self.notes_dir.join(fname))
    }

    /// Atomic write: serialize body to `<final>.tmp`, fsync, then rename
    /// to the final path. Filename: `<YYYY-MM-DD-HHmm>-<slug>-<id6>.md`
    /// where the date is derived from `started_at_unix_ms` (UTC), `<slug>`
    /// is a lowercased-dasherized form of `title` (falls back to
    /// `untitled`), and `<id6>` is the first 6 chars of the meeting id
    /// (HI-6 — prevents same-minute same-slug collisions).
    ///
    /// HI-5: previous implementation called std::fs::write + std::fs::rename
    /// without ever fsync'ing the file contents or the directory entry. POSIX
    /// rename atomicity guarantees the user sees either old or new at the
    /// directory level, but the *contents* of the renamed file can be empty
    /// or torn if a power loss happens before the data blocks are flushed
    /// to disk. We now:
    ///   1. Open the tmp file with create_new (atomic existence check —
    ///      blocks a concurrent same-path write).
    ///   2. Write the body, then `sync_all()` to flush data + metadata.
    ///   3. Drop the file handle so the rename is on a closed fd.
    ///   4. Rename to the final path (POSIX-atomic visibility).
    ///   5. fsync the parent directory so the rename itself is durable.
    pub fn write(&self, m: &Meeting<'_>) -> Result<PathBuf> {
        let fname = filename_for(m)?;
        let final_path = self.notes_dir.join(&fname);
        let tmp_path = self.notes_dir.join(format!("{fname}.tmp"));

        let content = render_yaml_frontmatter(m) + m.body_md;

        // If a prior crash left a stale `.tmp` around, remove it so
        // create_new can succeed. Best-effort — if it doesn't exist, fine.
        let _ = std::fs::remove_file(&tmp_path);

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("open tmp {tmp_path:?}"))?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("write tmp {tmp_path:?}"))?;
        // HI-5: fsync the tmp file BEFORE renaming so the data blocks are
        // durable on disk. Without this, POSIX rename only guarantees
        // visibility-atomicity, not content-atomicity.
        f.sync_all()
            .with_context(|| format!("fsync tmp {tmp_path:?}"))?;
        drop(f);

        std::fs::rename(&tmp_path, &final_path)
            .with_context(|| format!("rename {tmp_path:?} -> {final_path:?}"))?;

        // HI-5: fsync the parent directory so the rename's directory-entry
        // change is durable. On macOS APFS + ext4 this is a real win;
        // best-effort on platforms where dir-fsync is a no-op or errors.
        if let Ok(dir) = std::fs::File::open(&self.notes_dir) {
            let _ = dir.sync_all();
        }

        Ok(final_path)
    }
}

fn filename_for(m: &Meeting<'_>) -> Result<String> {
    let secs = m.started_at_unix_ms / 1000;
    let dt = OffsetDateTime::from_unix_timestamp(secs).context("invalid started_at")?;
    // Compile-time format description (avoids the deprecated runtime
    // `format_description::parse`).
    let fmt = format_description!("[year]-[month]-[day]-[hour][minute]");
    let stamp = dt.format(&fmt).context("format dt")?;
    let slug = slugify(m.title);
    // HI-6: append a 6-char id prefix so two meetings in the same minute
    // with the same slug ("Sync"-titled standups) don't silently
    // overwrite each other on rename. The id is a UUID (hex or
    // hex+hyphen); strip non-alphanumeric and take the first 6 chars.
    // Falls back to "noid" if the id is too short to slice (shouldn't
    // happen for real UUIDs but defensive).
    let id_safe: String = m.id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let id_prefix = if id_safe.len() >= 6 {
        &id_safe[..6]
    } else if id_safe.is_empty() {
        "noid"
    } else {
        id_safe.as_str()
    };
    Ok(format!("{stamp}-{slug}-{id_prefix}.md"))
}

fn slugify(s: &str) -> String {
    let s = s.trim().to_lowercase();
    if s.is_empty() {
        return "untitled".into();
    }
    let dasherized: String = s
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let parts: Vec<&str> = dasherized
        .trim_matches('-')
        .split('-')
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        "untitled".into()
    } else {
        parts.join("-")
    }
}

fn render_yaml_frontmatter(m: &Meeting<'_>) -> String {
    format!(
        "---\nid: {id}\ntitle: {title}\nstarted_at: {start}\nended_at: {end}\n---\n\n",
        id = m.id,
        title = yaml_escape(m.title),
        start = m.started_at_unix_ms,
        end = m
            .ended_at_unix_ms
            .map(|e| e.to_string())
            .unwrap_or_else(|| "null".into()),
    )
}

fn yaml_escape(s: &str) -> String {
    // Sufficient for v1 — titles are user-typed strings, no embedded YAML.
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
