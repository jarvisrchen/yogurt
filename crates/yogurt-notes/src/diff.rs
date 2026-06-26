//! Structural block-level diff/merge between user_md and enriched_md.
//!
//! Algorithm:
//!   1. Build a HashMap of `block_key(b) -> &Block` for every user block.
//!   2. Walk enriched_blocks in order:
//!        - If the key is in user_set, emit `Source::User` using the user's
//!          exact block text (preserves trailing whitespace / formatting).
//!        - Otherwise emit `Source::AiGrey { transcript_ts_sec }` with a
//!          best-effort transcript ts from `ts::guess_ts_sec`.
//!   3. Append any user blocks the LLM dropped at the end as `Source::User`
//!      (defensive — never lose user text).

use crate::ast::{block_key, Block};
use crate::{ts, MergedBlock, Source, TranscriptSegment};

pub fn merge(
    user: &[Block],
    enriched: &[Block],
    transcript: &[TranscriptSegment],
) -> Vec<MergedBlock> {
    use std::collections::{HashMap, HashSet};

    let mut user_by_key: HashMap<String, &Block> = HashMap::new();
    for b in user {
        user_by_key.insert(block_key(b), b);
    }

    let mut seen_user_keys: HashSet<String> = HashSet::new();
    let mut out: Vec<MergedBlock> = Vec::with_capacity(enriched.len() + 4);

    for b in enriched {
        let k = block_key(b);
        if let Some(user_block) = user_by_key.get(&k) {
            seen_user_keys.insert(k);
            out.push(MergedBlock {
                block: (*user_block).clone(),
                source: Source::User,
            });
        } else {
            let body_text = block_md_text(b);
            let ts_sec = ts::guess_ts_sec(&body_text, transcript).unwrap_or(0);
            out.push(MergedBlock {
                block: b.clone(),
                source: Source::AiGrey {
                    transcript_ts_sec: ts_sec,
                },
            });
        }
    }

    // Defensive: append any user blocks the LLM dropped.
    for b in user {
        let k = block_key(b);
        if !seen_user_keys.contains(&k) {
            out.push(MergedBlock {
                block: b.clone(),
                source: Source::User,
            });
        }
    }

    out
}

fn block_md_text(b: &Block) -> String {
    match b {
        Block::Heading { text, .. } => text.clone(),
        Block::Paragraph { md } | Block::ListItem { md, .. } | Block::BlockQuote { md } => {
            md.clone()
        }
        Block::CodeBlock { body, .. } => body.clone(),
        Block::Hr => String::new(),
    }
}
