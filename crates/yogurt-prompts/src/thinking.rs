//! Removes leading `<think>...</think>` reasoning from LLM responses.

const OPEN_TAG: &str = "<think>";
const CLOSE_TAG: &str = "</think>";

/// Remove one or more complete reasoning blocks from the start of a response.
/// Tags inside visible text are preserved because they may be part of an answer.
pub fn strip_thinking(input: &str) -> String {
    let mut visible = input;
    loop {
        let trimmed = visible.trim_start();
        let Some(reasoning) = trimmed.strip_prefix(OPEN_TAG) else {
            return visible.to_string();
        };
        let Some(end) = reasoning.find(CLOSE_TAG) else {
            return String::new();
        };
        visible = reasoning[end + CLOSE_TAG.len()..].trim_start();
    }
}

#[derive(Default)]
enum State {
    #[default]
    Start,
    Thinking,
    Visible,
}

/// Stateful equivalent of [`strip_thinking`] for streamed response chunks.
#[derive(Default)]
pub struct ThinkStripper {
    state: State,
    pending: String,
}

impl ThinkStripper {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, delta: &str) -> String {
        if matches!(self.state, State::Visible) {
            return delta.to_string();
        }
        self.pending.push_str(delta);

        loop {
            match self.state {
                State::Start => {
                    let trimmed = self.pending.trim_start();
                    if OPEN_TAG.starts_with(trimmed) {
                        return String::new();
                    }
                    if trimmed.starts_with(OPEN_TAG) {
                        let consumed = self.pending.len() - trimmed.len() + OPEN_TAG.len();
                        self.pending.drain(..consumed);
                        self.state = State::Thinking;
                        continue;
                    }
                    self.state = State::Visible;
                    return std::mem::take(&mut self.pending);
                }
                State::Thinking => {
                    if let Some(end) = self.pending.find(CLOSE_TAG) {
                        self.pending.drain(..end + CLOSE_TAG.len());
                        let trimmed = self.pending.trim_start();
                        let whitespace = self.pending.len() - trimmed.len();
                        self.pending.drain(..whitespace);
                        self.state = State::Start;
                        continue;
                    }

                    let keep = partial_tag_suffix_len(&self.pending, CLOSE_TAG);
                    let discard = self.pending.len() - keep;
                    self.pending.drain(..discard);
                    return String::new();
                }
                State::Visible => return std::mem::take(&mut self.pending),
            }
        }
    }

    /// Preserve unresolved visible text and drop an unterminated reasoning block.
    pub fn flush(&mut self) -> String {
        match self.state {
            State::Thinking => {
                self.pending.clear();
                String::new()
            }
            _ => std::mem::take(&mut self.pending),
        }
    }
}

fn partial_tag_suffix_len(input: &str, tag: &str) -> usize {
    input
        .char_indices()
        .map(|(index, _)| &input[index..])
        .find(|suffix| tag.starts_with(suffix))
        .map_or(0, str::len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_thinking_leaves_normal_and_inline_tags_unchanged() {
        assert_eq!(strip_thinking("Hello world."), "Hello world.");
        assert_eq!(
            strip_thinking("Example: <think>literal</think>"),
            "Example: <think>literal</think>"
        );
    }

    #[test]
    fn strip_thinking_removes_leading_blocks_and_whitespace() {
        assert_eq!(
            strip_thinking(" <think>first</think>\n<think>second</think>\n\nanswer"),
            "answer"
        );
    }

    #[test]
    fn strip_thinking_drops_unterminated_leading_block() {
        assert_eq!(strip_thinking("<think>never closes"), "");
    }

    #[test]
    fn stripper_passes_normal_chunks_through() {
        let mut stripper = ThinkStripper::new();
        assert_eq!(stripper.push("Hel"), "Hel");
        assert_eq!(stripper.push("lo"), "lo");
        assert_eq!(stripper.flush(), "");
    }

    #[test]
    fn stripper_handles_tags_split_across_chunks() {
        let mut stripper = ThinkStripper::new();
        assert_eq!(stripper.push("<th"), "");
        assert_eq!(stripper.push("ink>reasoning</th"), "");
        assert_eq!(stripper.push("ink>visible"), "visible");
        assert_eq!(stripper.flush(), "");
    }

    #[test]
    fn stripper_handles_multibyte_reasoning_without_panicking() {
        let mut stripper = ThinkStripper::new();
        assert_eq!(stripper.push("<think>😀123456789"), "");
        assert_eq!(stripper.push("</think>visible"), "visible");
    }

    #[test]
    fn stripper_flushes_an_incomplete_visible_tag() {
        let mut stripper = ThinkStripper::new();
        assert_eq!(stripper.push("<th"), "");
        assert_eq!(stripper.flush(), "<th");
    }
}
