//! Augmented-notes merge logic.
//!
//! Given the user's raw notes, the transcript, and the LLM's enriched markdown,
//! produce a `MergedDoc` that tags each block as either the user's or AI's
//! contribution, with transcript timestamps attached to AI blocks.
//!
//! The diff is **structural** — computed over the markdown AST at block
//! granularity (heading / paragraph / list item / code block / blockquote / hr).
//! It is NOT a character diff (CONTEXT D-07 .. D-10). A second, inline pass
//! (`weave`) then finds the user's note lines *inside* AI blocks - the model
//! is asked to fold a user line into the bullet that expands on it - and
//! records those byte ranges so the render paints only the user's words ink.

pub mod ast;
pub mod diff;
pub mod render;
pub mod ts;
pub mod weave;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Source {
    User,
    AiGrey { transcript_ts_sec: u64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergedBlock {
    pub block: ast::Block,
    pub source: Source,
    /// Byte ranges of the block's marker-stripped text that are the user's
    /// own words woven into an AI block (see `weave`). Empty for `User`
    /// blocks and for AI blocks with none of the user's lines in them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_runs: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergedDoc {
    pub blocks: Vec<MergedBlock>,
}

/// Public API. Merges user notes with the LLM-enriched markdown, attaching
/// transcript timestamps to the AI-added blocks.
///
/// `user_md`: the raw markdown the user typed in the editor.
/// `enriched_md`: the markdown the LLM produced (may or may not already contain
///   `<span data-ai-grey data-ts="N">` markers; we do not require it to).
/// `transcript_json`: the full transcript as JSON — we use it to find a
///   plausible timestamp for any AI block that didn't come back tagged.
pub fn merge_notes(
    user_md: &str,
    enriched_md: &str,
    transcript_json: &str,
) -> anyhow::Result<MergedDoc> {
    let user_blocks = ast::parse(user_md);
    let enriched_blocks = ast::parse(enriched_md);
    let transcript: Vec<TranscriptSegment> =
        serde_json::from_str(transcript_json).unwrap_or_default();

    let merged = diff::merge(&user_blocks, &enriched_blocks, &transcript);
    Ok(MergedDoc { blocks: merged })
}

#[derive(Debug, Clone, Deserialize)]
pub struct TranscriptSegment {
    pub ts_ms: u64,
    pub channel: String,
    pub text: String,
}
