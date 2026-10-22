//! Block-level markdown AST for diff/merge.
//! Inline content stays as reconstructed markdown source because the merge happens at block granularity.

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

/// Decides whether two blocks are the same across user_md and enriched_md.
/// It ignores trailing whitespace and our wire-format spans, which the model may add to a user line.
/// `ListItem` keys ignore depth because models flatten nested lists, and `diff::merge` then keeps the user's depth.
pub fn block_key(b: &Block) -> String {
    let raw = match b {
        Block::Heading { level, text } => format!("h{level}:{text}"),
        // Paragraph and ListItem share a key because models re-emit a bare line as a bullet or vice versa.
        // Otherwise the model's copy is tagged AiGrey and the user's original is appended, duplicating the sentence.
        Block::Paragraph { md } => format!("t:{md}"),
        Block::ListItem { md, .. } => format!("t:{md}"),
        Block::CodeBlock { lang, body } => {
            format!("code:{}:{body}", lang.as_deref().unwrap_or(""))
        }
        Block::BlockQuote { md } => format!("bq:{md}"),
        Block::Hr => "hr".into(),
    };
    strip_markers(&raw).trim().to_ascii_lowercase()
}

/// Removes our wire-format spans in one linear scan, leaving other markup intact.
/// A `data-ai-grey` span keeps its inner text, which holds the user's bullet content.
/// A `data-transcript-link` span is dropped whole, including its `↳ HH:MM` text.
/// Stripping `</span>` globally would leave link text behind when the spans nest.
pub fn strip_markers(s: &str) -> String {
    if !s.contains("<span") && !s.contains("</span>") {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'<' && bytes[i..].starts_with(b"<span") {
            if let Some(open_end_rel) = s[i..].find('>') {
                let open_end = i + open_end_rel + 1;
                let open_tag = &s[i..open_end];
                let is_wire_format =
                    open_tag.contains("data-ai-grey") || open_tag.contains("data-transcript-link");
                if is_wire_format {
                    let is_link_span = open_tag.contains("data-transcript-link");
                    let mut depth = 1;
                    let mut j = open_end;
                    while j < bytes.len() && depth > 0 {
                        if bytes[j] == b'<' && s[j..].starts_with("<span") {
                            depth += 1;
                            if let Some(rel) = s[j..].find('>') {
                                let nested_open = &s[j..j + rel + 1];
                                if nested_open.contains("data-transcript-link") {
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
                            let c = char_at(s, j);
                            out.push(c);
                            j += c.len_utf8();
                        } else {
                            j += 1;
                        }
                    }
                    i = j;
                    continue;
                } else {
                    out.push_str(open_tag);
                    i = open_end;
                    continue;
                }
            }
        }
        if bytes[i] == b'<' && s[i..].starts_with("</span>") {
            i += "</span>".len();
            continue;
        }
        let c = char_at(s, i);
        out.push(c);
        i += c.len_utf8();
    }
    out
}

/// Copying `bytes[i] as char` instead turns each byte of a multi-byte char into its own Latin-1 char.
fn char_at(s: &str, i: usize) -> char {
    s[i..].chars().next().expect("i < s.len()")
}

/// Lists are flattened, each `<li>` becoming a `Block::ListItem` with a depth.
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
                // pulldown-cmark emits Start(List) inside an Item with no End(Item) first,
                // so flush the outer item now or the nested item's `buf.clear()` erases its text.
                if let Some(ParseState::ListItem(d)) = state.as_ref().copied() {
                    if !buf.trim().is_empty() {
                        blocks.push(Block::ListItem {
                            md: std::mem::take(&mut buf),
                            depth: d,
                        });
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
                    // Empty for an outer item already flushed at the nested-list Start.
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

    /// Non-ASCII text must survive as whole chars, never `byte as char`.
    #[test]
    fn strip_markers_preserves_non_ascii() {
        let input = "- Today is the user\u{2019}s last day <span data-ai-grey data-ts=\"4\">caf\u{e9} \u{2026} \u{4f1a}\u{8b70} <span data-transcript-link data-ts=\"4\">\u{21b3} 00:04</span></span> done";
        let stripped = strip_markers(input);
        assert_eq!(
            stripped,
            "- Today is the user\u{2019}s last day caf\u{e9} \u{2026} \u{4f1a}\u{8b70}  done"
        );
    }

    #[test]
    fn strip_markers_handles_nested_wire_format_spans() {
        let input = r#"<span data-ai-grey data-ts="120">pricing <span data-transcript-link data-ts="120">↳ 02:00</span></span>"#;
        let stripped = strip_markers(input);
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
        assert!(
            !stripped.contains("<span"),
            "open spans removed, got: {stripped:?}"
        );
        assert!(
            !stripped.contains("</span>"),
            "close spans removed, got: {stripped:?}"
        );
    }

    #[test]
    fn block_key_matches_a_paragraph_the_model_turned_into_a_bullet() {
        let user = Block::Paragraph {
            md: "Follow up with Sarah".into(),
        };
        let llm = Block::ListItem {
            md: "<span data-ai-grey data-ts=\"24\">Follow up with Sarah</span>".into(),
            depth: 0,
        };
        assert_eq!(block_key(&user), block_key(&llm));
    }

    #[test]
    fn parse_preserves_parent_item_with_nested_children() {
        let md = "- pricing\n  - tiered\n  - discount\n";
        let blocks = parse(md);
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
