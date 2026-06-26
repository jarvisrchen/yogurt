//! Transcript timestamp inference for AI-added blocks.
//!
//! Strategy: word-overlap heuristic. For each transcript segment, count how
//! many >3-char words from the segment also appear in the block. Pick the
//! segment with the highest count; tie-break to earliest ts. If no overlap
//! and the transcript is non-empty, fall back to the first segment.

use crate::TranscriptSegment;

pub fn guess_ts_sec(block_md: &str, transcript: &[TranscriptSegment]) -> Option<u64> {
    let block_words: std::collections::HashSet<String> = block_md
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() > 3)
        .map(|s| s.to_string())
        .collect();
    if block_words.is_empty() {
        return transcript.first().map(|s| s.ts_ms / 1000);
    }

    let mut best: Option<(usize, u64)> = None;
    for seg in transcript {
        let count = seg
            .text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| block_words.contains(*w))
            .count();
        if count == 0 {
            continue;
        }
        match best {
            None => best = Some((count, seg.ts_ms / 1000)),
            Some((c, _)) if count > c => best = Some((count, seg.ts_ms / 1000)),
            _ => {}
        }
    }
    best.map(|(_, ts)| ts)
        .or_else(|| transcript.first().map(|s| s.ts_ms / 1000))
}
