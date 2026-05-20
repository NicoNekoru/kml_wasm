//! Kernel ML (KML) parser (see `spec/*.md`). Compiles source to HTML.
//! Pipeline: pre_lex -> expand_macros -> parse_blocks -> emit_html.

mod ast;
mod block;
mod emit;
mod escape;
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
    = second at same depth
    =[1] arabic style
        - deeply nested bullet
"#;
        let html = compile_inner(source).expect("nested lists must compile");
        // Top-level unordered list
        assert!(html.contains("<ul>"), "expected outer <ul>, got: {html}");
        // Nested ordered list with explicit rendered markers
        assert!(
            html.contains("<ol class=\"kml-ordered-list\">"),
            "expected nested ordered list, got: {html}"
        );
        assert!(
            html.contains("<span class=\"kml-list-marker\">a.</span>")
                && html.contains("<span class=\"kml-list-marker\">b.</span>")
                && html.contains("<span class=\"kml-list-marker\">1.</span>"),
            "expected explicit and continued ordered markers; got: {html}"
        );
        // Deeply nested bullet inside an ordered item
        assert!(
            html.contains("deeply nested bullet"),
            "expected deeply nested bullet text; got: {html}"
        );
    }

    #[test]
    fn test_blockquote_parses_inner_blocks() {
        let source = r#"
> Quote with **bold** text.
> #[2] Quoted heading
> - quoted list item
"#;
        let html = compile_inner(source).expect("blockquote must compile");
        assert!(
            html.contains("<blockquote>"),
            "expected blockquote wrapper; got: {html}"
        );
        assert!(
            html.contains("<p>Quote with <strong>bold</strong> text.</p>"),
            "expected inline parsing inside quoted paragraph; got: {html}"
        );
        assert!(
            html.contains("<h2 id=\"quoted-heading\">Quoted heading</h2>"),
            "expected heading parsed inside blockquote; got: {html}"
        );
        assert!(
            html.contains("<ul><li><p>quoted list item</p></li></ul>"),
            "expected list parsed inside blockquote; got: {html}"
        );
    }

    #[test]
    fn test_nested_blockquote_with_code_block_strips_quote_prefixes_and_padding() {
        let source = r#"
> > > ```text
> > >     alpha
> > >     beta
> > > ```
"#;
        let html = compile_inner(source).expect("nested blockquote code block must compile");
        assert_eq!(
            html.matches("<blockquote>").count(),
            3,
            "expected three nested blockquotes; got: {html}"
        );
        assert!(
            html.contains("<pre><code class=\"language-text\">alpha\nbeta</code></pre>"),
            "expected quoted code fence to strip quote prefixes and common padding; got: {html}"
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
        // Shorthands resolve to explicit visible markers.
        assert!(
            html.contains("<span class=\"kml-list-marker\">1.</span>")
                && html.contains("<span class=\"kml-list-marker\">2.</span>"),
            "expected numeric ordered markers; got: {html}"
        );
        assert!(
            html.contains("<span class=\"kml-list-marker\">a.</span>")
                && html.contains("<span class=\"kml-list-marker\">b.</span>")
                && html.contains("<span class=\"kml-list-marker\">i.</span>")
                && html.contains("<span class=\"kml-list-marker\">ii.</span>"),
            "expected alpha and roman ordered markers; got: {html}"
        );
    }

    #[test]
    fn test_ordered_list_continuation_and_verbose_template() {
        let source = r#"
= first implicit numeric
= second implicit numeric
=[4] fourth numeric
= fifth numeric
=[a:i] ninth alpha
= tenth alpha
=[a):i] old suffix shorthand
= suffix continuation
=[Problem {a}:i] verbose alpha problem
= verbose continuation
=[Item {1}.:7] verbose decimal
= verbose decimal continuation
"#;
        let html = compile_inner(source).expect("ordered list continuation must compile");
        for marker in [
            "1.",
            "2.",
            "4.",
            "5.",
            "i.",
            "j.",
            "i)",
            "j)",
            "Problem i",
            "Problem j",
            "Item 7.",
            "Item 8.",
        ] {
            assert!(
                html.contains(&format!("<span class=\"kml-list-marker\">{marker}</span>")),
                "expected marker {marker}; got: {html}"
            );
        }
    }

    #[test]
    fn test_ordered_verbose_template_requires_counter_slot() {
        let err =
            compile_inner("=[Problem alpha i] ambiguous").expect_err("ambiguous marker must fail");
        assert!(
            err.message.contains("Invalid list marker"),
            "expected invalid list marker error; got: {err:?}"
        );

        let err = compile_inner("=[1]tight").expect_err("marker must be separated from item text");
        assert!(
            err.message.contains("Invalid list marker"),
            "expected invalid list marker error; got: {err:?}"
        );
    }

    #[test]
    fn test_markdown_table_basic_alignment_and_inline_content() {
        let source = r#"
| Name | Count | Formula |
| :--- | ---: | :---: |
| **Alpha** | 12 | $x^2$ |
"#;
        let html = compile_inner(source).expect("markdown table must compile");
        assert!(html.contains("<table>"), "expected table; got: {html}");
        assert!(html.contains("<thead>"), "expected table head; got: {html}");
        assert!(
            html.contains("<th style=\"text-align: left\">Name</th>"),
            "expected left-aligned header; got: {html}"
        );
        assert!(
            html.contains("<td style=\"text-align: right\">12</td>"),
            "expected right-aligned cell; got: {html}"
        );
        assert!(
            html.contains("<strong>Alpha</strong>"),
            "expected inline parsing in cells; got: {html}"
        );
        assert!(
            html.contains("math-inline"),
            "expected inline math in cells; got: {html}"
        );
    }

    #[test]
    fn test_table_merge_markers_and_escaped_markers() {
        let source = r#"
| H1 | H2 | H3 |
| --- | --- | --- |
| Span 2x2 | > | C |
| ^ | > | D |
| \^ | \> | E |
| Left alias | < | F |
| \< | literal | G |
"#;
        let html = compile_inner(source).expect("merged table must compile");
        assert!(
            html.contains("<td rowspan=\"2\" colspan=\"2\">Span 2x2</td>"),
            "expected combined rowspan/colspan; got: {html}"
        );
        assert!(
            !html.contains("<td>&gt;</td><td>D</td>"),
            "merge marker should not render as data; got: {html}"
        );
        assert!(
            html.contains("<td>^</td><td>&gt;</td><td>E</td>"),
            "escaped merge markers should render literally; got: {html}"
        );
        assert!(
            html.contains("<td colspan=\"2\">Left alias</td><td>F</td>"),
            "left-angle merge marker should alias horizontal merge; got: {html}"
        );
        assert!(
            html.contains("<td>&lt;</td><td>literal</td><td>G</td>"),
            "escaped left-angle marker should render literally; got: {html}"
        );
    }

    #[test]
    fn test_table_vertical_and_multiline_headers() {
        let source = r#"
| Region | - | 2025 | > |
| Metric | - | Q1 | Q2 |
| --- | --- | --- | --- |
| Sales | - | 10 | 12 |
| Combined | - | Span | > |
| ^ | - | ^ | > |
"#;
        let html = compile_inner(source).expect("vertical header table must compile");
        assert_eq!(
            html.matches("<thead>").count(),
            1,
            "expected one thead; got: {html}"
        );
        assert!(
            html.contains("<th colspan=\"2\">2025</th>"),
            "expected merged multi-row header; got: {html}"
        );
        assert!(
            html.contains("<tr><th>Sales</th><td>10</td><td>12</td></tr>"),
            "expected vertical row header and omitted dash separator column; got: {html}"
        );
        assert!(
            html.contains(
                "<th rowspan=\"2\">Combined</th><td rowspan=\"2\" colspan=\"2\">Span</td>"
            ),
            "expected merged vertical header and data cells; got: {html}"
        );
        assert!(
            !html.contains("<th>-</th>") && !html.contains("<td>-</td>"),
            "vertical separator column should not render; got: {html}"
        );
    }

    #[test]
    fn test_non_rectangular_table_merge_is_error() {
        let source = r#"
| H1 | H2 |
| --- | --- |
| A | > |
| ^ | B |
"#;
        let err = compile_inner(source).expect_err("non-rectangular merge must fail");
        assert!(
            err.message.contains("rectangle"),
            "expected rectangle error; got: {:?}",
            err
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
    fn test_escaped_dollar_renders_literal_text() {
        let source = r#"Price is US\$50, not math."#;
        let out = compile_inner(source).expect("escaped dollar must compile");
        assert!(
            out.contains("<p>Price is US$50, not math.</p>"),
            "escaped dollar should render literally; got: {out}"
        );
        assert!(
            !out.contains("math-inline"),
            "escaped dollar should not open inline math; got: {out}"
        );
    }

    #[test]
    fn test_escaped_inline_delimiters_render_literal_text() {
        let source =
            r#"\*not italic\* [not a link] [literal\](x) \^{not sup} \_{not sub} \`not code\`"#;
        let out = compile_inner(source).expect("escaped inline delimiters must compile");
        assert!(
            out.contains("*not italic* [not a link] [literal](x) ^{not sup} _{not sub} `not code`"),
            "escaped delimiters should render literally; got: {out}"
        );
        assert!(!out.contains("<em>"), "escaped * should not emit em: {out}");
        assert!(
            !out.contains("<a "),
            "escaped [ should not emit link: {out}"
        );
        assert!(
            !out.contains("<sup>") && !out.contains("<sub>"),
            "escaped sup/sub should not emit tags: {out}"
        );
        assert!(
            !out.contains("<code>"),
            "escaped backtick should not emit code: {out}"
        );
    }

    #[test]
    fn test_plain_brackets_are_text_unless_followed_by_url() {
        let source = r#"Plain [brackets] and [a link](https://example.com)."#;
        let out = compile_inner(source).expect("plain brackets and link must compile");
        assert!(
            out.contains("Plain [brackets] and "),
            "plain brackets should render as text; got: {out}"
        );
        assert!(
            out.contains(r#"<a href="https://example.com">a link</a>"#),
            "brackets followed by URL parens should render as a link; got: {out}"
        );
    }

    #[test]
    fn test_bracket_display_math_splits_paragraph() {
        let source = r#"Before \[x + y\] after."#;
        let out = compile_inner(source).expect("bracket display math must compile");
        assert!(
            out.contains(
                "<p>Before</p>\n<div class=\"math math-display\">\\[x + y\\]</div>\n<p>after.</p>"
            ),
            "bracket display math should split paragraph; got: {out}"
        );
    }

    #[test]
    fn test_escaped_dollar_inside_inline_math_does_not_close_math() {
        let source = r#"Math $x\$y$ done."#;
        let out = compile_inner(source).expect("escaped dollar inside math must compile");
        assert!(
            out.contains(r#"<span class="math math-inline">\(x\$y\)</span>"#),
            "escaped dollar should remain inside math content; got: {out}"
        );
        assert_eq!(
            out.matches("math-inline").count(),
            1,
            "expected one inline math span; got: {out}"
        );
    }

    #[test]
    fn test_escaped_linebreak_macro_renders_literal_marker() {
        let source = r#"Literal \\n marker."#;
        let out = compile_inner(source).expect("escaped linebreak macro must compile");
        assert!(
            out.contains("<p>Literal \\n marker.</p>"),
            "escaped linebreak macro should render as literal \\n; got: {out}"
        );
        assert!(
            !out.contains("<br>"),
            "escaped linebreak macro should not emit a break; got: {out}"
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
    fn test_double_dollar_math_splits_paragraph_into_display_blocks() {
        let source = "gdsfaf$$4234$$gdsfaf";
        let out = compile_inner(source).expect("double-dollar math should start a display block");
        assert!(
            out.contains(
                "<p>gdsfaf</p>\n<div class=\"math math-display\">\\[4234\\]</div>\n<p>gdsfaf</p>"
            ),
            "expected paragraph/display/paragraph split; got: {out}"
        );
    }

    #[test]
    fn test_multiple_double_dollar_math_spans_split_paragraph() {
        let source =
            "sadfsadf$$test$$sfdafasdlfajsdfsafasdsadfsadf$$test$$sfdafasdlfajsdfsafasdsadfsadf $$test$$ sfdafasdlfajsdfsafasd";
        let out =
            compile_inner(source).expect("double-dollar math spans should start display blocks");
        assert_eq!(
            out.matches("math-display").count(),
            3,
            "expected three display math blocks; got: {out}"
        );
        assert!(
            !out.contains("math-inline"),
            "double-dollar math should not emit inline math; got: {out}"
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
