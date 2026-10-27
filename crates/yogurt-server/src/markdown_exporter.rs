//! Single writer to `~/.yogurt/notes/` with atomic per-meeting markdown emit.
//!
//! Every `notes_md` / `enriched_md` mutation funnels through this writer, which emits
//! `~/.yogurt/notes/<YYYY-MM-DD-HHmm>-<slug>-<id6>.md` (id6 = last 6 alphanumeric id chars) with YAML front-matter.

use anyhow::{Context, Result};
use chrono::DateTime;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    pub fn new(notes_dir: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&notes_dir).with_context(|| format!("mkdir {notes_dir:?}"))?;
        Ok(Self { notes_dir })
    }

    pub fn notes_dir(&self) -> &Path {
        &self.notes_dir
    }

    /// The path `write()` would produce, without writing. Errors only if `started_at_unix_ms`
    /// is outside the legal `DateTime` range.
    pub fn path_for(&self, m: &Meeting<'_>) -> Result<PathBuf> {
        let fname = filename_for(m)?;
        Ok(self.notes_dir.join(fname))
    }

    /// Writes `<final>.tmp` with `create_new` (blocks a concurrent same-path write), fsyncs it,
    /// renames, then fsyncs the directory. Rename alone is visibility-atomic, but after a power
    /// loss the file contents can be empty or torn without the fsyncs.
    /// UTC date comes from `started_at_unix_ms`, `<slug>` falls back to `untitled`.
    pub fn write(&self, m: &Meeting<'_>) -> Result<PathBuf> {
        let fname = filename_for(m)?;
        let final_path = self.notes_dir.join(&fname);
        let tmp_path = self.notes_dir.join(format!("{fname}.tmp"));

        let content = render_yaml_frontmatter(m) + m.body_md;

        // A crash may have left a stale `.tmp` that would fail `create_new`.
        let _ = std::fs::remove_file(&tmp_path);

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp_path)
            .with_context(|| format!("open tmp {tmp_path:?}"))?;
        f.write_all(content.as_bytes())
            .with_context(|| format!("write tmp {tmp_path:?}"))?;
        f.sync_all()
            .with_context(|| format!("fsync tmp {tmp_path:?}"))?;
        drop(f);

        std::fs::rename(&tmp_path, &final_path)
            .with_context(|| format!("rename {tmp_path:?} -> {final_path:?}"))?;

        // Best-effort: dir-fsync is a no-op or errors on some platforms.
        if let Ok(dir) = std::fs::File::open(&self.notes_dir) {
            let _ = dir.sync_all();
        }

        // The new file is already written, so a stale legacy copy is not worth failing the write.
        if let Err(e) = self.remove_legacy(m) {
            tracing::warn!(error = %e, "markdown export: legacy note file not removed");
        }

        Ok(final_path)
    }

    /// Removes the meeting's file, and its legacy-named file when that one belongs to this meeting.
    pub fn remove(&self, m: &Meeting<'_>) -> Result<()> {
        let path = self.path_for(m)?;
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("remove {path:?}")),
        }
        self.remove_legacy(m)
    }

    /// Older versions named files with the first 6 id characters, which time-ordered ids share.
    /// The front-matter id check keeps a twin's file safe.
    fn remove_legacy(&self, m: &Meeting<'_>) -> Result<()> {
        let legacy = self
            .notes_dir
            .join(name_with(m, |id| &id[..id.len().min(6)])?);
        if legacy == self.path_for(m)? || !frontmatter_id_is(&legacy, m.id) {
            return Ok(());
        }
        match std::fs::remove_file(&legacy) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e).with_context(|| format!("remove {legacy:?}")),
        }
    }
}

fn frontmatter_id_is(path: &Path, id: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    let Some(rest) = text.strip_prefix("---\n") else {
        return false;
    };
    rest.lines()
        .take_while(|l| *l != "---")
        .any(|l| l.strip_prefix("id: ") == Some(id))
}

fn filename_for(m: &Meeting<'_>) -> Result<String> {
    name_with(m, |id| &id[id.len().saturating_sub(6)..])
}

fn name_with(m: &Meeting<'_>, pick: impl Fn(&str) -> &str) -> Result<String> {
    let secs = m.started_at_unix_ms / 1000;
    let dt = DateTime::from_timestamp(secs, 0).context("invalid started_at")?;
    let stamp = dt.format("%Y-%m-%d-%H%M");
    let slug = slugify(m.title);
    // Ids are time-ordered, so only the random tail tells same-minute meetings apart.
    let id_safe: String = m.id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let id_part = if id_safe.is_empty() {
        "noid"
    } else {
        pick(&id_safe)
    };
    Ok(format!("{stamp}-{slug}-{id_part}.md"))
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
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}
