//! Augmented-notes merge: tags each block as the user's or the AI's, with transcript timestamps on AI blocks.
//! The diff is structural, over markdown AST blocks, not a character diff.
//! A second inline pass (`weave`) finds the user's note lines inside AI blocks so the render paints only those words ink.

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
    /// Byte ranges of the marker-stripped text that are the user's words woven into an AI block.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_runs: Vec<(usize, usize)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MergedDoc {
    pub blocks: Vec<MergedBlock>,
}

/// `enriched_md` may or may not already contain `<span data-ai-grey data-ts="N">` markers.
/// The transcript supplies a plausible timestamp for any AI block that came back untagged.
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
