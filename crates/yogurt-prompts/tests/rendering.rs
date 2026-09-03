//! Rendering tests for the yogurt-prompts crate.
//!
//! These pin the contract for the hero enhance flow + chat system:
//! 1. `{notes}` + `{transcript}` substitute into `enhance.md`.
//! 2. `chat-system.md` is served verbatim with no templating.
//! 3. HTML special characters in user notes are NOT escaped (D-16).
//! 4. Every id in `TEMPLATE_IDS` loads, and the `{format}` block lists
//!    them all when auto-detecting or just the one that was forced.

use yogurt_prompts::{EnhanceCtx, Mode, Prompts, TEMPLATE_IDS};

#[test]
fn it_renders_enhance_with_notes_and_transcript() {
    let p = Prompts::load(Mode::Release).expect("load");
    let out = p
        .render_enhance(&EnhanceCtx {
            notes: "- pricing\n- timeline\n",
            transcript: r#"[{"ts_ms":120000,"channel":"mic","text":"We agreed on $14/mo"}]"#,
            template: None,
        })
        .expect("render");
    assert!(out.contains("- pricing"), "notes substituted: {out}");
    assert!(out.contains("$14/mo"), "transcript substituted: {out}");
    assert!(
        out.contains("<user_notes>") && out.contains("<transcript>"),
        "prompt scaffolding present: {out}"
    );
    assert!(
        !out.contains("{format}"),
        "format placeholder rendered: {out}"
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
            template: None,
        })
        .unwrap();
    assert!(
        out.contains("<emphasis>"),
        "must not escape — see set_default_formatter: {out}"
    );
    assert!(out.contains("& friends"), "must not escape &: {out}");
}

#[test]
fn every_template_loads_in_both_modes() {
    for mode in [Mode::Release, Mode::Dev] {
        let p = Prompts::load(mode).expect("load");
        let all = p.templates().expect("templates");
        assert_eq!(all.len(), TEMPLATE_IDS.len());
        for (t, id) in all.iter().zip(TEMPLATE_IDS) {
            assert_eq!(t.id, id);
            assert!(!t.name.is_empty() && !t.when.is_empty() && !t.body.is_empty());
        }
        assert_eq!(
            all[0].id, "general",
            "general is the fallback and listed first"
        );
        assert!(p.template("nope").unwrap().is_none());
    }
}

#[test]
fn auto_mode_lists_every_format_and_forced_mode_only_one() {
    let p = Prompts::load(Mode::Release).expect("load");
    let auto = p
        .render_enhance(&EnhanceCtx {
            notes: "",
            transcript: "[]",
            template: None,
        })
        .unwrap();
    for id in TEMPLATE_IDS {
        assert!(
            auto.contains(&format!("### {id}\n")),
            "auto lists {id}: {auto}"
        );
    }
    let forced = p
        .render_enhance(&EnhanceCtx {
            notes: "",
            transcript: "[]",
            template: Some("standup"),
        })
        .unwrap();
    assert!(forced.contains("Note format: Standup"), "{forced}");
    assert!(forced.contains("## Blockers"), "{forced}");
    assert!(
        !forced.contains("### general"),
        "forced mode names one format: {forced}"
    );
    assert!(
        !forced.contains("## Proposal"),
        "no other format's sections: {forced}"
    );
    assert!(p
        .render_enhance(&EnhanceCtx {
            notes: "",
            transcript: "[]",
            template: Some("nope"),
        })
        .is_err());
}
