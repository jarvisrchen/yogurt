//! `POST /api/chat`: chat across every meeting.
//!
//! Retrieves the top meetings for the latest question from the FTS5 index, then streams the answer back as NDJSON.
//! The first line is `{"sources":[{id,title,started_at}]}`, numbered from 1 in that order, which the answer cites as `[n]`.
//! Every later line is `{"delta":"..."}`. A failure arrives as a final delta, like per-meeting chat.
//! Nothing is persisted: the client holds the conversation and sends it back on each turn.

use std::convert::Infallible;

use axum::{
    body::{Body, Bytes},
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::{Datelike, Duration, Local, NaiveDate, TimeZone};
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::json;
use yogurt_db::RetrievalHit;
use yogurt_llm::{ChatMessage as LlmMessage, ChatRequest};

use crate::AppState;

const TOP_K: usize = 6;
/// Earlier turns sent back to the model. Retrieval only ever looks at the latest question.
const HISTORY_TURNS: usize = 6;

#[derive(Debug, Deserialize)]
pub struct LibraryChatBody {
    /// The whole conversation, oldest first, ending with the user's new question.
    pub messages: Vec<LlmMessage>,
}

pub async fn post_library_chat(
    State(state): State<AppState>,
    Json(body): Json<LibraryChatBody>,
) -> Response {
    let valid_roles = body
        .messages
        .iter()
        .all(|m| m.role == "user" || m.role == "assistant");
    let question = match body.messages.last() {
        Some(m) if valid_roles && m.role == "user" && !m.content.trim().is_empty() => {
            m.content.trim().to_string()
        }
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "messages must end with a non-empty user question" })),
            )
                .into_response()
        }
    };

    let today = Local::now().date_naive();
    let terms = extract_terms(&question);
    let range = date_range(&question, today).and_then(|(a, b)| Some((local_ms(a)?, local_ms(b)?)));
    let repo = state.meeting_repo.clone();
    let hits = match tokio::task::spawn_blocking(move || repo.retrieve(&terms, range, TOP_K)).await
    {
        Ok(Ok(h)) => h,
        Ok(Err(e)) => return server_error(format!("retrieval failed: {e}")),
        Err(e) => return server_error(format!("retrieval task failed: {e}")),
    };

    let system = match state.prompts.library_chat_system() {
        Ok(s) => s,
        Err(e) => return server_error(format!("prompt load failed: {e}")),
    };
    let messages = build_messages(&system, today, &hits, &body.messages);
    let sources: Vec<_> = hits
        .iter()
        .map(|h| json!({ "id": h.meeting.id, "title": h.meeting.title, "started_at": h.meeting.started_at }))
        .collect();

    let (tx, rx) = tokio::sync::mpsc::channel::<Bytes>(32);
    let _ = tx.send(ndjson(&json!({ "sources": sources }))).await;
    tokio::spawn(async move {
        let llm = match crate::llm_openai::resolve(&state).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "library chat: LLM resolution failed");
                let _ = tx.send(ndjson(&json!({ "delta": format!("[{e}]") }))).await;
                return;
            }
        };
        let mut stream = match llm
            .stream(ChatRequest {
                messages,
                stream: true,
            })
            .await
        {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "library chat: stream open failed");
                let _ = tx
                    .send(ndjson(&json!({ "delta": format!("[stream error: {e}]") })))
                    .await;
                return;
            }
        };
        while let Some(chunk) = stream.next().await {
            let (delta, done) = match chunk {
                Ok(c) => (c.delta, c.done),
                Err(e) => (format!("\n\n[stream error: {e}]"), true),
            };
            // A send error means the client went away, so stop generating.
            if !delta.is_empty() && tx.send(ndjson(&json!({ "delta": delta }))).await.is_err() {
                return;
            }
            if done {
                return;
            }
        }
    });

    let body = futures_util::stream::unfold(rx, |mut rx| async {
        rx.recv().await.map(|b| (Ok::<_, Infallible>(b), rx))
    });
    (
        [(header::CONTENT_TYPE, "application/x-ndjson")],
        Body::from_stream(body),
    )
        .into_response()
}

fn server_error(msg: String) -> Response {
    tracing::error!(error = %msg, "library chat");
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
        .into_response()
}

fn ndjson(v: &serde_json::Value) -> Bytes {
    let mut s = v.to_string();
    s.push('\n');
    Bytes::from(s)
}

fn build_messages(
    system: &str,
    today: NaiveDate,
    hits: &[RetrievalHit],
    conversation: &[LlmMessage],
) -> Vec<LlmMessage> {
    let mut context = String::from("MEETINGS:\n");
    if hits.is_empty() {
        context.push_str("\n(no meetings matched)\n");
    }
    for (i, h) in hits.iter().enumerate() {
        let m = &h.meeting;
        let date = Local
            .timestamp_millis_opt(m.started_at)
            .single()
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let notes = match m.enriched_md.as_deref() {
            Some(e) if !e.trim().is_empty() => yogurt_notes::ast::strip_markers(e),
            _ => m.notes_md.clone(),
        };
        context.push_str(&format!(
            "\n[{}] {} ({date})\nNotes:\n{}\n",
            i + 1,
            m.title,
            notes.trim()
        ));
        if !h.excerpt.trim().is_empty() {
            context.push_str(&format!("Transcript excerpt:\n{}\n", h.excerpt.trim()));
        }
    }
    let mut out = vec![
        LlmMessage::system(format!("{}\n\nToday is {today}.", system.trim())),
        LlmMessage::system(context),
    ];
    let start = conversation.len().saturating_sub(HISTORY_TURNS + 1);
    out.extend(conversation[start..].iter().cloned());
    out
}

const STOPWORDS: &[&str] = &[
    "a",
    "about",
    "after",
    "all",
    "also",
    "am",
    "an",
    "and",
    "any",
    "anything",
    "are",
    "as",
    "at",
    "be",
    "been",
    "before",
    "but",
    "by",
    "can",
    "could",
    "did",
    "do",
    "does",
    "for",
    "from",
    "get",
    "got",
    "had",
    "has",
    "have",
    "he",
    "her",
    "him",
    "his",
    "how",
    "i",
    "if",
    "in",
    "is",
    "it",
    "its",
    "me",
    "meeting",
    "meetings",
    "my",
    "of",
    "on",
    "or",
    "our",
    "say",
    "said",
    "she",
    "should",
    "so",
    "tell",
    "than",
    "that",
    "the",
    "their",
    "them",
    "then",
    "there",
    "these",
    "they",
    "this",
    "those",
    "to",
    "us",
    "was",
    "we",
    "were",
    "what",
    "when",
    "where",
    "which",
    "who",
    "why",
    "will",
    "with",
    "would",
    "you",
    "your",
    // Date words, which `date_range` reads instead.
    "today",
    "yesterday",
    "week",
    "month",
    "year",
    "last",
    "past",
    "ago",
];

const MONTHS: [&str; 12] = [
    "january",
    "february",
    "march",
    "april",
    "may",
    "june",
    "july",
    "august",
    "september",
    "october",
    "november",
    "december",
];

const VERB_MONTHS: [&str; 2] = ["may", "march"];
const MONTH_CUES: [&str; 11] = [
    "in", "during", "since", "from", "until", "last", "this", "early", "mid", "late", "of",
];

fn words(q: &str) -> Vec<String> {
    q.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The question's content words, lowercased and deduplicated, for an ORed FTS5 query.
fn extract_terms(q: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for w in words(q) {
        let skip = w.chars().count() < 2
            || STOPWORDS.contains(&w.as_str())
            || MONTHS.contains(&w.as_str())
            || out.contains(&w);
        if !skip {
            out.push(w);
        }
    }
    out
}

/// `[start, end)` local dates for the first date phrase the question names, if any.
/// A bare month name means its most recent occurrence, so "in December" asked in March is last December.
/// "May" and "March" are also verbs, so they name a month only next to a date cue: "in May", "last March", "May 12th", "May 2025".
fn date_range(q: &str, today: NaiveDate) -> Option<(NaiveDate, NaiveDate)> {
    let w = words(q);
    let has = |s: &str| w.iter().any(|x| x == s);
    let pair = |a: &str, b: &str| w.windows(2).any(|p| p[0] == a && p[1] == b);
    let month_start = |y: i32, m: u32| NaiveDate::from_ymd_opt(y, m, 1);
    let next_month = |d: NaiveDate| {
        if d.month() == 12 {
            month_start(d.year() + 1, 1)
        } else {
            month_start(d.year(), d.month() + 1)
        }
    };
    let this_month = month_start(today.year(), today.month())?;
    let monday = today - Duration::days(today.weekday().num_days_from_monday() as i64);

    if has("today") {
        return Some((today, today.succ_opt()?));
    }
    if has("yesterday") {
        return Some((today.pred_opt()?, today));
    }
    if pair("this", "week") {
        return Some((monday, monday + Duration::days(7)));
    }
    if pair("last", "week") || pair("past", "week") {
        return Some((monday - Duration::days(7), monday));
    }
    if pair("this", "month") {
        return Some((this_month, next_month(this_month)?));
    }
    if pair("last", "month") || pair("past", "month") {
        let prev = (this_month.pred_opt()?).with_day(1)?;
        return Some((prev, this_month));
    }
    if pair("this", "year") {
        return Some((
            month_start(today.year(), 1)?,
            month_start(today.year() + 1, 1)?,
        ));
    }
    if pair("last", "year") || pair("past", "year") {
        return Some((
            month_start(today.year() - 1, 1)?,
            month_start(today.year(), 1)?,
        ));
    }
    for (i, name) in MONTHS.iter().enumerate() {
        let m = i as u32 + 1;
        for pos in (0..w.len()).filter(|&p| w[p] == *name) {
            let next = w.get(pos + 1);
            let named_year = next.filter(|y| y.len() == 4).and_then(|y| y.parse().ok());
            let day_follows = next.is_some_and(|d| {
                d.trim_end_matches(char::is_alphabetic)
                    .parse::<u32>()
                    .is_ok_and(|n| (1..=31).contains(&n))
            });
            let cued = w[..pos]
                .last()
                .is_some_and(|p| MONTH_CUES.contains(&p.as_str()));
            if VERB_MONTHS.contains(name) && named_year.is_none() && !day_follows && !cued {
                continue;
            }
            let year = named_year.unwrap_or(if m <= today.month() {
                today.year()
            } else {
                today.year() - 1
            });
            let start = month_start(year, m)?;
            return Some((start, next_month(start)?));
        }
    }
    None
}

fn local_ms(d: NaiveDate) -> Option<i64> {
    Local
        .from_local_datetime(&d.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|t| t.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).unwrap()
    }

    #[test]
    fn terms_drop_stopwords_dates_and_duplicates() {
        assert_eq!(
            extract_terms("What did Steven and I decide about pricing last month? Pricing!"),
            ["steven", "decide", "pricing"]
        );
        assert!(extract_terms("what did we say in September?").is_empty());
    }

    #[test]
    fn date_phrases_resolve_against_today() {
        // 2026-10-21 is a Wednesday.
        let today = d(2026, 10, 21);
        assert_eq!(
            date_range("pricing last month", today),
            Some((d(2026, 9, 1), d(2026, 10, 1)))
        );
        assert_eq!(
            date_range("this week", today),
            Some((d(2026, 10, 19), d(2026, 10, 26)))
        );
        assert_eq!(
            date_range("last week", today),
            Some((d(2026, 10, 12), d(2026, 10, 19)))
        );
        assert_eq!(
            date_range("yesterday", today),
            Some((d(2026, 10, 20), today))
        );
        assert_eq!(
            date_range("in September", today),
            Some((d(2026, 9, 1), d(2026, 10, 1)))
        );
        assert_eq!(
            date_range("in december", today),
            Some((d(2025, 12, 1), d(2026, 1, 1)))
        );
        assert_eq!(
            date_range("december 2026", today),
            Some((d(2026, 12, 1), d(2027, 1, 1)))
        );
        assert_eq!(
            date_range("last month", d(2026, 1, 5)),
            Some((d(2025, 12, 1), d(2026, 1, 1)))
        );
        assert_eq!(date_range("what did steven say", today), None);
        assert_eq!(date_range("did steven say we may slip?", today), None);
        assert_eq!(date_range("should we march ahead?", today), None);
        assert_eq!(date_range("may?", today), None);
        assert_eq!(
            date_range("did we say we may ship in may?", today),
            Some((d(2026, 5, 1), d(2026, 6, 1)))
        );
        for q in ["since may", "last march", "may 12th offsite", "early may"] {
            assert!(date_range(q, today).is_some(), "{q}");
        }
        assert_eq!(
            date_range("the march 3 review", today),
            Some((d(2026, 3, 1), d(2026, 4, 1)))
        );
        assert_eq!(
            date_range("what happened in may", today),
            Some((d(2026, 5, 1), d(2026, 6, 1)))
        );
        assert_eq!(
            date_range("may 2025", today),
            Some((d(2025, 5, 1), d(2025, 6, 1)))
        );
    }
}
