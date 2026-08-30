//! Inline weaving: locate the user's own note lines inside an AI block so
//! the render can paint just those words ink and leave the model's
//! phrasing grey. Without this, a note like "25% on monday" and the
//! model's "Rollout: 25% on Monday the 5th, then 50% the following week"
//! sit side by side as two bullets; with it they are one bullet, the
//! user's words black inside the model's sentence.

/// Fewest words a note line needs before we go looking for it inside AI
/// text - a one-word note matches everywhere and means nothing.
const MIN_WORDS: usize = 2;
/// Fraction of the line's words that must be found, in order.
const MIN_RATIO: f32 = 0.75;

/// Byte range of `text` (already marker-stripped) that carries `line`, if
/// enough of the line's words appear in order within a window no longer
/// than twice the line. The range runs from the first matched word to the
/// last, so model words interleaved with the user's ("ask Jordan *today*
/// for *updated* cost numbers") are painted ink too - the sentence should
/// read as one thing.
pub fn find_user_line(text: &str, line: &str) -> Option<(usize, usize)> {
    let want: Vec<String> = words(line).into_iter().map(|(_, _, w)| w).collect();
    if want.len() < MIN_WORDS {
        return None;
    }
    let have = words(text);
    let max_span = want.len() * 2 + 1;
    let mut best: Option<(usize, usize, usize)> = None; // (matched, start_idx, end_idx)
    for (i, tok) in have.iter().enumerate() {
        if tok.2 != want[0] {
            continue;
        }
        let limit = (i + max_span).min(have.len());
        let mut matched = 1;
        let mut last = i;
        let mut j = i + 1;
        for w in &want[1..] {
            if let Some(k) = (j..limit).find(|&k| have[k].2 == *w) {
                matched += 1;
                last = k;
                j = k + 1;
            }
        }
        let better = match best {
            None => true,
            Some((m, s, e)) => matched > m || (matched == m && last - i < e - s),
        };
        if better {
            best = Some((matched, i, last));
        }
    }
    let (matched, s, e) = best?;
    if matched < MIN_WORDS || (matched as f32) < (want.len() as f32) * MIN_RATIO {
        return None;
    }
    Some((have[s].0, have[e].1))
}

/// Sort and merge overlapping ranges so the render can walk them once.
pub fn merge_ranges(mut runs: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    runs.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(runs.len());
    for (a, b) in runs {
        match out.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

/// Words with their byte spans in `s`, lowercased with edge punctuation
/// trimmed ("Jordan," -> "jordan", "25%," -> "25%").
fn words(s: &str) -> Vec<(usize, usize, String)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            if let Some(st) = start.take() {
                push_word(s, st, i, &mut out);
            }
        } else if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(st) = start {
        push_word(s, st, s.len(), &mut out);
    }
    out
}

fn push_word(s: &str, st: usize, end: usize, out: &mut Vec<(usize, usize, String)>) {
    let raw = &s[st..end];
    let keep = |c: char| c.is_alphanumeric() || c == '%' || c == '$';
    let lead = raw.len() - raw.trim_start_matches(|c| !keep(c)).len();
    let trail = raw.len() - raw.trim_end_matches(|c| !keep(c)).len();
    if lead + trail >= raw.len() {
        return;
    }
    let (a, b) = (st + lead, end - trail);
    out.push((a, b, s[a..b].to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_words_inside_a_longer_bullet() {
        let t = "Rollout: 25% on Monday the 5th, then 50% the following week";
        let r = find_user_line(t, "25% on monday").unwrap();
        assert_eq!(&t[r.0..r.1], "25% on Monday");
    }

    #[test]
    fn interleaved_model_words_are_swallowed_into_the_run() {
        let t =
            "Dan: ask Jordan today for updated cost numbers (Finance wants a per-user estimate)";
        let r = find_user_line(t, "ask jordan for cost numbers").unwrap();
        assert_eq!(&t[r.0..r.1], "ask Jordan today for updated cost numbers");
    }

    #[test]
    fn too_few_words_matched_is_no_match() {
        assert_eq!(
            find_user_line(
                "Sam: ping Priya about the language eval",
                "ask jordan for cost numbers"
            ),
            None
        );
        // one-word notes never match
        assert_eq!(find_user_line("kill switch before Monday", "switch"), None);
    }

    #[test]
    fn merge_ranges_collapses_overlaps() {
        assert_eq!(
            merge_ranges(vec![(10, 20), (0, 5), (15, 30)]),
            vec![(0, 5), (10, 30)]
        );
    }
}
