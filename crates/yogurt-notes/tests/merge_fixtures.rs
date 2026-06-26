//! Fixture-driven tests for `yogurt_notes::merge_notes`.
//!
//! Each fixture directory under `tests/fixtures/` contains exactly four files:
//!   - `notes.md` — the user's raw markdown
//!   - `transcript.json` — the transcript segment array
//!   - `enriched.md` — the LLM's output
//!   - `expected.json` — the expected `MergedDoc` JSON
//!
//! Five scenarios (per CONTEXT D-09 + PRD §5.3) lock the contract:
//!   01: pure new AI (empty notes, transcript-attributed AI bullets)
//!   02: AI bullets under a user heading
//!   03: AI bullet inserted between user bullets
//!   04: promote-grey-on-edit (re-enhance shorter, user's edit wins)
//!   05: re-enhance preserves promoted-black + still adds new AI bullets

use std::path::Path;

#[test]
fn it_merges_pure_new_ai() {
    run("01_pure_new_ai");
}

#[test]
fn it_merges_ai_under_user_heading() {
    run("02_ai_under_user_heading");
}

#[test]
fn it_merges_ai_bullet_next_to_user() {
    run("03_ai_bullet_next_to_user");
}

#[test]
fn it_preserves_promoted_grey_on_reenhance_short() {
    run("04_promote_grey_on_edit");
}

#[test]
fn it_preserves_promoted_grey_on_reenhance_long() {
    run("05_reenhance_preserves_promoted");
}

fn run(name: &str) {
    let dir = Path::new("tests/fixtures").join(name);
    let notes = std::fs::read_to_string(dir.join("notes.md")).unwrap();
    let transcript = std::fs::read_to_string(dir.join("transcript.json")).unwrap();
    let enriched = std::fs::read_to_string(dir.join("enriched.md")).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("expected.json")).unwrap()).unwrap();

    let got = yogurt_notes::merge_notes(&notes, &enriched, &transcript).expect("merge");
    let got_json = serde_json::to_value(&got).unwrap();

    if got_json != expected {
        // Pretty-diff on failure.
        let pretty_got = serde_json::to_string_pretty(&got_json).unwrap();
        let pretty_exp = serde_json::to_string_pretty(&expected).unwrap();
        panic!(
            "merge mismatch in {name}:\n--- expected ---\n{pretty_exp}\n--- got ---\n{pretty_got}"
        );
    }
}

#[test]
fn it_renders_merged_doc_to_wire_markdown_with_spans() {
    let dir = std::path::Path::new("tests/fixtures/01_pure_new_ai");
    let notes = std::fs::read_to_string(dir.join("notes.md")).unwrap();
    let transcript = std::fs::read_to_string(dir.join("transcript.json")).unwrap();
    let enriched = std::fs::read_to_string(dir.join("enriched.md")).unwrap();
    let doc = yogurt_notes::merge_notes(&notes, &enriched, &transcript).unwrap();
    let md = yogurt_notes::render::to_markdown(&doc);
    assert!(
        md.contains(r#"data-ai-grey data-ts="120""#),
        "first AI bullet tagged, got:\n{md}"
    );
    assert!(
        md.contains(r#"data-ai-grey data-ts="240""#),
        "second AI bullet tagged, got:\n{md}"
    );
    assert!(
        md.contains(r#"data-transcript-link data-ts="240">↳ 04:00</span>"#),
        "deep-link suffix present, got:\n{md}"
    );
}
