//! The enhance steps the Cloud webapp runs in the browser, each a thin call into the crates the Mac's `/enhance` route uses.

use wasm_bindgen::prelude::*;
use yogurt_prompts::{thinking::strip_thinking, EnhanceCtx, Mode, Prompts};

fn forced(template: Option<String>) -> Result<Option<&'static str>, JsError> {
    yogurt_prompts::template_choice(template.as_deref()).map_err(|e| JsError::new(&e.to_string()))
}

/// The chat messages the Mac sends for this enhance, as JSON. `template` is a format id, or none or `auto` to let the model pick.
#[wasm_bindgen]
pub fn render_enhance(
    notes_md: &str,
    transcript_json: &str,
    template: Option<String>,
) -> Result<String, JsError> {
    let template = forced(template)?;
    let prompt = Prompts::load(Mode::Release)
        .and_then(|p| {
            p.render_enhance(&EnhanceCtx {
                notes: notes_md,
                transcript: transcript_json,
                template,
            })
        })
        .map_err(|e| JsError::new(&e.to_string()))?;
    // The Mac sends one user message and no system message.
    Ok(serde_json::json!([{ "role": "user", "content": prompt }]).to_string())
}

/// Whether the meeting is an accidental start/stop, which the Mac does not enhance.
#[wasm_bindgen]
pub fn too_short(notes_md: &str, transcript_json: &str) -> bool {
    yogurt_notes::enhance::too_short(notes_md, transcript_json)
}

/// The document so far, for a live preview while the reply streams.
#[wasm_bindgen]
pub fn preview(llm_output: &str) -> String {
    yogurt_prompts::split_template_line(&strip_thinking(llm_output))
        .1
        .to_string()
}

#[wasm_bindgen(getter_with_clone)]
pub struct Enhanced {
    pub enriched_md: String,
    pub template: String,
}

/// The stored document for the model's whole reply, as the Mac's `/enhance` builds it.
#[wasm_bindgen]
pub fn finish_enhance(
    notes_md: &str,
    transcript_json: &str,
    llm_output: &str,
    template: Option<String>,
) -> Result<Enhanced, JsError> {
    // The Mac's LLM client strips leading <think> blocks as the reply streams, before the route sees it.
    let streamed = strip_thinking(llm_output);
    let done =
        yogurt_notes::enhance::finish(notes_md, transcript_json, &streamed, forced(template)?)
            .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(Enhanced {
        enriched_md: done.enriched_md,
        template: done.template.to_string(),
    })
}
