//! Model-agnostic stripper for inline chain-of-thought blocks emitted by
//! reasoning models (DeepSeek R1, MiniMax M3, Qwen QwQ, …).
//!
//! Most reasoning models surface their internal reasoning inside the
//! OpenAI-compat `delta.content` (or `message.content`) field, wrapped in
//! `<think>…</think>` (dominant) or its `<thinking>…</thinking>` /
//! `<reason>…</reason>` aliases. A handful of providers (DeepSeek with
//! `reasoning_content`, MiniMax with `reasoning_split`) instead route the
//! reasoning into a sibling field — the crate-level client parses and
//! discards that field elsewhere.
//!
//! ## Two surfaces
//!
//! - [`ThinkStripper`]: stateful filter for streaming chunks. Holds back
//!   up to `MAX_TAG_LEN - 1` chars between calls so a tag split across
//!   chunk boundaries (`"<th"` then `"ink>…"` then `"</think>"`) never
//!   reaches the UI.
//! - [`strip_thinking`]: one-shot whole-string filter for non-streaming
//!   responses. Used by `OpenAiCompatClient::complete`.
//!
//! ## Why provider-agnostic
//!
//! The filter looks at the byte shape of the input, not the model name or
//! the base URL. Any model that wraps its reasoning in the recognized
//! tags gets the same treatment — switching providers or upgrading a
//! model can't reintroduce the leak.

/// Open-tag set. The longest open tag (`<thinking>`) is 10 chars, so the
/// streaming stripper defers 9 chars at chunk boundaries to disambiguate
/// a real tag from a partial one.
const OPEN_TAGS: &[&str] = &["<think>", "<thinking>", "<reason>"];

/// Close-tag set. Longest is `</thinking>` at 11 chars.
const CLOSE_TAGS: &[&str] = &["</think>", "</thinking>", "</reason>"];

/// Longest tag in either set, used to size the streaming deferral window.
const MAX_TAG_LEN: usize = 11;

/// One-shot whole-string filter. Strips every `<think>…</think>` (and
/// `<thinking>…</thinking>`, `<reason>…</reason>`) span in `s`, including
/// spans that appear inline among visible text.
///
/// **Whitespace handling**: leading whitespace immediately after a
/// stripped block is dropped — but only when no visible text has been
/// emitted yet. Reasoning models open with `<think>…</think>\n\n` and
/// then the answer; the `\n\n` is reasoning artifact, not content, and
/// should not appear in the chat bubble. Inline think blocks between
/// visible words (`"Hello <think>foo</think> world"`) preserve the
/// inter-word space — collapsing it would render as `"Helloworld"`.
pub fn strip_thinking(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    let mut inside = false;
    let mut out_so_far_is_whitespace = true;
    loop {
        let tags = if inside { CLOSE_TAGS } else { OPEN_TAGS };
        let next = find_any(rest, tags);
        match next {
            Some((pos, len)) => {
                if !inside {
                    let visible_prefix = &rest[..pos];
                    out.push_str(visible_prefix);
                    if has_non_whitespace(visible_prefix) {
                        out_so_far_is_whitespace = false;
                    }
                    rest = &rest[pos + len..];
                    inside = true;
                } else {
                    rest = &rest[pos + len..];
                    inside = false;
                    // Just transitioned inside → outside. Drop leading
                    // whitespace from `rest` only if nothing visible
                    // has been emitted yet — otherwise preserve the gap.
                    if out_so_far_is_whitespace {
                        rest = rest.trim_start();
                    }
                }
            }
            None => {
                if !inside {
                    out.push_str(rest);
                }
                break;
            }
        }
    }
    out
}

/// Stateful streaming filter. Each `push` consumes one or more SSE deltas
/// worth of text and returns the visible portion (think blocks elided).
///
/// Tags may be split across chunks (e.g. `"<th"` then `"ink>…"` then
/// `"</think>"`); the stripper holds back up to `MAX_TAG_LEN - 1` chars
/// at a boundary so a partial prefix never reaches the caller. When the
/// stream ends mid-think, [`flush`](Self::flush) returns whatever
/// trailing bytes couldn't be classified yet so the chat handler can
/// decide whether to drop or surface them.
pub struct ThinkStripper {
    inside: bool,
    /// Trailing bytes held back because they might be a partial open tag
    /// (`!inside`) or partial close tag (`inside`). Always at most
    /// `MAX_TAG_LEN - 1` chars after each `push` — bounded.
    defer: String,
    /// True until the first non-whitespace visible text is emitted.
    /// Gates the post-think-block whitespace trim: we drop `\n\n`
    /// reasoning artifacts at the start of the response, but preserve
    /// the inter-word gap when a think block lands inline between
    /// visible tokens.
    output_started: bool,
}

impl Default for ThinkStripper {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkStripper {
    pub fn new() -> Self {
        Self {
            inside: false,
            defer: String::new(),
            output_started: false,
        }
    }

    /// Feed a chunk and return the visible text that should be shown to
    /// the user. Think-block contents are discarded; partial-tag suffixes
    /// are held back internally until the next chunk (or [`flush`]) resolves
    /// them.
    ///
    /// Algorithm: append to a defer buffer, then drain it left-to-right.
    /// Outside-think, every byte before the first `<' that could start a
    /// known tag is safe to emit — `'<3'` and `'<x>'` aren't think tags,
    /// they're literal text. Only `<'` followed by `'t'` (think/think​ing)
    /// or `'r'` (reason) is a possible tag prefix; we hold back from there
    /// until we either see a complete tag (strip it) or see a character
    /// that rules one out (emit everything).
    ///
    /// When a close tag fires we also drop any leading whitespace from
    /// the remainder — reasoning models wrap the answer in blank lines
    /// (`\n\n` between `</think>` and the response). That whitespace is
    /// reasoning artifact, not user-visible content, and the chat
    /// handler would otherwise render it as a leading gap in the bubble.
    pub fn push(&mut self, delta: &str) -> String {
        self.defer.push_str(delta);
        let mut out = String::new();

        loop {
            let tags = if self.inside { CLOSE_TAGS } else { OPEN_TAGS };
            match find_any(&self.defer, tags) {
                Some((pos, len)) => {
                    let was_inside = self.inside;
                    if !self.inside {
                        // Emit any visible text before the opening tag.
                        let visible_prefix = &self.defer[..pos];
                        out.push_str(visible_prefix);
                        if has_non_whitespace(visible_prefix) {
                            self.output_started = true;
                        }
                    }
                    self.defer.drain(..pos + len);
                    self.inside = !self.inside;
                    // Just transitioned inside → outside: trim the
                    // blank-line whitespace reasoning models emit before
                    // the answer, but only when no visible text has been
                    // emitted yet. Inline think blocks preserve the gap.
                    if was_inside && !self.inside && !self.output_started {
                        let trimmed = self.defer.trim_start();
                        let consumed = self.defer.len() - trimmed.len();
                        if consumed > 0 {
                            self.defer.drain(..consumed);
                        }
                    }
                }
                None => {
                    // No complete tag in the buffer. Decide what we can
                    // emit and what to hold back.
                    let boundary = self.safe_emit_end();
                    if self.inside {
                        // Inside a think block: everything before the
                        // safe boundary is junk. Drop it; keep the tail
                        // (which may contain the start of a close tag).
                        self.defer.drain(..boundary);
                    } else {
                        // Outside a think block: everything before the
                        // safe boundary is unambiguously visible. Emit
                        // it, keep the tail.
                        let visible = &self.defer[..boundary];
                        out.push_str(visible);
                        if has_non_whitespace(visible) {
                            self.output_started = true;
                        }
                        self.defer.drain(..boundary);
                    }
                    break;
                }
            }
        }

        out
    }

    /// The largest index `n` such that `self.defer[..n]` is guaranteed to
    /// never become part of a think tag (or always be inside one). Beyond
    /// `n` we either hold back (outside) or drop (inside).
    ///
    /// **Outside**: walk the buffer; at each `<'` check whether it could
    /// still become a known open tag (`<'` is a prefix of an open tag, or
    /// an open tag is a prefix of `defer[i..]`). If yes, hold back from
    /// `i`. If no, the `<'` is literal visible text and we keep scanning.
    ///
    /// **Inside**: only the trailing `MAX_TAG_LEN - 1` bytes can possibly
    /// contain a close-tag prefix; everything before is junk.
    fn safe_emit_end(&self) -> usize {
        if self.inside {
            self.defer.len().saturating_sub(MAX_TAG_LEN - 1)
        } else {
            let bytes = self.defer.as_bytes();
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'<' && could_become_tag(&self.defer[i..], OPEN_TAGS) {
                    return i;
                }
                i += 1;
            }
            self.defer.len()
        }
    }

    /// Drain whatever couldn't be classified as a visible think-block
    /// fragment and return it verbatim. Called once at end-of-stream so
    /// the chat handler can decide policy: drop the suffix (clean
    /// model), or surface it as visible text (user toggled "show
    /// reasoning"). Returns `String::new()` when nothing was held back.
    pub fn flush(&mut self) -> String {
        std::mem::take(&mut self.defer)
    }
}

/// Find the earliest occurrence of any tag in `tags` within `s`. Returns
/// `(byte_offset, tag_len)` — the caller slices using its own tag list.
fn find_any(s: &str, tags: &[&str]) -> Option<(usize, usize)> {
    let mut best: Option<(usize, usize)> = None;
    for tag in tags {
        let len = tag.len();
        if let Some(pos) = s.find(tag) {
            if best.is_none_or(|(b, _)| pos < b) {
                best = Some((pos, len));
            }
        }
    }
    best
}

/// Could `s` ever grow into (or already equal) one of `tags`? True when
/// either `s` is a prefix of some tag (more chunks might complete it) or
/// some tag is a prefix of `s` (it's already a tag we should strip). This
/// is what decides whether to hold back a `<'` at a chunk boundary or
/// emit it as literal text.
fn could_become_tag(s: &str, tags: &[&str]) -> bool {
    tags.iter().any(|t| t.starts_with(s) || s.starts_with(t))
}

/// `true` when `s` contains any non-whitespace character. Used to
/// decide whether the stripper has emitted real output yet — empty
/// or whitespace-only prefixes don't flip the "output started" flag.
fn has_non_whitespace(s: &str) -> bool {
    s.bytes().any(|b| !b.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── strip_thinking (whole-string) ─────────────────────────────────

    #[test]
    fn strip_thinking_leaves_text_without_think_blocks_alone() {
        assert_eq!(strip_thinking("Hello world."), "Hello world.");
    }

    #[test]
    fn strip_thinking_removes_leading_think_block() {
        assert_eq!(
            strip_thinking("<think>let me think</think>The answer is 42."),
            "The answer is 42."
        );
    }

    #[test]
    fn strip_thinking_removes_inline_think_block_keeping_surrounding_text() {
        assert_eq!(
            strip_thinking("Hello <think>reasoning</think> world"),
            "Hello  world"
        );
    }

    #[test]
    fn strip_thinking_removes_multiple_consecutive_think_blocks() {
        assert_eq!(
            strip_thinking("<think>a</think><think>b</think>final"),
            "final"
        );
    }

    #[test]
    fn strip_thinking_handles_thinking_alias() {
        assert_eq!(
            strip_thinking("<thinking>hidden</thinking>visible"),
            "visible"
        );
    }

    #[test]
    fn strip_thinking_handles_multiline_think_content() {
        // Leading whitespace after the close tag is dropped when no
        // visible text was emitted before the think block — reasoning
        // models wrap their answer in blank lines and those lines
        // should not appear in the chat bubble.
        assert_eq!(
            strip_thinking("<think>\nline 1\nline 2\n</think>\n\nanswer"),
            "answer"
        );
    }

    #[test]
    fn strip_thinking_drops_unterminated_think_block() {
        // Malformed input — open tag with no close tag. Drop the opening
        // and everything after it, leaving whatever was before.
        assert_eq!(strip_thinking("before<think>never closes"), "before");
    }

    // ── ThinkStripper (streaming, stateful) ───────────────────────────

    #[test]
    fn stripper_passes_clean_through_chunk_by_chunk() {
        let mut s = ThinkStripper::new();
        assert_eq!(s.push("Hel"), "Hel");
        assert_eq!(s.push("lo "), "lo ");
        assert_eq!(s.push("world."), "world.");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_drops_entire_think_block_when_in_one_chunk() {
        let mut s = ThinkStripper::new();
        // Tag + content + close tag all in one delta — the simplest case.
        assert_eq!(s.push("<think>hidden</think>visible"), "visible");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_splits_open_tag_across_chunks() {
        let mut s = ThinkStripper::new();
        // Open tag itself arrives split; reasoning body may arrive split;
        // close tag arrives split.
        assert_eq!(s.push("<th"), "");
        assert_eq!(s.push("ink>rea"), "");
        assert_eq!(s.push("son</th"), "");
        assert_eq!(s.push("ink>visible"), "visible");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_splits_close_tag_across_chunks() {
        let mut s = ThinkStripper::new();
        assert_eq!(s.push("<think>rea"), "");
        assert_eq!(s.push("son</th"), "");
        assert_eq!(s.push("ink>visible"), "visible");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_handles_thinking_alias_across_chunks() {
        let mut s = ThinkStripper::new();
        assert_eq!(s.push("<thinking>hid"), "");
        assert_eq!(s.push("den</thinking>vis"), "vis");
        assert_eq!(s.push("ible"), "ible");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_holds_back_partial_open_tag_at_end_then_resolves_to_not_a_tag() {
        // A '<' that COULD start a known open tag (followed by 't' or
        // 'r') is held back from the visible stream because the next
        // chunk might complete it. A '<' followed by anything else
        // (digit, letter not in our tag set, etc.) is unambiguously
        // visible text and emits immediately.
        let mut s = ThinkStripper::new();
        // "abc<t" — '<' followed by 't' COULD be <think>; hold back from
        // there. Emit "abc".
        assert_eq!(s.push("abc<t"), "abc");
        // "ext" — not the start of any known tag, so the held-back "<t"
        // plus "ext" all stream out.
        assert_eq!(s.push("ext"), "<text");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_emits_visible_text_then_enters_think_block() {
        let mut s = ThinkStripper::new();
        assert_eq!(s.push("hello<think>"), "hello");
        assert_eq!(s.push("inside</think>"), "");
        assert_eq!(s.push(" after"), " after");
        assert_eq!(s.flush(), "");
    }

    #[test]
    fn stripper_flush_drops_unresolved_partial_tag() {
        // Stream ended mid-tag without enough context to resolve it.
        // The defer buffer is returned to the caller so it can decide
        // policy: today the chat handler drops it; a future "show
        // reasoning" toggle could surface it instead.
        let mut s = ThinkStripper::new();
        s.push("hello <thinking");
        assert_eq!(s.flush(), "<thinking");
    }
}
