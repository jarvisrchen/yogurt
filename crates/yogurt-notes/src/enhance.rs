//! What the Mac and the Cloud webapp both do with an enhance reply: clean it, merge it with the user's notes, and render `enriched_md`.
//! Keeping it here, beside the merge, is what keeps the two byte-identical.

use crate::{merge_notes, render, sanitize::sanitize_enriched_md, MergedDoc, TranscriptSegment};
use yogurt_prompts::{split_template_line, thinking::strip_thinking, TEMPLATE_IDS};

/// A meeting with no typed notes and fewer transcript words than this is an
/// accidental start/stop. Deliberately low: a real single sentence
/// ("today is my last day, thanks everyone") must still enhance.
const TOO_SHORT_TRANSCRIPT_WORDS: usize = 6;

/// Whether the meeting is an accidental start/stop that enhance skips.
pub fn too_short(notes_md: &str, transcript_json: &str) -> bool {
    let words: usize = serde_json::from_str::<Vec<TranscriptSegment>>(transcript_json)
        .unwrap_or_default()
        .iter()
        .map(|seg| seg.text.split_whitespace().count())
        .sum();
    notes_md.trim().is_empty() && words < TOO_SHORT_TRANSCRIPT_WORDS
}

#[derive(Debug)]
pub struct Enhanced {
    pub doc: MergedDoc,
    /// Rendered and sanitized, with the wire-format spans.
    pub enriched_md: String,
    /// The forced format, or the one the model picked, or `general` when it named nothing usable.
    pub template: &'static str,
    /// Set when the model's own pick was unusable and `template` fell back to `general`, with what it named.
    pub unrecognized_pick: Option<Option<String>>,
}

#[derive(Debug, thiserror::Error)]
pub enum EnhanceError {
    /// `merge_notes(notes, "")` returns the notes untouched, which would be stored as the "enhanced" document.
    #[error("the model returned no text")]
    EmptyReply,
    #[error("merge_notes: {0}")]
    Merge(anyhow::Error),
}

/// Turns the model's whole reply into the stored document.
/// `llm_output` is the reply as streamed, which may still carry the `template:` line, `<think>` blocks, and mojibake.
pub fn finish(
    notes_md: &str,
    transcript_json: &str,
    llm_output: &str,
    forced_template: Option<&'static str>,
) -> Result<Enhanced, EnhanceError> {
    // Some models (MiniMax-Text-01) emit CP1252-mangled punctuation and echo the
    // prompt's section scaffolding, so scrub both before storing.
    // A forced format wins over the model's `template:` line. In auto mode an
    // unknown or missing id falls back to `general` since the document is still good.
    let llm_output = strip_thinking(&fix_model_mojibake(llm_output));
    let (picked, body) = split_template_line(&llm_output);
    let mut unrecognized_pick = None;
    let template: &'static str = match forced_template {
        Some(id) => id,
        None => {
            let picked = picked.map(|p| p.to_ascii_lowercase());
            match TEMPLATE_IDS
                .iter()
                .copied()
                .find(|t| Some(*t) == picked.as_deref())
            {
                Some(known) => known,
                None => {
                    unrecognized_pick = Some(picked);
                    TEMPLATE_IDS[0]
                }
            }
        }
    };
    let body = strip_prompt_scaffolding(body);
    if body.trim().is_empty() {
        return Err(EnhanceError::EmptyReply);
    }
    let doc = merge_notes(notes_md, &body, transcript_json).map_err(EnhanceError::Merge)?;
    // Strip LLM-hallucinated script/img/iframe/on* handlers/javascript: URLs. The wire-format spans survive.
    let enriched_md = sanitize_enriched_md(&render::to_markdown(&doc));
    Ok(Enhanced {
        doc,
        enriched_md,
        template,
        unrecognized_pick,
    })
}

/// Drop lines where the model echoed the enhance prompt's scaffolding (section
/// wrappers, instruction parentheticals, `---` separators). Lines are judged with
/// `<...>` tags removed, so a header wrapped in marker spans is still caught.
fn strip_prompt_scaffolding(s: &str) -> String {
    fn without_tags(line: &str) -> String {
        let mut out = String::new();
        let mut depth = 0u32;
        for c in line.chars() {
            match c {
                '<' => depth += 1,
                '>' => depth = depth.saturating_sub(1),
                _ if depth == 0 => out.push(c),
                _ => {}
            }
        }
        out
    }
    s.lines()
        .filter(|line| {
            // Checked before tag-stripping, which would reduce these to kept blank lines.
            let raw = line.trim().to_lowercase();
            if matches!(
                raw.as_str(),
                "<user_notes>" | "</user_notes>" | "<transcript>" | "</transcript>"
            ) {
                return false;
            }
            let flat = without_tags(line);
            let flat = flat.trim();
            if flat.is_empty() {
                // Pure markup (e.g. an empty marker span) is dropped; real blanks are kept.
                return line.trim().is_empty();
            }
            let lower = flat.to_lowercase();
            // `---` separators: alone, as a bullet/heading, or followed only by a
            // `\u{21b3} mm:ss` transcript-link stub.
            let residue: String = flat
                .chars()
                .filter(|c| {
                    !matches!(c, '#' | '-' | '*' | ' ' | '\u{21b3}' | ':') && !c.is_ascii_digit()
                })
                .collect();
            let has_dashes = flat.contains("---");
            let is_separator =
                residue.is_empty() && (has_dashes || flat.trim_matches(['#', '*', ' ']).is_empty());
            let is_scaffold = lower.contains("user notes")
                && (lower.starts_with('#') || lower.contains("preserve verbatim"))
                || lower.starts_with("## transcript")
                || lower.contains("source for your additions");
            !(is_separator || is_scaffold)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Repair mojibake the model emits as literal tokens (observed from MiniMax in both
/// the CP1252 form "\u{e2}\u{20ac}\u{a6}" and the Latin-1 form with raw C1 controls).
/// Maps each char back to its source byte and decodes runs that form valid UTF-8.
/// ASCII and legitimate accented text pass through, since a lone "\u{e2}" followed
/// by ASCII never forms a valid sequence.
fn fix_model_mojibake(s: &str) -> String {
    fn mangled_byte(c: char) -> Option<u8> {
        let u = c as u32;
        if (0x80..=0xFF).contains(&u) {
            return Some(u as u8); // Latin-1 range, incl. raw C1 controls
        }
        // CP1252's 0x80-0x9F remappings.
        Some(match c {
            '\u{20AC}' => 0x80,
            '\u{201A}' => 0x82,
            '\u{0192}' => 0x83,
            '\u{201E}' => 0x84,
            '\u{2026}' => 0x85,
            '\u{2020}' => 0x86,
            '\u{2021}' => 0x87,
            '\u{02C6}' => 0x88,
            '\u{2030}' => 0x89,
            '\u{0160}' => 0x8A,
            '\u{2039}' => 0x8B,
            '\u{0152}' => 0x8C,
            '\u{017D}' => 0x8E,
            '\u{2018}' => 0x91,
            '\u{2019}' => 0x92,
            '\u{201C}' => 0x93,
            '\u{201D}' => 0x94,
            '\u{2022}' => 0x95,
            '\u{2013}' => 0x96,
            '\u{2014}' => 0x97,
            '\u{02DC}' => 0x98,
            '\u{2122}' => 0x99,
            '\u{0161}' => 0x9A,
            '\u{203A}' => 0x9B,
            '\u{0153}' => 0x9C,
            '\u{017E}' => 0x9E,
            '\u{0178}' => 0x9F,
            _ => return None,
        })
    }

    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let lead = mangled_byte(c).filter(|b| (0xC2..=0xF4).contains(b));
        if let Some(lead_byte) = lead {
            let need = match lead_byte {
                0xC2..=0xDF => 1,
                0xE0..=0xEF => 2,
                _ => 3,
            };
            let mut bytes = vec![lead_byte];
            for j in 1..=need {
                match chars.get(i + j).copied().and_then(mangled_byte) {
                    Some(b) if (0x80..=0xBF).contains(&b) => bytes.push(b),
                    _ => break,
                }
            }
            if bytes.len() == need + 1 {
                if let Ok(decoded) = std::str::from_utf8(&bytes) {
                    out.push_str(decoded);
                    i += need + 1;
                    continue;
                }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{fix_model_mojibake, strip_prompt_scaffolding};

    #[test]
    fn scrubs_cp1252_variant() {
        let got = fix_model_mojibake(
            "October.\u{e2}\u{20ac}\u{a6} then \u{e2}\u{20ac}\u{153}ship\u{e2}\u{20ac}\u{9d}",
        );
        assert_eq!(got, "October.\u{2026} then \u{201c}ship\u{201d}");
    }

    #[test]
    fn scrubs_latin1_control_variant() {
        let got = fix_model_mojibake("let\u{e2}\u{80}\u{99}s see and\u{e2}\u{80}\u{a6} done");
        assert_eq!(got, "let\u{2019}s see and\u{2026} done");
    }

    #[test]
    fn strips_echoed_prompt_scaffolding() {
        let raw = "## USER NOTES (preserve verbatim, do not wrap)\n\n                   <span data-ai-grey data-ts=\"3\">--- <span data-transcript-link data-ts=\"3\">\u{21b3} 00:03</span></span>\n\n                   ## TRANSCRIPT (source for your additions; ts_ms is millis since meeting start)\n\n                   - <span data-ai-grey data-ts=\"30\">12:00: Staff meeting <span data-transcript-link data-ts=\"30\">\u{21b3} 00:30</span></span>\n                   <user_notes>\n</user_notes>";
        let got = strip_prompt_scaffolding(raw);
        assert!(!got.contains("USER NOTES"), "got: {got}");
        assert!(!got.contains("TRANSCRIPT ("), "got: {got}");
        assert!(!got.contains("---"), "got: {got}");
        assert!(!got.contains("<user_notes>"), "got: {got}");
        assert!(
            got.contains("12:00: Staff meeting"),
            "real bullet kept: {got}"
        );
    }

    #[test]
    fn keeps_normal_headings_and_bullets() {
        let doc = "## Decisions\n- <span data-ai-grey data-ts=\"16\">Pro tier: $20/month</span>\n\n## Action items\n- follow up";
        assert_eq!(strip_prompt_scaffolding(doc), doc);
    }

    #[test]
    fn clean_text_passes_through_untouched() {
        let s = "normal text with real \u{2026} and \u{201c}quotes\u{201d} and \u{21b3} 00:16";
        assert_eq!(fix_model_mojibake(s), s);
        let fr = "c\u{e2}ble and caf\u{e9} \u{e0} Paris";
        assert_eq!(fix_model_mojibake(fr), fr);
    }
}
