//! Deterministic mock LLM: user notes verbatim plus one AI bullet per transcript segment.
//! Used when no provider is configured and by tests that need reproducible output.

use anyhow::Result;
use async_trait::async_trait;
use futures_util::stream::{self, BoxStream, StreamExt};
use serde::Deserialize;
use yogurt_llm::{ChatChunk, ChatRequest, ChatResponse, LlmClient};

#[derive(Default, Clone)]
pub struct MockLlm;

#[async_trait]
impl LlmClient for MockLlm {
    fn model_name(&self) -> String {
        "mock".into()
    }

    async fn complete(&self, req: ChatRequest) -> Result<ChatResponse> {
        // System messages are ignored by the mock.
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
        // One delta, then the terminal chunk.
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

/// Echoes user notes, then one `<span data-ai-grey>` bullet per transcript segment ending in a
/// `<span data-transcript-link>` timestamp.
fn build_mock_output(user: &str) -> String {
    let has_markers = user.contains("<user_notes>");
    let (notes, transcript_json) = split_prompt(user);
    let transcript: Vec<Seg> = serde_json::from_str(transcript_json).unwrap_or_default();

    let mut out = String::new();
    // The prompt asks the model to name the note format on the first line. The mock picks
    // standup from the word alone to exercise auto-detect. Skipped on a marker-less prompt so
    // a malformed prompt still yields an empty document.
    if has_markers {
        let standup = transcript
            .iter()
            .any(|s| s.text.to_ascii_lowercase().contains("standup"));
        out.push_str(if standup {
            "template: standup\n\n"
        } else {
            "template: general\n\n"
        });
    }
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
        let words: Vec<&str> = seg.text.split_whitespace().take(8).collect();
        let summary_raw = words.join(" ");
        // Transcript text is network-fed and untrusted, so escape it before it enters the span.
        let summary = html_escape::encode_safe(&summary_raw);
        out.push_str(&format!(
            "- <span data-ai-grey data-ts=\"{ts}\">{summary} <span data-transcript-link data-ts=\"{ts}\">↳ {stamp}</span></span>\n"
        ));
    }
    out
}

fn split_prompt(user: &str) -> (&str, &str) {
    // `rfind` because the template's rules mention the literal tag names before the real wrappers.
    let notes = match (user.rfind("<user_notes>"), user.rfind("</user_notes>")) {
        (Some(start), Some(end)) if start < end => &user[start + "<user_notes>".len()..end],
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
        assert!(
            out.starts_with("template: general\n\n"),
            "names the note format first: {out}"
        );
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
