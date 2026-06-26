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
///
/// HI-3: ListItem keys DELIBERATELY ignore depth. LLMs frequently flatten
/// nested lists (depth-1 sub-bullets re-emitted as depth-0 top-level
/// items); without depth-agnostic matching, the merge would treat the
/// flattened LLM bullet as a NEW AI block and silently drop the user's
/// depth-1 sub-bullet text. By matching on cleaned-text-only, the diff
/// recognizes the bullet as the same content; `diff::merge` then preserves
/// the USER's original depth (see fixture `04-nested-list-flattened`).
pub fn block_key(b: &Block) -> String {
    let raw = match b {
        Block::Heading { level, text } => format!("h{level}:{text}"),
        Block::Paragraph { md } => format!("p:{md}"),
        // HI-3: depth intentionally omitted from key — see doc comment above.
        Block::ListItem { md, .. } => format!("li:{md}"),
        Block::CodeBlock { lang, body } => {
            format!("code:{}:{body}", lang.as_deref().unwrap_or(""))
        }
        Block::BlockQuote { md } => format!("bq:{md}"),
        Block::Hr => "hr".into(),
    };
    strip_markers(&raw).trim().to_ascii_lowercase()
}

/// HI-4: HTML-aware wire-format span stripper. The previous regex-based
/// stripper (three `regex_lite::Regex::new`s on every call — also LO-1)
/// had two related bugs:
///   1. `</span>` was stripped GLOBALLY. With nested wire-format spans
///      (outer `data-ai-grey`, inner `data-transcript-link`), the regex
///      stripped BOTH closing tags, leaving phantom transcript-link
///      content inside the user-block key — so a re-enhance flow couldn't
///      recognize previously-promoted user content.
///   2. Regex compilation happened on every call (LO-1) — wasted cycles.
///
/// The replacement is a single linear scan that walks `<span ...>` open +
/// matching `</span>` close pairs, deletes BOTH for any span carrying our
/// wire-format attribute, and leaves anything else (raw user-typed span,
/// other markup) intact. Compiled regexes are cached in `OnceLock`.
fn strip_markers(s: &str) -> String {
    // Fast path: nothing to strip.
    if !s.contains("<span") && !s.contains("</span>") {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `<span` opening.
        if bytes[i] == b'<' && bytes[i..].starts_with(b"<span") {
            // Find the closing `>`.
            if let Some(open_end_rel) = s[i..].find('>') {
                let open_end = i + open_end_rel + 1; // index AFTER `>`
                let open_tag = &s[i..open_end];
                let is_wire_format =
                    open_tag.contains("data-ai-grey") || open_tag.contains("data-transcript-link");
                if is_wire_format {
                    // Different handling depending on which wire-format
                    // attribute matched:
                    //   - data-ai-grey: PRESERVE the inner text (the user's
                    //     real bullet content lives there — block_key needs
                    //     it for matching). Skip any nested transcript-link
                    //     spans (their `↳ HH:MM` content is just chrome,
                    //     not user text).
                    //   - data-transcript-link: DROP the entire span including
                    //     its `↳ HH:MM` content (it's not part of the
                    //     user's bullet body).
                    let is_link_span = open_tag.contains("data-transcript-link");
                    let mut depth = 1;
                    let mut j = open_end;
                    while j < bytes.len() && depth > 0 {
                        if bytes[j] == b'<' && s[j..].starts_with("<span") {
                            depth += 1;
                            // Skip past this nested open's `>`. Mark it as
                            // a transcript-link span if relevant so we
                            // suppress its inner text too.
                            if let Some(rel) = s[j..].find('>') {
                                // Determine if this nested span is a
                                // transcript-link; if so, skip its inner
                                // content entirely.
                                let nested_open = &s[j..j + rel + 1];
                                if nested_open.contains("data-transcript-link") {
                                    // Skip to its matching close at this
                                    // depth.
                                    let mut k = j + rel + 1;
                                    let mut inner_depth = 1;
                                    while k < bytes.len() && inner_depth > 0 {
                                        if bytes[k] == b'<' && s[k..].starts_with("<span") {
                                            inner_depth += 1;
                                            if let Some(r2) = s[k..].find('>') {
                                                k += r2 + 1;
                                                continue;
                                            } else {
                                                break;
                                            }
                                        }
                                        if bytes[k] == b'<' && s[k..].starts_with("</span>") {
                                            inner_depth -= 1;
                                            k += "</span>".len();
                                            continue;
                                        }
                                        k += 1;
                                    }
                                    depth -= 1;
                                    j = k;
                                    continue;
                                }
                                j += rel + 1;
                                continue;
                            } else {
                                break;
                            }
                        }
                        if bytes[j] == b'<' && s[j..].starts_with("</span>") {
                            depth -= 1;
                            j += "</span>".len();
                            continue;
                        }
                        if !is_link_span && depth == 1 {
                            // Inside the OUTER ai-grey span: preserve the
                            // inner bullet text. For a transcript-link
                            // outer span, suppress everything.
                            out.push(bytes[j] as char);
                        }
                        j += 1;
                    }
                    i = j;
                    continue;
                } else {
                    // Non-wire-format span: keep it verbatim.
                    out.push_str(open_tag);
                    i = open_end;
                    continue;
                }
            }
        }
        // Lone closing `</span>` outside any matched open — drop quietly.
        if bytes[i] == b'<' && s[i..].starts_with("</span>") {
            i += "</span>".len();
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
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
                // HI-3: when a NESTED list opens inside an Item whose buf is
                // populated, flush the outer item NOW before its children
                // overwrite buf. pulldown-cmark emits Start(List) inside an
                // Item without an intermediate End(Item), and the cleared buf
                // when the nested Item starts would otherwise erase the
                // outer item's text content.
                if let Some(ParseState::ListItem(d)) = state.as_ref().copied() {
                    if !buf.trim().is_empty() {
                        blocks.push(Block::ListItem {
                            md: std::mem::take(&mut buf),
                            depth: d,
                        });
                        // Keep state as-is so End(Item) below knows we
                        // already flushed and avoids emitting a duplicate
                        // empty bullet (the `take()` returns None then).
                        state = None;
                    }
                }
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
                    // HI-3: skip flushing an empty item — this fires for the
                    // outer item whose body we already pre-flushed at the
                    // nested-list Start above.
                    let md = std::mem::take(&mut buf);
                    if !md.trim().is_empty() {
                        blocks.push(Block::ListItem { md, depth: d });
                    }
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

#[derive(Copy, Clone)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// HI-4: nested wire-format spans (transcript-link inside ai-grey) must
    /// strip both spans cleanly without leaving phantom `↳ HH:MM` text in
    /// the block key. The old global `</span>` regex stripped both closers
    /// and left the transcript-link text glued onto the user's bullet
    /// content, breaking re-enhance matching on previously-promoted lines.
    #[test]
    fn strip_markers_handles_nested_wire_format_spans() {
        let input = r#"<span data-ai-grey data-ts="120">pricing <span data-transcript-link data-ts="120">↳ 02:00</span></span>"#;
        let stripped = strip_markers(input);
        // The user's text "pricing" must survive (with optional trailing
        // whitespace), and the transcript-link content `↳ 02:00` must NOT.
        assert!(
            stripped.contains("pricing"),
            "outer ai-grey text preserved, got: {stripped:?}"
        );
        assert!(
            !stripped.contains("↳"),
            "transcript-link content must be removed, got: {stripped:?}"
        );
        assert!(
            !stripped.contains("02:00"),
            "transcript timestamp must be removed, got: {stripped:?}"
        );
        // No leftover span tags either.
        assert!(
            !stripped.contains("<span"),
            "open spans removed, got: {stripped:?}"
        );
        assert!(
            !stripped.contains("</span>"),
            "close spans removed, got: {stripped:?}"
        );
    }

    /// HI-3 + parser fix: a parent ListItem whose body has text AND nested
    /// children must still appear as its own block (it would have been
    /// silently dropped before the parse() fix because the nested
    /// child's `buf.clear()` clobbered the parent's content).
    #[test]
    fn parse_preserves_parent_item_with_nested_children() {
        let md = "- pricing\n  - tiered\n  - discount\n";
        let blocks = parse(md);
        // 3 items: pricing (depth 0), tiered (depth 1), discount (depth 1).
        assert_eq!(blocks.len(), 3, "expected 3 ListItems, got: {blocks:?}");
        let texts: Vec<_> = blocks
            .iter()
            .filter_map(|b| match b {
                Block::ListItem { md, .. } => Some(md.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["pricing", "tiered", "discount"]);
    }
}
