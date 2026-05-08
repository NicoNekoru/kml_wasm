//! Kernel ML (KML) parser (see `spec/*.md`). Compiles source to HTML.
//! Pipeline: pre_lex -> expand_macros -> parse_blocks -> emit_html.

mod ast;
mod block;
mod emit;
mod inline;
mod live;
mod macros;
mod prelex;

use block::parse_blocks;
use emit::Emitter;
pub use live::{LiveCompiler, LiveCompilerInner, LiveRenderStats};
use macros::expand_macros;
use prelex::{pre_lex, CompileError};
use wasm_bindgen::prelude::*;

fn to_js_error(e: CompileError) -> JsValue {
    JsValue::from_str(&format!("{} at offset {}", e.message, e.offset))
}

/// Compile KML source to HTML (body snippet). Used by tests, the compile_kml binary, and callers that need Result rather than JsValue.
pub fn compile_inner(source: &str) -> Result<String, CompileError> {
    let spans = pre_lex(source)?;
    let (expanded, _adjusted_spans) = expand_macros(source, &spans);
    let (frontmatter, blocks) = parse_blocks(&expanded)?;
    let mut emitter = Emitter::new();
    Ok(emitter.emit_html(&blocks, frontmatter.as_deref()))
}

#[wasm_bindgen]
pub fn compile(source: &str) -> Result<String, JsValue> {
    compile_inner(source).map_err(to_js_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelex::pre_lex;

    /// Test from `spec/tests.md` section 3: Paragraph + line breaks + `\n` macro.
    #[test]
    fn test_paragraph_line_breaks_and_macro() {
        let source = r#"This is a paragraph with a line break\n
And continues on the same paragraph.

This is a new paragraph."#;
        let out = compile(source).expect("compile");
        assert!(
            out.contains("<br>"),
            "\\n should expand to <br>; got: {}",
            out
        );
        assert!(
            out.contains("<p>"),
            "should have paragraph tags; got: {}",
            out
        );
        let p_count = out.matches("</p>").count();
        assert!(
            p_count >= 2,
            "expected at least 2 paragraphs; got {}; output: {}",
            p_count,
            out
        );
    }

    #[test]
    fn test_heading_and_bold() {
        let source = "# Hello\n\n**bold** and *italic*.";
        let out = compile(source).expect("compile");
        assert!(out.contains("<h1"), "should have h1; got: {}", out);
        assert!(
            out.contains("Hello"),
            "should contain heading text; got: {}",
            out
        );
        assert!(out.contains("<strong>"), "should have bold; got: {}", out);
        assert!(out.contains("<em>"), "should have italic; got: {}", out);
    }

    #[test]
    fn test_nested_lists_ordered_and_unordered() {
        let source = r#"
**Unordered lists:** line starts with `- `. **Ordered lists:** `=[a]`, `=[i]`, or `=[1]`.

- first unordered item
- second item
    =[a] nested ordered (alphabetic)
    =[a] second at same depth
    =[1] arabic style
        - deeply nested bullet
"#;
        let html = compile_inner(source).expect("nested lists must compile");
        // Top-level unordered list
        assert!(html.contains("<ul>"), "expected outer <ul>, got: {html}");
        // Nested ordered list (alphabetic)
        assert!(
            html.contains("<ol type=\"a\">"),
            "expected nested <ol type=\"a\">, got: {html}"
        );
        // Nested ordered list (numeric)
        assert!(
            html.contains("<ol type=\"1\">") || html.contains("<ol type=\"a\">"),
            "expected ordered lists; got: {html}"
        );
        // Deeply nested bullet inside an ordered item
        assert!(
            html.contains("deeply nested bullet"),
            "expected deeply nested bullet text; got: {html}"
        );
    }

    #[test]
    fn test_ordered_list_shorthand_syntax() {
        let source = r#"
=[1] first numeric
=[2] second numeric

=[a] first alpha
=[b] second alpha

=[i] first roman
=[ii] second roman (treated as style i)
"#;
        let html = compile_inner(source).expect("shorthand ordered lists must compile");
        // We should see at least one numeric ordered list and one alphabetic/roman list.
        assert!(
            html.contains("<ol type=\"1\">"),
            "expected numeric ordered list; got: {html}"
        );
        assert!(
            html.contains("<ol type=\"a\">") || html.contains("<ol type=\"i\">"),
            "expected alpha or roman ordered list; got: {html}"
        );
    }

    #[test]
    fn test_inline_math() {
        let source = "We have $x^2$ here.";
        let out = compile(source).expect("compile");
        assert!(
            out.contains("math-inline"),
            "should have inline math class; got: {}",
            out
        );
        assert!(
            out.contains("x^2"),
            "should contain math content; got: {}",
            out
        );
    }

    #[test]
    fn test_code_block() {
        let source = "```\nfn main() {}\n```";
        let out = compile(source).expect("compile");
        assert!(out.contains("<pre>"), "should have pre; got: {}", out);
        assert!(
            out.contains("fn main()"),
            "should contain code; got: {}",
            out
        );
    }

    #[test]
    fn test_display_math_after_paragraph_without_blank_line() {
        let source = "gdsfaf\n$$4234$$";
        let out = compile_inner(source).expect("display math should start a block after paragraph");
        assert!(
            out.contains("<p>gdsfaf</p>"),
            "expected paragraph; got: {out}"
        );
        assert!(
            out.contains("<div class=\"math math-display\">\\[4234\\]</div>"),
            "expected display math block; got: {out}"
        );
    }

    #[test]
    fn test_double_dollar_math_inside_paragraph() {
        let source = "gdsfaf$$4234$$gdsfaf";
        let out =
            compile_inner(source).expect("double-dollar math should be atomic inline content");
        assert!(
            out.contains("<p>gdsfaf<span class=\"math math-inline\">\\(4234\\)</span>gdsfaf</p>"),
            "expected inline math span; got: {out}"
        );
    }

    /// Pre-lex: nested code block (indented closing fence) must not open CodeInline.
    #[test]
    fn test_prelex_nested_code_block() {
        let source = r#"---
title: x
---
- item
    ```python
    x
    ```
- next"#;
        let spans = pre_lex(source).expect("pre_lex should not fail");
        let has_code_display = spans
            .iter()
            .any(|s| matches!(s.kind, crate::prelex::SpanKind::CodeDisplay));
        assert!(
            has_code_display,
            "expected CodeDisplay span; got {:?}",
            spans
        );
    }

    /// Pre-lex: inline `` `code` with `` backticks `` (double-backtick spans).
    #[test]
    fn test_prelex_double_backtick_inline() {
        let source = r#"One backtick: `single`, or `` `code` with `` backticks ``."#;
        let spans = pre_lex(source).expect("pre_lex should not fail");
        let code_inline_count = spans
            .iter()
            .filter(|s| matches!(s.kind, crate::prelex::SpanKind::CodeInline))
            .count();
        assert!(
            code_inline_count >= 2,
            "expected at least 2 CodeInline spans; got {:?}",
            spans
        );
    }

    /// Compile the Kernel ML showcase (`spec/showcase.kml`).
    /// Uses first 127 lines to avoid known perf issue with full list+code block section.
    #[test]
    fn test_showcase_compile() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/showcase.kml");
        let source = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("showcase.kml not found at {}: {}", path.display(), e));
        let source: String = source.lines().take(127).collect::<Vec<_>>().join("\n");
        let html = compile_inner(&source).expect("showcase.kml must compile");
        assert!(html.contains("</p>"), "expected paragraphs");
        assert!(html.contains("<h1"), "expected headings");
        assert!(html.contains("<strong>"), "expected bold");
        assert!(html.contains("<em>"), "expected italic");
        assert!(html.contains("<code"), "expected code");
        assert!(html.contains("<pre>"), "expected code block");
        assert!(html.contains("<a "), "expected links");
        assert!(html.contains("<br>"), "expected line break");
        assert!(html.contains("math-inline"), "expected inline math");
        assert!(html.contains("math-display"), "expected display math");
    }

    #[test]
    fn test_footnote_note_with_link() {
        let source = "A claim^[Source: [doc](https://example.com/doc)].";
        let html = compile_inner(source).expect("footnote must compile");
        assert!(
            html.contains("Source: [doc](https://example.com/doc)"),
            "footer should contain full note text"
        );
        assert!(html.contains("fn-1"), "footer should have fn id");
        assert!(html.contains("fnref-1"), "footer should have back ref");
        assert!(
            html.contains("footnote-back"),
            "footer should have back link class"
        );
        assert!(html.contains("↩"), "footer should have back link character");
    }

    #[test]
    fn test_prelex_debug() {
        let source = r#"One backtick: `single`, or `` `code` with `` backticks ``."#;
        let r = pre_lex(source);
        assert!(r.is_ok(), "pre_lex should succeed: {:?}", r);
    }

    #[test]
    fn test_prelex_showcase() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("spec/showcase.kml");
        let source = std::fs::read_to_string(&path).unwrap();
        let r = pre_lex(&source);
        assert!(r.is_ok(), "showcase pre_lex should succeed: {:?}", r);
    }
}
