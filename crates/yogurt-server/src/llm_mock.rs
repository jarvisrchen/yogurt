//! Deterministic mock LLM (CONTEXT D-20).
//!
//! Used by:
//! - The enhance handler (Plan 04-03) when `YOGURT_LLM_*` env vars are not
//!   set — so first-time developers get a working hero experience without
//!   provisioning a real provider.
//! - Tests that need a reproducible enriched-markdown output.
//!
//! The mock builds enriched markdown by:
//! 1. Parsing the `## USER NOTES` / `## TRANSCRIPT` sections out of the
//!    prompt (we know the format because we own `enhance.md`).
//! 2. Echoing the user notes verbatim (preserving the "black ranges are
//!    never overwritten" contract).
//! 3. Appending one AI bullet per transcript segment, wrapped in the
//!    wire-format spans `<span data-ai-grey data-ts="N">…</span>` with a
//!    trailing `<span data-transcript-link data-ts="N">↳ HH:MM</span>`.
//!
//! Phase 5 deletes this file — the real LLM replaces it.

use anyhow::Result;
use serde::Deserialize;

/// Inherent method only (no `LlmClient` trait yet — see CONTEXT D-21).
/// Phase 5 introduces the trait and re-shapes `MockLlm` into a trait impl
/// — or deletes it entirely.
///
/// `#[allow(dead_code)]` because the consumer (the enhance handler) lands
/// in Plan 04-03 — for the few hours between this commit and that wiring
/// the struct is reachable only from its own tests.
#[derive(Default)]
#[allow(dead_code)]
pub struct MockLlm;

#[allow(dead_code)]
impl MockLlm {
    pub async fn complete(&self, _system: &str, user: &str) -> Result<String> {
        let (notes, transcript_json) = split_prompt(user);
        let transcript: Vec<Seg> = serde_json::from_str(transcript_json).unwrap_or_default();

        let mut out = String::new();
        // User notes are preserved verbatim (the merge layer treats them as
        // user-source markdown and pulldown-cmark will escape any HTML on
        // parse). Notes pass through unmodified here; the BL-2 sanitization
        // happens at the enrich-handler layer via `ammonia::clean` on the
        // final enriched_md, AND `render::wrap_ai` HTML-escapes its inner
        // text. Together those two layers neutralize XSS regardless of
        // whether the input came from notes, transcript, or LLM.
        out.push_str(notes.trim_start());
        if !out.ends_with('\n') {
            out.push('\n');
        }
        if !notes.trim().is_empty() && !transcript.is_empty() {
            out.push('\n');
        }
        for seg in &transcript {
            let ts = seg.ts_ms / 1000;
            let stamp = format!("{:02}:{:02}", ts / 60, ts % 60);
            // Mock "summary" = first 8 words of the segment.
            let words: Vec<&str> = seg.text.split_whitespace().take(8).collect();
            let summary_raw = words.join(" ");
            // BL-2: ESCAPE transcript-derived text before interpolating into
            // the wire-format span. Transcript text is network-fed (Deepgram)
            // and untrusted — `<script>` in transcript captions WILL fire as
            // <script> in the browser DOM without this escape.
            let summary = html_escape::encode_safe(&summary_raw);
            out.push_str(&format!(
                "- <span data-ai-grey data-ts=\"{ts}\">{summary} <span data-transcript-link data-ts=\"{ts}\">↳ {stamp}</span></span>\n"
            ));
        }
        Ok(out)
    }
}

fn split_prompt(user: &str) -> (&str, &str) {
    let notes_marker = "## USER NOTES (preserve verbatim, do not wrap)";
    let trans_marker = "## TRANSCRIPT";
    let (Some(n_start), Some(t_start)) = (user.find(notes_marker), user.find(trans_marker)) else {
        // Defensive — if the prompt format changes, return empty/empty so
        // the caller still gets a valid (if uninformative) markdown doc.
        return ("", "[]");
    };
    let notes = &user[n_start + notes_marker.len()..t_start];
    // Skip the heading line of TRANSCRIPT and any blank lines before the JSON.
    let after = &user[t_start..];
    let json_start = after.find('[').unwrap_or(after.len());
    // Trim outer whitespace but PRESERVE leading `-` bullets in user notes —
    // those are part of the markdown the user wrote.
    (notes.trim(), after[json_start..].trim())
}

#[derive(Deserialize)]
struct Seg {
    ts_ms: u64,
    #[allow(dead_code)]
    channel: String,
    text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_echoes_notes_and_adds_one_bullet_per_segment() {
        let mock = MockLlm;
        let prompt = r#"
## USER NOTES (preserve verbatim, do not wrap)

- pricing
- timeline

## TRANSCRIPT

[{"ts_ms":120000,"channel":"mic","text":"We debated the pricing model in detail today"}]
"#;
        let out = mock.complete("", prompt).await.unwrap();
        assert!(out.contains("- pricing"), "user notes preserved: {out}");
        assert!(
            out.contains("data-ai-grey data-ts=\"120\""),
            "ai bullet tagged: {out}"
        );
        assert!(
            out.contains("↳ 02:00"),
            "deep-link timestamp formatted: {out}"
        );
    }

    #[tokio::test]
    async fn it_returns_empty_friendly_doc_when_prompt_lacks_markers() {
        let mock = MockLlm;
        let out = mock
            .complete("", "garbage prompt with no markers")
            .await
            .unwrap();
        // No notes, no transcript JSON → trailing newline only.
        assert!(
            out.trim().is_empty(),
            "expected empty body on malformed prompt, got: {out:?}"
        );
    }

    #[tokio::test]
    async fn it_produces_no_ai_bullets_when_transcript_is_empty() {
        let mock = MockLlm;
        let prompt = r#"
## USER NOTES (preserve verbatim, do not wrap)

- pricing

## TRANSCRIPT

[]
"#;
        let out = mock.complete("", prompt).await.unwrap();
        assert!(out.contains("- pricing"), "notes preserved");
        assert!(
            !out.contains("data-ai-grey"),
            "no AI bullets when transcript empty: {out}"
        );
    }
}
