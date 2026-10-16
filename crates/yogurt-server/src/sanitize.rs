//! HTML sanitization for the enrich pipeline.
//!
//! The browser renders `enriched_md` through `markdown-it` with `html: true`, so any HTML an
//! LLM, transcript or user note injects would execute. `wrap_ai` in `yogurt-notes` escapes AI
//! bullet text first, and this module re-runs `ammonia` over the final markdown as the second layer.
//!
//! The allowlist is only the wire-format spans and their `data-*` attributes. Markdown syntax
//! passes through because ammonia acts on tags, not text.

use ammonia::Builder;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

/// Strips all HTML except `<span data-ai-grey data-ts>` and `<span data-transcript-link data-ts>`.
pub fn sanitize_enriched_md(input: &str) -> String {
    static BUILDER: OnceLock<Builder<'static>> = OnceLock::new();
    let builder = BUILDER.get_or_init(|| {
        let mut b = Builder::default();

        let mut tags: HashSet<&str> = HashSet::new();
        tags.insert("span");
        b.tags(tags);

        let mut tag_attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
        let mut span_attrs: HashSet<&str> = HashSet::new();
        span_attrs.insert("data-ai-grey");
        span_attrs.insert("data-transcript-link");
        span_attrs.insert("data-ts");
        tag_attrs.insert("span", span_attrs);
        b.tag_attributes(tag_attrs);

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
        let input = r#"<a href="javascript:alert(1)">click</a>"#;
        let out = sanitize_enriched_md(input);
        assert!(!out.contains("<a"), "anchor removed: {out}");
        assert!(!out.contains("javascript:"), "js url removed: {out}");
    }
}
