//! `parse_deepgram_event` against real-world-shaped Deepgram frames from `tests/fixtures/deepgram_real_output.json`.
//!
//! `is_final` fires every ~6 s mid-utterance while `speech_final` fires only at end-of-utterance.
//! The parser must map partial, interim-final and metadata or empty frames to non-final or `None`, and only speech-final to final.

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

    fn parse_one(frame: &serde_json::Value) -> Option<yogurt_stt::TranscriptEvent> {
        let s = serde_json::to_string(frame).unwrap();
        parse_deepgram_event(&s, Channel::Mic)
    }

    let ev = parse_one(&frames[0]).expect("partial parses");
    assert_eq!(ev.text, "hello wor");
    assert!(!ev.is_final, "partial must not be final");

    // Interim final: `is_final=true, speech_final=false` must not lock the line.
    let ev = parse_one(&frames[1]).expect("interim final parses");
    assert_eq!(ev.text, "hello world how are");
    assert!(
        !ev.is_final,
        "interim final (speech_final=false) MUST NOT lock the line — that's BL-03"
    );

    let ev = parse_one(&frames[2]).expect("speech final parses");
    assert_eq!(ev.text, "hello world how are you doing today");
    assert!(ev.is_final, "speech_final=true MUST lock the line");

    assert!(
        parse_one(&frames[3]).is_none(),
        "metadata frame must yield None"
    );

    assert!(
        parse_one(&frames[4]).is_none(),
        "empty transcript must yield None"
    );
}
