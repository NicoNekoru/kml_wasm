//! HTML emitter: walk AST and emit HTML.
//! Math nodes emitted as \( \) / \[ \] for client-side MathJax.

use crate::ast::{Block, Inline, TableAlignment, TableCell};
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
        let body: String = blocks
            .iter()
            .map(|b| self.emit_block(b))
            .collect::<Vec<_>>()
            .join("\n");
        let mut out = body;
        if !self.footnote_table.is_empty() {
            out += "\n<footer class=\"footnotes\">\n<ol>\n";
            for (i, (note, href)) in self.footnote_table.iter().enumerate() {
                let num = i + 1;
                let link_part = if href.is_empty() {
                    String::new()
                } else {
                    format!(
                        " <a href=\"{}\">{}</a>",
                        escape_html(href),
                        escape_html(href)
                    )
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
                            .map(|c| {
                                if c.is_ascii_alphanumeric() || c == '-' || c == ' ' {
                                    c
                                } else {
                                    '-'
                                }
                            })
                            .collect::<String>()
                            .split_whitespace()
                            .collect::<Vec<_>>()
                            .join("-");
                        s.chars()
                            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
                            .collect::<String>()
                    }
                };
                let id_slug = if id_slug.is_empty() {
                    "heading".to_string()
                } else {
                    id_slug
                };
                self.heading_ids.insert(text.clone(), id_slug.clone());
                let heading_inlines = parse_inline(text).unwrap_or_else(|_| {
                    vec![Inline::Text {
                        content: text.clone(),
                    }]
                });
                let inner: String = heading_inlines
                    .iter()
                    .map(|inl| self.emit_inline(inl))
                    .collect();
                format!(
                    "<h{level} id=\"{}\">{inner}</h{level}>",
                    escape_html(&id_slug)
                )
            }
            Block::Paragraph { inlines } => {
                let inner: String = inlines.iter().map(|inl| self.emit_inline(inl)).collect();
                format!("<p>{inner}</p>")
            }
            Block::List { ordered, items } => {
                if *ordered {
                    let mut out = String::from("<ol class=\"kml-ordered-list\">");
                    for item in items {
                        out.push_str("<li>");
                        if let Some(marker) = item.marker.as_deref() {
                            out.push_str(&format!(
                                "<span class=\"kml-list-marker\">{}</span>",
                                escape_html(marker)
                            ));
                        }
                        out.push_str("<div class=\"kml-list-body\">");
                        for b in &item.blocks {
                            out.push_str(&self.emit_block(b));
                        }
                        out.push_str("</div></li>");
                    }
                    out.push_str("</ol>");
                    out
                } else {
                    let mut out = String::from("<ul>");
                    for item in items {
                        out.push_str("<li>");
                        for b in &item.blocks {
                            out.push_str(&self.emit_block(b));
                        }
                        out.push_str("</li>");
                    }
                    out.push_str("</ul>");
                    out
                }
            }
            Block::Blockquote { blocks } => {
                let inner = blocks
                    .iter()
                    .map(|b| self.emit_block(b))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("<blockquote>{inner}</blockquote>")
            }
            Block::Table { rows, header_rows } => {
                let mut out = String::from("<table>");
                if *header_rows > 0 {
                    out.push_str("<thead>");
                    for row in rows.iter().take(*header_rows) {
                        out.push_str("<tr>");
                        for cell in &row.cells {
                            out.push_str(&self.emit_table_cell(cell));
                        }
                        out.push_str("</tr>");
                    }
                    out.push_str("</thead>");
                }
                if rows.len() > *header_rows {
                    out.push_str("<tbody>");
                    for row in rows.iter().skip(*header_rows) {
                        out.push_str("<tr>");
                        for cell in &row.cells {
                            out.push_str(&self.emit_table_cell(cell));
                        }
                        out.push_str("</tr>");
                    }
                    out.push_str("</tbody>");
                }
                out.push_str("</table>");
                out
            }
            Block::CodeBlock { lang, content } => {
                let lang_attr = if lang.is_empty() {
                    String::new()
                } else {
                    format!(" class=\"language-{lang}\"")
                };
                format!(
                    "<pre><code{lang_attr}>{}</code></pre>",
                    escape_html(content)
                )
            }
            Block::DisplayMath { content } => {
                format!(
                    "<div class=\"math math-display\">\\[{}\\]</div>",
                    escape_html(content)
                )
            }
            Block::HtmlBlock { content } => content.clone(),
        }
    }

    fn emit_table_cell(&mut self, cell: &TableCell) -> String {
        let tag = if cell.header { "th" } else { "td" };
        let mut attrs = String::new();
        if cell.rowspan > 1 {
            attrs.push_str(&format!(" rowspan=\"{}\"", cell.rowspan));
        }
        if cell.colspan > 1 {
            attrs.push_str(&format!(" colspan=\"{}\"", cell.colspan));
        }
        if let Some(align) = cell.align {
            let value = match align {
                TableAlignment::Left => "left",
                TableAlignment::Center => "center",
                TableAlignment::Right => "right",
            };
            attrs.push_str(&format!(" style=\"text-align: {value}\""));
        }
        let inner: String = cell
            .inlines
            .iter()
            .map(|inl| self.emit_inline(inl))
            .collect();
        format!("<{tag}{attrs}>{inner}</{tag}>")
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
                format!(
                    "<span class=\"math math-inline\">\\({}\\)</span>",
                    escape_html(content)
                )
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
