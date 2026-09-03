//! Structural block-level diff/merge between user_md and enriched_md.
//!
//! Algorithm:
//!   1. Build a HashMap of `block_key(b) -> &Block` for every user block.
//!   2. Walk enriched_blocks in order:
//!        - If the key is in user_set, emit `Source::User` using the user's
//!          exact block text (preserves trailing whitespace / formatting).
//!        - Otherwise emit `Source::AiGrey { transcript_ts_sec }` with a
//!          best-effort transcript ts from `ts::guess_ts_sec`.
//!   3. Inline pass: for every AiGrey paragraph / list item, find each user
//!      line woven into it (`weave::find_user_line`) and record the byte
//!      ranges as `user_runs`; a line found this way counts as kept.
//!   4. Append any user blocks the LLM dropped at the end as `Source::User`
//!      (defensive — never lose user text).

use crate::ast::{block_key, strip_markers, Block};
use crate::{ts, weave, MergedBlock, Source, TranscriptSegment};

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
            // The user's exact text always wins. The user's block SHAPE wins
            // too (HI-3: keep their nesting depth when the model flattens),
            // with one exception: a bare line the user typed that the model
            // filed as a bullet stays a bullet, so their note sits in the
            // list with the AI additions instead of floating beside it as
            // a lone paragraph.
            let block = match (*user_block, b) {
                (Block::Paragraph { md }, Block::ListItem { depth, .. }) => Block::ListItem {
                    md: md.clone(),
                    depth: *depth,
                },
                _ => (*user_block).clone(),
            };
            out.push(MergedBlock {
                block,
                source: Source::User,
                user_runs: Vec::new(),
            });
        } else {
            let body_text = block_md_text(b);
            let ts_sec = ts::guess_ts_sec(&body_text, transcript).unwrap_or(0);
            out.push(MergedBlock {
                block: b.clone(),
                source: Source::AiGrey {
                    transcript_ts_sec: ts_sec,
                },
                user_runs: Vec::new(),
            });
        }
    }

    // Inline pass: the user's lines woven into AI bullets.
    let user_lines: Vec<(String, String)> = user
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph { md } | Block::ListItem { md, .. } => {
                Some((block_key(b), strip_markers(md)))
            }
            _ => None,
        })
        .collect();
    let mut absorbed: HashSet<String> = HashSet::new();
    for mb in out.iter_mut() {
        if !matches!(mb.source, Source::AiGrey { .. }) {
            continue;
        }
        let md = match &mb.block {
            Block::Paragraph { md } | Block::ListItem { md, .. } => md,
            _ => continue,
        };
        let text = strip_markers(md);
        let mut runs = Vec::new();
        for (key, line) in &user_lines {
            if let Some(r) = weave::find_user_line(&text, line) {
                runs.push(r);
                absorbed.insert(key.clone());
            }
        }
        mb.user_runs = weave::merge_ranges(runs);
    }

    // Defensive: append any user blocks the LLM dropped - unless their words
    // live on inside an AI bullet, which is the whole point of weaving.
    for b in user {
        let k = block_key(b);
        if !seen_user_keys.contains(&k) && !absorbed.contains(&k) {
            out.push(MergedBlock {
                block: b.clone(),
                source: Source::User,
                user_runs: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_paragraph_the_model_bulletized_stays_a_bullet_with_user_text() {
        let user = vec![Block::Paragraph {
            md: "5% users on Tuesday".into(),
        }];
        let enriched = vec![
            Block::ListItem {
                md: "5% users on Tuesday".into(),
                depth: 0,
            },
            Block::ListItem {
                md: "Error rate at 0.3%".into(),
                depth: 0,
            },
        ];
        let out = merge(&user, &enriched, &[]);
        assert_eq!(out.len(), 2, "no duplicate appended: {out:?}");
        assert!(out[0].user_runs.is_empty());
        assert_eq!(
            out[0].block,
            Block::ListItem {
                md: "5% users on Tuesday".into(),
                depth: 0
            }
        );
        assert_eq!(out[0].source, Source::User);
        assert!(matches!(out[1].source, Source::AiGrey { .. }));
    }

    /// LLM-8 (meeting 01a05a46): notes `247k` / `464k RSU`, model output
    /// `- Base salary: 247k`. The one-word line used to come back grey
    /// inside the bullet and then AGAIN as a bare paragraph at the end.
    #[test]
    fn one_word_note_folded_into_an_ai_bullet_is_painted_not_appended() {
        let user = vec![
            Block::Paragraph { md: "247k".into() },
            Block::Paragraph {
                md: "464k RSU".into(),
            },
        ];
        let enriched = vec![
            Block::ListItem {
                md: "Base salary: 247k".into(),
                depth: 0,
            },
            Block::ListItem {
                md: "464k RSU with quarterly vesting at 6.25%".into(),
                depth: 0,
            },
        ];
        let out = merge(&user, &enriched, &[]);
        assert_eq!(out.len(), 2, "no orphan append: {out:?}");
        assert_eq!(out[0].user_runs, vec![(13, 17)]);
        assert_eq!(out[1].user_runs, vec![(0, 8)]);
    }

    #[test]
    fn user_line_woven_into_an_ai_bullet_is_painted_not_appended() {
        let user = vec![
            Block::Paragraph {
                md: "25% on monday".into(),
            },
            Block::Paragraph {
                md: "totally unrelated line".into(),
            },
        ];
        let enriched = vec![Block::ListItem {
            md: "Rollout: 25% on Monday the 5th, then 50% the following week".into(),
            depth: 0,
        }];
        let out = merge(&user, &enriched, &[]);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(out[0].source, Source::AiGrey { .. }));
        assert_eq!(out[0].user_runs, vec![(9, 22)]);
        // the unrelated line still gets the defensive append
        assert_eq!(out[1].source, Source::User);
        assert_eq!(
            out[1].block,
            Block::Paragraph {
                md: "totally unrelated line".into()
            }
        );
    }
}
