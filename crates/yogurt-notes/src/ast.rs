//! A tiny block-level markdown AST tailored for the diff/merge use case.
//!
//! pulldown-cmark gives us an event stream. We collapse it into a list of
//! `Block`s where each block is a top-level structural unit (heading, paragraph,
//! list item, list, blockquote, code fence). Inline content is kept as the
//! reconstructed markdown source for that block — we do not need to model
//! inline marks because the merge happens at block granularity.

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Block {
    Heading { level: u8, text: String },
    Paragraph { md: String },
    ListItem { md: String, depth: u8 },
    CodeBlock { lang: Option<String>, body: String },
    BlockQuote { md: String },
    Hr,
}

/// A canonical "key" we use to decide if two blocks are "the same block"
/// across user_md and enriched_md. It deliberately ignores trailing
/// whitespace, transcript-link spans, and our ai-grey marker spans —
/// because the LLM may add those to a user line, and we still want
/// to recognize the underlying line as the user's.
pub fn block_key(b: &Block) -> String {
    let raw = match b {
        Block::Heading { level, text } => format!("h{level}:{text}"),
        Block::Paragraph { md } => format!("p:{md}"),
        Block::ListItem { md, depth } => format!("li{depth}:{md}"),
        Block::CodeBlock { lang, body } => {
            format!("code:{}:{body}", lang.as_deref().unwrap_or(""))
        }
        Block::BlockQuote { md } => format!("bq:{md}"),
        Block::Hr => "hr".into(),
    };
    strip_markers(&raw).trim().to_ascii_lowercase()
}

fn strip_markers(s: &str) -> String {
    // Remove our wire-format spans before computing identity.
    let re1 = regex_lite::Regex::new(r#"<span data-ai-grey[^>]*>"#).unwrap();
    let re2 =
        regex_lite::Regex::new(r#"<span data-transcript-link[^>]*>↳ \d{2}:\d{2}</span>"#).unwrap();
    let re3 = regex_lite::Regex::new(r#"</span>"#).unwrap();
    let a = re1.replace_all(s, "");
    let b = re2.replace_all(&a, "");
    re3.replace_all(&b, "").into_owned()
}

/// Parse markdown into our flat block list. Lists are flattened — each
/// `<li>` becomes its own `Block::ListItem` with a depth attribute.
pub fn parse(md: &str) -> Vec<Block> {
    let parser = Parser::new_ext(md, Options::all());
    let mut blocks: Vec<Block> = Vec::new();
    let mut buf = String::new();
    let mut state: Option<ParseState> = None;
    let mut list_depth: u8 = 0;

    for ev in parser {
        match ev {
            Event::Start(Tag::Heading { level, .. }) => {
                state = Some(ParseState::Heading(heading_level_to_u8(level)));
                buf.clear();
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(ParseState::Heading(lv)) = state.take() {
                    blocks.push(Block::Heading {
                        level: lv,
                        text: std::mem::take(&mut buf),
                    });
                }
            }
            Event::Start(Tag::Paragraph) => {
                state = Some(ParseState::Paragraph);
                buf.clear();
            }
            Event::End(TagEnd::Paragraph) => {
                if matches!(state, Some(ParseState::Paragraph)) {
                    state = None;
                    blocks.push(Block::Paragraph {
                        md: std::mem::take(&mut buf),
                    });
                }
            }
            Event::Start(Tag::List(_)) => {
                list_depth = list_depth.saturating_add(1);
            }
            Event::End(TagEnd::List(_)) => {
                list_depth = list_depth.saturating_sub(1);
            }
            Event::Start(Tag::Item) => {
                state = Some(ParseState::ListItem(list_depth.saturating_sub(1)));
                buf.clear();
            }
            Event::End(TagEnd::Item) => {
                if let Some(ParseState::ListItem(d)) = state.take() {
                    blocks.push(Block::ListItem {
                        md: std::mem::take(&mut buf),
                        depth: d,
                    });
                }
            }
            Event::Text(t) | Event::Code(t) | Event::Html(t) | Event::InlineHtml(t) => {
                buf.push_str(&t);
            }
            Event::SoftBreak => buf.push(' '),
            Event::HardBreak => buf.push('\n'),
            Event::Rule => blocks.push(Block::Hr),
            _ => {}
        }
    }
    blocks
}

enum ParseState {
    Heading(u8),
    Paragraph,
    ListItem(u8),
}

fn heading_level_to_u8(l: HeadingLevel) -> u8 {
    match l {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
