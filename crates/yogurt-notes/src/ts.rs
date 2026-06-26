//! Transcript timestamp inference for AI-added blocks.
//! Filled in by Task 2.

use crate::TranscriptSegment;

/// Best-effort: find the transcript segment whose text most overlaps the given
/// block markdown. Returns the segment's ts_ms / 1000 (seconds).
pub fn guess_ts_sec(_block_md: &str, _transcript: &[TranscriptSegment]) -> Option<u64> {
    // Filled in Task 2.
    None
}
