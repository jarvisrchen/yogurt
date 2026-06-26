//! Bundled LLM prompt templates for yogurt.
//!
//! Ships exactly two templates: `enhance.md` (hero augmented-notes prompt,
//! takes `{notes}` + `{transcript}` placeholders per CONTEXT D-16) and
//! `chat-system.md` (static in-meeting chat system prompt, consumed by
//! Phase 6).
//!
//! Two loading modes (CONTEXT D-15, D-17):
//! - `Mode::Release` reads embedded templates once at `Prompts::load`. The
//!   `rust-embed` derive bakes the on-disk `templates/` into the binary at
//!   `cargo build` time so a `cargo build --release` followed by a binary
//!   restart picks up any edits — that satisfies PROMPT-04 for release.
//! - `Mode::Dev` re-reads from `CARGO_MANIFEST_DIR/templates/` on every call
//!   so a power user editing the template file sees the new text on the
//!   very next request without rebuilding.

mod ctx;
pub use ctx::EnhanceCtx;

use anyhow::{Context, Result};
use rust_embed::RustEmbed;
use serde::Serialize;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;

#[derive(RustEmbed)]
#[folder = "templates/"]
struct Embedded;

/// Where to load templates from in dev mode (relative to the crate root).
/// In release builds, this is ignored — `RustEmbed` serves from the binary.
fn dev_template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates")
}

/// Source of the template bytes.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// Read templates from disk on every call. For `cargo run`.
    Dev,
    /// Read once at construction from the embedded copy.
    Release,
}

pub struct Prompts {
    mode: Mode,
    cached_enhance: Option<String>,
    cached_chat: Option<String>,
}

impl Prompts {
    pub fn load(mode: Mode) -> Result<Self> {
        let mut p = Self {
            mode,
            cached_enhance: None,
            cached_chat: None,
        };
        if matches!(mode, Mode::Release) {
            p.cached_enhance = Some(read_embedded("enhance.md")?);
            p.cached_chat = Some(read_embedded("chat-system.md")?);
        }
        Ok(p)
    }

    /// Render `enhance.md` with the given context. Disables HTML-escaping
    /// because the rendered output flows to an LLM prompt, not to a browser
    /// (CONTEXT D-16).
    pub fn render_enhance<S: Serialize>(&self, ctx: &S) -> Result<String> {
        let raw = self.read("enhance.md", self.cached_enhance.as_deref())?;
        render(&raw, "enhance", ctx)
    }

    /// Return the static `chat-system.md` prompt body. No templating.
    pub fn chat_system(&self) -> Result<String> {
        self.read("chat-system.md", self.cached_chat.as_deref())
    }

    fn read(&self, name: &str, cached: Option<&str>) -> Result<String> {
        match self.mode {
            Mode::Dev => std::fs::read_to_string(dev_template_dir().join(name))
                .with_context(|| format!("dev: reading templates/{name}")),
            Mode::Release => Ok(cached.expect("release mode caches at load").to_string()),
        }
    }
}

fn read_embedded(name: &str) -> Result<String> {
    let f = Embedded::get(name).with_context(|| format!("embedded asset missing: {name}"))?;
    Ok(std::str::from_utf8(&f.data)?.to_string())
}

fn render<S: Serialize>(template: &str, name: &str, ctx: &S) -> Result<String> {
    let mut tt = TinyTemplate::new();
    // tinytemplate HTML-escapes by default. We want raw insertion because
    // notes/transcript are markdown going *into* a prompt, not HTML headed
    // for a browser.
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template(name, template)?;
    Ok(tt.render(name, ctx)?)
}
