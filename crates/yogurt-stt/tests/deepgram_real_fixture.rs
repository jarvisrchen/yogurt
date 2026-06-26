//! BL-03 regression: validate `parse_deepgram_event` against real-world-shaped
//! Deepgram frames sourced from `tests/fixtures/deepgram_real_output.json`.
//!
//! The fixture is hand-constructed from Deepgram's published streaming API
//! schema (https://developers.deepgram.com/reference/listen-live). Each frame
//! carries both `is_final` (interim-final, fires every ~6s mid-utterance) and
//! `speech_final` (true at end-of-utterance only). The fixture covers all
//! four shapes the parser must distinguish:
//!   - partial (both false) → ev.is_final = false
//!   - interim final (is_final=true, speech_final=false) → ev.is_final = false
//!   - speech final (both true) → ev.is_final = true
//!   - metadata / empty → None

use yogurt_stt::deepgram::parse_deepgram_event;
use yogurt_stt::Channel;

const FIXTURE: &str = include_str!("fixtures/deepgram_real_output.json");

#[test]
fn it_classifies_each_real_world_frame_correctly() {
    let v: serde_json::Value = serde_json::from_str(FIXTURE).expect("fixture parses");
    let frames = v
        .get("frames")
        .and_then(|f| f.as_array())
        .expect("fixture has frames array");

    // Helper: render one frame as a JSON string and run the parser.
    fn parse_one(frame: &serde_json::Value) -> Option<yogurt_stt::TranscriptEvent> {
        let s = serde_json::to_string(frame).unwrap();
        parse_deepgram_event(&s, Channel::Mic)
    }

    // 0: partial
    let ev = parse_one(&frames[0]).expect("partial parses");
    assert_eq!(ev.text, "hello wor");
    assert!(!ev.is_final, "partial must not be final");

    // 1: interim final (is_final=true, speech_final=false) → ev.is_final = false.
    // This is the BL-03 case: the OLD parser returned is_final=true here,
    // which caused the dock to lock a mid-utterance line.
    let ev = parse_one(&frames[1]).expect("interim final parses");
    assert_eq!(ev.text, "hello world how are");
    assert!(
        !ev.is_final,
        "interim final (speech_final=false) MUST NOT lock the line — that's BL-03"
    );

    // 2: speech final → ev.is_final = true. End-of-utterance, dock locks.
    let ev = parse_one(&frames[2]).expect("speech final parses");
    assert_eq!(ev.text, "hello world how are you doing today");
    assert!(ev.is_final, "speech_final=true MUST lock the line");

    // 3: metadata → None.
    assert!(
        parse_one(&frames[3]).is_none(),
        "metadata frame must yield None"
    );

    // 4: empty transcript → None.
    assert!(
        parse_one(&frames[4]).is_none(),
        "empty transcript must yield None"
    );
}
