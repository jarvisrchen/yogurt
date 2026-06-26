//! HTML sanitization for the enrich pipeline (BL-2 of Phase 4 review).
//!
//! The enrich handler ships `enriched_md` to the browser, which renders it
//! through `markdown-it` with `html: true` so our wire-format spans survive.
//! That makes ANY HTML in the markdown a potential XSS vector: an LLM
//! hallucinating `<script>` or `<img src=x onerror=alert(1)>`, a transcript
//! line with raw angle brackets, or a malicious user pasting markup into
//! their notes would all execute in the user's browser.
//!
//! We close the hole with a TWO-layer defense:
//!
//!   1. `yogurt-notes::render::wrap_ai` HTML-escapes the inner text of AI
//!      bullets BEFORE they leave the server.
//!   2. This module re-runs `ammonia::Builder` over the FINAL enriched_md to
//!      strip anything that slipped through (e.g. raw HTML from a
//!      pre-enhance user note, an LLM that emitted an `<img>` tag).
//!
//! The allowlist is intentionally tiny: only the wire-format spans + the
//! transcript-link span (with their `data-*` attributes), nothing else.
//! All other tags (script, img, iframe, style, object, embed, form, input)
//! are removed; all event-handler attributes (onclick, onerror, onload) are
//! removed; `javascript:` URLs in any surviving attribute are dropped.
//!
//! NOTE: ammonia works on HTML, not markdown. We render to HTML via
//! markdown-it on the browser, so the server-side sanitization must run on
//! the enriched markdown's RAW HTML content (the wire-format spans + any
//! adversarial HTML the LLM injected). Plain markdown syntax (`-`, `#`,
//! `**bold**`) passes through `clean` unchanged because ammonia only acts
//! on tags, not on text.

use ammonia::Builder;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Strip all HTML in the input EXCEPT the wire-format allowlist:
///   * `<span data-ai-grey data-ts="N">…</span>`
///   * `<span data-transcript-link data-ts="N">…</span>`
///
/// Everything else (script, img, iframe, on* attrs, javascript: URLs) is
/// removed. Markdown syntax (`-`, `#`, etc.) is untouched.
pub fn sanitize_enriched_md(input: &str) -> String {
    static BUILDER: OnceLock<Builder<'static>> = OnceLock::new();
    let builder = BUILDER.get_or_init(|| {
        let mut b = Builder::default();

        // Tags: only <span>. Drop <script>, <img>, <iframe>, <a>, <style>, etc.
        let mut tags: HashSet<&str> = HashSet::new();
        tags.insert("span");
        b.tags(tags);

        // Attributes: <span> may carry only our wire-format data-* attrs.
        let mut tag_attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut span_attrs: HashSet<&str> = HashSet::new();
        span_attrs.insert("data-ai-grey");
        span_attrs.insert("data-transcript-link");
        span_attrs.insert("data-ts");
        tag_attrs.insert("span", span_attrs);
        b.tag_attributes(tag_attrs);

        // No URL schemes allowed for our allowlist tags (spans don't carry
        // href/src), and ammonia drops javascript: URLs by default.
        b.url_schemes(HashSet::new());
        b
    });
    builder.clean(input).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_preserves_wire_format_spans() {
        let input = r#"- <span data-ai-grey data-ts="120">summary <span data-transcript-link data-ts="120">↳ 02:00</span></span>"#;
        let out = sanitize_enriched_md(input);
        assert!(
            out.contains(r#"data-ai-grey"#),
            "data-ai-grey survived: {out}"
        );
        assert!(
            out.contains(r#"data-transcript-link"#),
            "data-transcript-link survived: {out}"
        );
        assert!(out.contains(r#"data-ts="120""#), "data-ts survived: {out}");
    }

    #[test]
    fn it_strips_script_tags() {
        let input = r#"- note <script>alert(1)</script>"#;
        let out = sanitize_enriched_md(input);
        assert!(!out.contains("<script"), "script removed: {out}");
        assert!(!out.contains("alert(1)"), "script body removed: {out}");
    }

    #[test]
    fn it_strips_img_onerror() {
        let input = r#"- note <img src=x onerror="alert(1)">"#;
        let out = sanitize_enriched_md(input);
        assert!(!out.contains("<img"), "img removed: {out}");
        assert!(!out.contains("onerror"), "onerror removed: {out}");
    }

    #[test]
    fn it_strips_iframe() {
        let input = r#"<iframe srcdoc="<script>alert(1)</script>"></iframe>"#;
        let out = sanitize_enriched_md(input);
        assert!(!out.contains("<iframe"), "iframe removed: {out}");
        assert!(!out.contains("srcdoc"), "srcdoc removed: {out}");
    }

    #[test]
    fn it_preserves_markdown_syntax() {
        let input = "- bullet\n# heading\n**bold** text";
        let out = sanitize_enriched_md(input);
        assert!(out.contains("- bullet"), "bullet syntax preserved: {out}");
        assert!(out.contains("# heading"), "heading syntax preserved: {out}");
        assert!(out.contains("**bold**"), "bold syntax preserved: {out}");
    }

    #[test]
    fn it_strips_event_handlers_from_spans() {
        let input = r#"<span data-ai-grey data-ts="0" onclick="alert(1)">x</span>"#;
        let out = sanitize_enriched_md(input);
        assert!(!out.contains("onclick"), "onclick removed: {out}");
        assert!(
            out.contains(r#"data-ai-grey"#),
            "data-ai-grey preserved: {out}"
        );
    }

    #[test]
    fn it_strips_javascript_url_links() {
        // <a> is not in the allowlist, so it's removed entirely — but
        // verify a span doesn't sneak href= through either.
        let input = r#"<a href="javascript:alert(1)">click</a>"#;
        let out = sanitize_enriched_md(input);
        assert!(!out.contains("<a"), "anchor removed: {out}");
        assert!(!out.contains("javascript:"), "js url removed: {out}");
    }
}
