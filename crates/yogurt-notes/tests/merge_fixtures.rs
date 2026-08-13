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

/// HI-3: LLMs reliably flatten nested user bullets (depth-1 sub-bullets
/// re-emitted as depth-0 top-level items). The diff must recognize the
/// LLM's flattened bullets as the SAME content as the user's nested
/// bullets and preserve the user's original depth — otherwise outline-
/// style notes silently lose their structure on enhance.
#[test]
fn it_preserves_user_depth_when_llm_flattens_nested_list() {
    run("06_nested_list_flattened");
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

/// Regression: weaker LLMs (e.g. Minimax) reproduce the `<span data-ai-grey>`
/// scaffolding from the enhance prompt literally in the bullet body. We own
/// the wrapping, so the render layer must STRIP those model-emitted wire-format
/// spans before escaping — otherwise the raw markup is HTML-escaped
/// (`&lt;span…&gt;`) and double-wrapped, surfacing literal span tags in the
/// user's notes. Found via E2E enhance against Minimax (2026-08-13).
#[test]
fn it_strips_model_emitted_spans_instead_of_escaping_them() {
    let transcript = std::fs::read_to_string(
        std::path::Path::new("tests/fixtures/01_pure_new_ai").join("transcript.json"),
    )
    .unwrap();
    // The LLM echoed the prompt's span format into its own output.
    let enriched = concat!(
        "## Discussion\n\n",
        "- <span data-ai-grey data-ts=\"120\">Pricing model debated",
        "<span data-transcript-link data-ts=\"120\">↳ 02:00</span></span>\n"
    );
    let md = yogurt_notes::render::to_markdown(
        &yogurt_notes::merge_notes("", enriched, &transcript).unwrap(),
    );
    assert!(
        !md.contains("&lt;span"),
        "model-emitted span markup must be stripped, not escaped; got:\n{md}"
    );
    assert!(
        md.contains(r#"- <span data-ai-grey data-ts="120">Pricing model debated <span data-transcript-link data-ts="120">↳ 02:00</span></span>"#),
        "bullet must be wrapped exactly once with clean inner text; got:\n{md}"
    );
}
