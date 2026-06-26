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

    /// Atomic write: serialize body to `<final>.tmp`, then rename to the final
    /// path. Filename: `<YYYY-MM-DD-HHmm>-<slug>.md` where the date is derived
    /// from `started_at_unix_ms` (UTC) and `<slug>` is a lowercased-dasherized
    /// form of `title` (falls back to `untitled`).
    pub fn write(&self, m: &Meeting<'_>) -> Result<PathBuf> {
        let fname = filename_for(m)?;
        let final_path = self.notes_dir.join(&fname);
        let tmp_path = self.notes_dir.join(format!("{fname}.tmp"));

        let content = render_yaml_frontmatter(m) + m.body_md;
        std::fs::write(&tmp_path, &content).with_context(|| format!("write tmp {tmp_path:?}"))?;
        std::fs::rename(&tmp_path, &final_path)
            .with_context(|| format!("rename {tmp_path:?} -> {final_path:?}"))?;
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
    Ok(format!("{stamp}-{slug}.md"))
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
