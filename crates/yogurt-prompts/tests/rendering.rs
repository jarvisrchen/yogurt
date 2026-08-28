//! Rendering tests for the yogurt-prompts crate.
//!
//! All three tests pin the contract for the hero enhance flow + chat system:
//! 1. `{notes}` + `{transcript}` substitute into `enhance.md`.
//! 2. `chat-system.md` is served verbatim with no templating.
//! 3. HTML special characters in user notes are NOT escaped (D-16).

use yogurt_prompts::{EnhanceCtx, Mode, Prompts};

#[test]
fn it_renders_enhance_with_notes_and_transcript() {
    let p = Prompts::load(Mode::Release).expect("load");
    let out = p
        .render_enhance(&EnhanceCtx {
            notes: "- pricing\n- timeline\n",
            transcript: r#"[{"ts_ms":120000,"channel":"mic","text":"We agreed on $14/mo"}]"#,
        })
        .expect("render");
    assert!(out.contains("- pricing"), "notes substituted: {out}");
    assert!(out.contains("$14/mo"), "transcript substituted: {out}");
    assert!(
        out.contains("<user_notes>") && out.contains("<transcript>"),
        "prompt scaffolding present: {out}"
    );
}

#[test]
fn it_serves_chat_system_unmodified() {
    let p = Prompts::load(Mode::Release).expect("load");
    let s = p.chat_system().expect("read");
    assert!(
        s.contains("watching a meeting"),
        "chat-system prompt loaded: {s}"
    );
}

#[test]
fn it_does_not_html_escape_special_chars_in_notes() {
    let p = Prompts::load(Mode::Release).expect("load");
    let out = p
        .render_enhance(&EnhanceCtx {
            notes: "use <emphasis> & friends",
            transcript: "[]",
        })
        .unwrap();
    assert!(
        out.contains("<emphasis>"),
        "must not escape — see set_default_formatter: {out}"
    );
    assert!(out.contains("& friends"), "must not escape &: {out}");
}
