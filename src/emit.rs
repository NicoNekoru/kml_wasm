//! HTML emitter: walk AST and emit HTML.
//! Math nodes emitted as \( \) / \[ \] for client-side MathJax.

use crate::ast::{Block, Inline};
use crate::inline::parse_inline;
use std::collections::HashMap;

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub struct Emitter {
    footnote_table: Vec<(String, String)>,
    heading_ids: HashMap<String, String>,
}

impl Emitter {
    pub fn new() -> Self {
        Self {
            footnote_table: Vec::new(),
            heading_ids: HashMap::new(),
        }
    }

    pub fn emit_html(&mut self, blocks: &[Block], _frontmatter: Option<&str>) -> String {
        self.footnote_table.clear();
        self.heading_ids.clear();
        let body: String = blocks.iter().map(|b| self.emit_block(b)).collect::<Vec<_>>().join("\n");
        let mut out = body;
        if !self.footnote_table.is_empty() {
            out += "\n<footer class=\"footnotes\">\n<ol>\n";
            for (i, (note, href)) in self.footnote_table.iter().enumerate() {
                let num = i + 1;
                let link_part = if href.is_empty() {
                    String::new()
                } else {
                    format!(" <a href=\"{}\">{}</a>", escape_html(href), escape_html(href))
                };
                out += &format!(
                    "<li id=\"fn-{num}\">{}{} <a href=\"#fnref-{num}\" class=\"footnote-back\">↩</a></li>\n",
                    escape_html(note),
                    link_part
                );
            }
            out += "</ol>\n</footer>\n";
        }
        out
    }

    fn emit_block(&mut self, block: &Block) -> String {
        match block {
            Block::Heading { level, id, text } => {
                let id_slug = match id {
                    Some(ref id) => id.clone(),
                    None => {
                        let s: String = text
                            .to_lowercase()
                            .chars()
                            .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == ' ' { c } else { '-' })
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join("-");
                        s.chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                            .collect::<String>()
                    }
                };
                let id_slug = if id_slug.is_empty() { "heading".to_string() } else { id_slug };
                self.heading_ids.insert(text.clone(), id_slug.clone());
                let heading_inlines = parse_inline(text).unwrap_or_else(|_| vec![Inline::Text { content: text.clone() }]);
                let inner: String = heading_inlines.iter().map(|inl| self.emit_inline(inl)).collect();
                format!("<h{level} id=\"{}\">{inner}</h{level}>", escape_html(&id_slug))
            }
            Block::Paragraph { inlines } => {
                let inner: String = inlines.iter().map(|inl| self.emit_inline(inl)).collect();
                format!("<p>{inner}</p>")
            }
            Block::List { ordered, style, items } => {
                let tag = if *ordered { "ol" } else { "ul" };
                let mut attrs = String::new();
                if *ordered {
                    if let Some(s) = style.as_deref() {
                        // "1", "a", or "i" – anything else is ignored and falls back to default.
                        if s == "1" || s == "a" || s == "i" {
                            attrs = format!(" type=\"{s}\"");
                        }
                    }
                }
                let mut out = format!("<{tag}{attrs}>");
                for item in items {
                    out.push_str("<li>");
                    for b in &item.blocks {
                        out.push_str(&self.emit_block(b));
                    }
                    out.push_str("</li>");
                }
                out.push_str(&format!("</{tag}>"));
                out
            }
            Block::CodeBlock { lang, content } => {
                let lang_attr = if lang.is_empty() {
                    String::new()
                } else {
                    format!(" class=\"language-{lang}\"")
                };
                format!("<pre><code{lang_attr}>{}</code></pre>", escape_html(content))
            }
            Block::DisplayMath { content } => {
                format!("<div class=\"math math-display\">\\[{}\\]</div>", escape_html(content))
            }
            Block::HtmlBlock { content } => content.clone(),
            Block::FrontMatter { .. } => String::new(),
        }
    }

    fn emit_inline(&mut self, inline: &Inline) -> String {
        match inline {
            Inline::Text { content } => escape_html(content),
            Inline::LineBreak => "<br>".to_string(),
            Inline::Bold { children } => {
                let inner: String = children.iter().map(|c| self.emit_inline(c)).collect();
                format!("<strong>{inner}</strong>")
            }
            Inline::Italic { children } => {
                let inner: String = children.iter().map(|c| self.emit_inline(c)).collect();
                format!("<em>{inner}</em>")
            }
            Inline::Code { content } => format!("<code>{}</code>", escape_html(content)),
            Inline::InlineMath { content } => {
                format!("<span class=\"math math-inline\">\\({}\\)</span>", escape_html(content))
            }
            Inline::Link { text, href } => {
                format!(
                    "<a href=\"{}\">{}</a>",
                    escape_html(href),
                    escape_html(text)
                )
            }
            Inline::Footnote { note, href } => {
                let num = self.footnote_table.len() + 1;
                self.footnote_table.push((note.clone(), href.clone()));
                format!(
                    "<sup class=\"footnote-ref\"><a href=\"#fn-{num}\" id=\"fnref-{num}\">{num}</a></sup>"
                )
            }
            Inline::Superscript { content } => format!("<sup>{}</sup>", escape_html(content)),
            Inline::Subscript { content } => format!("<sub>{}</sub>", escape_html(content)),
        }
    }
}

impl Default for Emitter {
    fn default() -> Self {
        Self::new()
    }
}
