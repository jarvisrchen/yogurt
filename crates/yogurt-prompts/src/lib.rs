//! Bundled LLM prompt templates.
//! `Mode::Release` serves the copy `rust-embed` bakes in at build time.
//! `Mode::Dev` re-reads `templates/` on every call so edits show up without rebuilding.

mod ctx;
pub use ctx::EnhanceCtx;

use anyhow::{bail, Context, Result};
use rust_embed::RustEmbed;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tinytemplate::TinyTemplate;

#[derive(RustEmbed)]
#[folder = "templates/"]
struct Embedded;

/// In UI order. The first entry is the fallback when auto-detection names nothing recognizable.
/// Each id is the stem of `templates/enhance/<id>.md`.
pub const TEMPLATE_IDS: [&str; 7] = [
    "general",
    "standup",
    "one-on-one",
    "team-meeting",
    "design-review",
    "customer-call",
    "interview",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Template {
    pub id: &'static str,
    pub name: String,
    /// Shown as a picker hint and given to the model to choose by.
    pub when: String,
    #[serde(skip)]
    pub body: String,
}

fn dev_template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates")
}

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Dev,
    Release,
}

pub struct Prompts {
    mode: Mode,
    cached: HashMap<String, String>,
}

impl Prompts {
    pub fn load(mode: Mode) -> Result<Self> {
        let mut p = Self {
            mode,
            cached: HashMap::new(),
        };
        if matches!(mode, Mode::Release) {
            for name in ["enhance.md", "chat-system.md", "library-chat-system.md"] {
                p.cached.insert(name.to_string(), read_embedded(name)?);
            }
            for id in TEMPLATE_IDS {
                let name = template_file(id);
                p.cached.insert(name.clone(), read_embedded(&name)?);
            }
        }
        Ok(p)
    }

    /// `None` renders every format with an instruction to pick one, `Some(id)` renders just that one.
    /// An unknown id is an error, since callers validate against [`TEMPLATE_IDS`] first.
    pub fn render_enhance(&self, ctx: &EnhanceCtx<'_>) -> Result<String> {
        let format = match ctx.template {
            Some(id) => {
                let t = self
                    .template(id)?
                    .with_context(|| format!("unknown enhance template: {id}"))?;
                format!(
                    "Note format: {} ({}).\n{OUTPUT_CONTRACT}\n\n{}",
                    t.name,
                    t.when,
                    t.body.trim_end()
                )
            }
            None => {
                let mut s = String::from(
                    "Note format. Pick the format below that best matches what this meeting \
                     actually was, judging by the transcript and the notes.\n",
                );
                s.push_str(OUTPUT_CONTRACT);
                for t in self.templates()? {
                    s.push_str(&format!(
                        "\n\n### {}\nUse when: {}.\n{}",
                        t.id,
                        t.when,
                        t.body.trim_end()
                    ));
                }
                s
            }
        };
        #[derive(Serialize)]
        struct Full<'a> {
            notes: &'a str,
            transcript: &'a str,
            format: String,
        }
        let raw = self.read("enhance.md")?;
        render(
            &raw,
            "enhance",
            &Full {
                notes: ctx.notes,
                transcript: ctx.transcript,
                format,
            },
        )
    }

    pub fn chat_system(&self) -> Result<String> {
        self.read("chat-system.md")
    }

    pub fn library_chat_system(&self) -> Result<String> {
        self.read("library-chat-system.md")
    }

    pub fn templates(&self) -> Result<Vec<Template>> {
        TEMPLATE_IDS
            .iter()
            .map(|id| {
                self.template(id)?
                    .with_context(|| format!("template listed but not loadable: {id}"))
            })
            .collect()
    }

    pub fn template(&self, id: &str) -> Result<Option<Template>> {
        let Some(id) = TEMPLATE_IDS.iter().copied().find(|t| *t == id) else {
            return Ok(None);
        };
        let raw = self.read(&template_file(id))?;
        parse_template(id, &raw).map(Some)
    }

    fn read(&self, name: &str) -> Result<String> {
        match self.mode {
            Mode::Dev => std::fs::read_to_string(dev_template_dir().join(name))
                .with_context(|| format!("dev: reading templates/{name}")),
            Mode::Release => Ok(self
                .cached
                .get(name)
                .expect("release mode caches at load")
                .clone()),
        }
    }
}

/// Shared by the pick and forced paths: the first-line marker [`split_template_line`] parses, plus the section rules.
const OUTPUT_CONTRACT: &str =
    "Your output's first line must be exactly `template: <id>` on its own \
    line, followed by a blank line, then the document. Keep the user's own headings if they wrote \
    any and put the format's remaining sections after them. Include only the sections that have \
    content and omit empty ones. Headings are `##`.";

fn template_file(id: &str) -> String {
    format!("enhance/{id}.md")
}

/// Expects a `name:` line, a `when:` line, a blank line, then the body.
fn parse_template(id: &'static str, raw: &str) -> Result<Template> {
    let mut lines = raw.lines();
    let field = |lines: &mut std::str::Lines<'_>, key: &str| -> Result<String> {
        let line = lines
            .next()
            .with_context(|| format!("template {id}: missing `{key}:` line"))?;
        let value = line
            .strip_prefix(key)
            .and_then(|s| s.strip_prefix(':'))
            .with_context(|| format!("template {id}: expected `{key}: ...`, got {line:?}"))?;
        let value = value.trim();
        if value.is_empty() {
            bail!("template {id}: `{key}:` is empty");
        }
        Ok(value.to_string())
    };
    let name = field(&mut lines, "name")?;
    let when = field(&mut lines, "when")?;
    let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
    if body.is_empty() {
        bail!("template {id}: empty body");
    }
    Ok(Template {
        id,
        name,
        when,
        body,
    })
}

/// Splits the `template: <id>` marker off the model's output, without validating the id.
/// A reply without the marker returns `(None, text)`.
/// A streaming prefix of the marker (`"templ"`) is treated as the marker so a live preview never flashes it.
pub fn split_template_line(text: &str) -> (Option<&str>, &str) {
    const MARKER: &str = "template:";
    let start = text.trim_start();
    let (first, rest) = match start.find('\n') {
        Some(i) => (&start[..i], &start[i + 1..]),
        None => (start, ""),
    };
    let first_trim = first.trim().trim_end_matches('`').trim_start_matches('`');
    let lower = first_trim.to_ascii_lowercase();
    if let Some(id) = lower.strip_prefix(MARKER) {
        let id = id.trim().trim_matches(['`', '*', '"']);
        let id = match id.is_empty() {
            true => None,
            false => Some(&first_trim[first_trim.len() - id.len()..]),
        };
        return (id, rest.trim_start_matches(['\n', '\r']));
    }
    if rest.is_empty() && !lower.is_empty() && MARKER.starts_with(&lower) {
        return (None, "");
    }
    (None, text)
}

fn read_embedded(name: &str) -> Result<String> {
    let f = Embedded::get(name).with_context(|| format!("embedded asset missing: {name}"))?;
    Ok(std::str::from_utf8(&f.data)?.to_string())
}

fn render<S: Serialize>(template: &str, name: &str, ctx: &S) -> Result<String> {
    let mut tt = TinyTemplate::new();
    // Output goes into an LLM prompt, not a browser, so skip tinytemplate's HTML escaping.
    tt.set_default_formatter(&tinytemplate::format_unescaped);
    tt.add_template(name, template)?;
    Ok(tt.render(name, ctx)?)
}

#[cfg(test)]
mod tests {
    use super::split_template_line;

    #[test]
    fn splits_marker_and_body() {
        let (id, body) = split_template_line("template: standup\n\n## Updates\n- x");
        assert_eq!(id, Some("standup"));
        assert_eq!(body, "## Updates\n- x");
    }

    #[test]
    fn tolerates_case_backticks_and_leading_blank_lines() {
        let (id, body) = split_template_line("\n\n`Template: Design-Review`\n## Proposal");
        assert_eq!(id, Some("Design-Review"));
        assert_eq!(body, "## Proposal");
    }

    #[test]
    fn passes_through_a_reply_without_marker() {
        let (id, body) = split_template_line("## Decisions\n- ship");
        assert_eq!(id, None);
        assert_eq!(body, "## Decisions\n- ship");
    }

    #[test]
    fn hides_a_streaming_prefix_of_the_marker() {
        assert_eq!(split_template_line("templ"), (None, ""));
        assert_eq!(split_template_line("template: stand"), (Some("stand"), ""));
        assert_eq!(split_template_line("template:"), (None, ""));
        assert_eq!(split_template_line("## Dec"), (None, "## Dec"));
    }
}
