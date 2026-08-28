//! Deterministic mock LLM (CONTEXT D-20).
//!
//! Phase 5 update (Plan 05-01): now implements `yogurt_llm::LlmClient` so
//! the enhance handler can hold an `Arc<dyn LlmClient>` and swap mock vs.
//! real adapter without branching at call-sites. The mock still produces
//! the same wire-format output (USER notes verbatim + one AI bullet per
//! transcript segment) so Phase 4 fixture tests keep passing.
//!
//! Used by:
//! - The enhance handler when `YOGURT_LLM_*` env vars are not set (and
//!   Plan 05-02's `AppState.keys` lookup also misses) — so first-time
//!   developers get a working hero experience without a real provider.
//! - Tests that need a reproducible enriched-markdown output.
//!
//! `streaming` is intentionally minimal here — Phase 6 will exercise the
//! real `OpenAiCompatClient::stream` directly; the mock returning a
//! single chunk is enough to satisfy the trait bound.

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use yogurt_llm::{ChatChunk, ChatRequest, ChatResponse, LlmClient};

/// Trait impl of the Phase 4 mock — re-shaped per CONTEXT D-21.
#[derive(Default, Clone)]
pub struct MockLlm;

#[async_trait]
impl LlmClient for MockLlm {
    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse> {
        // Phase 4 wraps the user-facing payload as a single `user`-role
        // message containing the rendered enhance.md prompt. We rebuild
        // the legacy prompt string by concatenating user-role contents
        // (system messages are advisory only for the mock).
        let user_text = req
            .messages
            .iter()
            .filter(|m| m.role == "user")
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let content = build_mock_output(&user_text);
        Ok(ChatResponse {
            content,
            model: "mock-llm".to_string(),
        })
    }

    async fn stream(&self, req: ChatRequest) -> Result<BoxStream<'static, Result<ChatChunk>>> {
        // Single-shot stream: emit the full mock content as one delta then
        // terminate with `done=true`. Sufficient for Phase 4-compatible
        // tests that just want to round-trip through the trait surface.
        let full = self.complete(req).await?.content;
        let s = stream::iter(vec![
            Ok(ChatChunk {
                delta: full,
                done: false,
            }),
            Ok(ChatChunk {
                delta: String::new(),
                done: true,
            }),
        ]);
        Ok(s.boxed())
    }
}

/// Phase 4-shape mock output: echo user notes verbatim, then emit one
/// `<span data-ai-grey>` bullet per transcript segment with a trailing
/// `<span data-transcript-link>↳ HH:MM</span>`. Public-in-module so the
/// trait impl can reuse it; not exported from the crate.
fn build_mock_output(user: &str) -> String {
    let (notes, transcript_json) = split_prompt(user);
    let transcript: Vec<Seg> = serde_json::from_str(transcript_json).unwrap_or_default();

    let mut out = String::new();
    // User notes preserved verbatim — the merge layer treats them as
    // user-source markdown and pulldown-cmark escapes any HTML on parse.
    // BL-2 sanitization happens at the enrich-handler layer via
    // `ammonia::clean` on the final enriched_md.
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
        // BL-2: escape transcript-derived text BEFORE interpolating into
        // the wire-format span. Transcript text is network-fed (Deepgram)
        // and untrusted.
        let summary = html_escape::encode_safe(&summary_raw);
        out.push_str(&format!(
            "- <span data-ai-grey data-ts=\"{ts}\">{summary} <span data-transcript-link data-ts=\"{ts}\">↳ {stamp}</span></span>\n"
        ));
    }
    out
}

fn split_prompt(user: &str) -> (&str, &str) {
    // The enhance template wraps inputs in `<user_notes>`/`<transcript>`
    // tags. `rfind` because the template's RULES also mention the literal
    // tag names (in backticks) well before the real wrappers appear.
    let notes = match (user.rfind("<user_notes>"), user.rfind("</user_notes>")) {
        (Some(start), Some(end)) if start < end => &user[start + "<user_notes>".len()..end],
        // Defensive — if the prompt format changes, return empty/empty so
        // the caller still gets a valid (if uninformative) markdown doc.
        _ => return ("", "[]"),
    };
    let transcript = match (user.rfind("<transcript>"), user.rfind("</transcript>")) {
        (Some(start), Some(end)) if start < end => {
            let inner = &user[start + "<transcript>".len()..end];
            let json_start = inner.find('[').unwrap_or(inner.len());
            &inner[json_start..]
        }
        _ => "[]",
    };
    // Trim outer whitespace but PRESERVE leading `-` bullets in user notes.
    (notes.trim(), transcript.trim())
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
    use yogurt_llm::ChatMessage;

    #[tokio::test]
    async fn it_echoes_notes_and_adds_one_bullet_per_segment_via_trait() {
        let mock = MockLlm;
        let prompt = r#"
<user_notes>
- pricing
- timeline
</user_notes>

<transcript>
[{"ts_ms":120000,"channel":"mic","text":"We debated the pricing model in detail today"}]
</transcript>
"#;
        let resp = mock
            .complete(ChatRequest {
                messages: vec![ChatMessage::user(prompt)],
                stream: false,
            })
            .await
            .unwrap();
        let out = &resp.content;
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
    async fn produces_no_ai_bullets_when_transcript_is_empty_via_trait() {
        let mock = MockLlm;
        let prompt = r#"
<user_notes>
- pricing
</user_notes>

<transcript>
[]
</transcript>
"#;
        let resp = mock
            .complete(ChatRequest {
                messages: vec![ChatMessage::user(prompt)],
                stream: false,
            })
            .await
            .unwrap();
        assert!(resp.content.contains("- pricing"), "notes preserved");
        assert!(
            !resp.content.contains("data-ai-grey"),
            "no AI bullets when transcript empty: {}",
            resp.content
        );
    }

    #[tokio::test]
    async fn returns_empty_friendly_doc_when_prompt_lacks_markers() {
        let mock = MockLlm;
        let resp = mock
            .complete(ChatRequest {
                messages: vec![ChatMessage::user("garbage prompt with no markers")],
                stream: false,
            })
            .await
            .unwrap();
        assert!(
            resp.content.trim().is_empty(),
            "expected empty body on malformed prompt, got: {:?}",
            resp.content
        );
    }

    #[tokio::test]
    async fn stream_terminates_with_done_chunk() {
        use futures_util::StreamExt;
        let mock = MockLlm;
        let mut s = mock
            .stream(ChatRequest {
                messages: vec![ChatMessage::user("test")],
                stream: true,
            })
            .await
            .unwrap();
        let mut saw_done = false;
        while let Some(chunk) = s.next().await {
            let chunk = chunk.unwrap();
            if chunk.done {
                saw_done = true;
            }
        }
        assert!(saw_done, "mock stream must emit a terminal done=true chunk");
    }
}
