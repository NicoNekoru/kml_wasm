//! Block parser: consumes expanded source line-by-line.
//! Frontmatter, headings, lists, code blocks, display math, paragraphs.

use crate::ast::{Block, ListItem};
use crate::inline::parse_inline;
use crate::prelex::CompileError;
use std::result::Result;

pub fn parse_blocks(source: &str) -> Result<(Option<String>, Vec<Block>), CompileError> {
    let mut blocks = Vec::new();
    let mut frontmatter = None;
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;

    if let Some(first) = lines.first() {
        if first.trim() == "---" {
            let start = i;
            i += 1;
            while i < lines.len() && lines[i].trim() != "---" {
                i += 1;
            }
            if i >= lines.len() {
                return Err(CompileError {
                    message: "Unclosed frontmatter".into(),
                    offset: 0,
                });
            }
            frontmatter = Some(lines[start + 1..i].join("\n"));
            i += 1;
        }
    }

    // Indentation unit (in spaces) is determined by the first nested list item
    // encountered in the document. Inconsistent indentation is a compile error.
    let mut indent_unit: Option<usize> = None;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if trimmed.starts_with("```") {
            let (block, next) = parse_code_block(&lines, i)?;
            blocks.push(block);
            i = next;
            continue;
        }
        // Raw HTML passthrough for embeds (e.g. playground markers). Fence: :::html … closing ::: on its own line.
        if trimmed == ":::html" {
            let (block, next) = parse_html_fence_block(&lines, i)?;
            blocks.push(block);
            i = next;
            continue;
        }
        // Display math:
        // - Single-line $$...$$ (possibly indented, e.g. inside a list)
        // - Multi-line $$ on its own lines, or \[ ... \] form
        if trimmed.starts_with("$$") {
            if let Some(close_idx) = trimmed[2..].find("$$") {
                let content = trimmed[2..2 + close_idx].trim().to_string();
                blocks.push(Block::DisplayMath { content });
                i += 1;
                continue;
            }
        }
        if trimmed.starts_with("$$") || trimmed.starts_with("\\[") {
            let (block, next) = parse_display_math(&lines, i)?;
            blocks.push(block);
            i = next;
            continue;
        }
        if let Some((level, id, text)) = parse_heading(trimmed) {
            if level < 1 || level > 6 {
                return Err(CompileError {
                    message: "Heading level must be 1-6".into(),
                    offset: 0,
                });
            }
            blocks.push(Block::Heading {
                level,
                id,
                text: text.to_string(),
            });
            i += 1;
            continue;
        }
        // List: unordered "-" or ordered marker (legacy "-[x]" or new "=[x]").
        let line_indent = leading_spaces(line);
        let trimmed_start = line.trim_start();
        if is_list_marker(trimmed_start) {
            let (list_block, next) = parse_list(&lines, i, line_indent, &mut indent_unit)?;
            blocks.push(list_block);
            i = next;
            continue;
        }

        let (paragraph, next) = parse_paragraph(&lines, i)?;
        blocks.push(paragraph);
        i = next;
    }

    Ok((frontmatter, blocks))
}

fn parse_code_block(lines: &[&str], start: usize) -> Result<(Block, usize), CompileError> {
    let open = lines[start];
    let indent_len = open.len() - open.trim_start().len();
    let lang = open.trim()[3..].trim();
    let mut content = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i];
        let line_indent = line.len() - line.trim_start().len();
        if line.trim_start().starts_with("```") && line_indent == indent_len {
            let raw = content.join("\n");
            let trim_len = content
                .iter()
                .filter_map(|l: &&str| {
                    let t = l.trim_start();
                    if t.is_empty() {
                        None
                    } else {
                        Some(l.len() - t.len())
                    }
                })
                .min()
                .unwrap_or(0);
            let trimmed = if trim_len > 0 {
                content
                    .iter()
                    .map(|l: &&str| {
                        if l.len() >= trim_len {
                            &l[trim_len..]
                        } else {
                            *l
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                raw
            };
            return Ok((
                Block::CodeBlock {
                    lang: lang.to_string(),
                    content: trimmed,
                },
                i + 1,
            ));
        }
        content.push(line);
        i += 1;
    }
    Err(CompileError {
        message: "Unclosed code block".into(),
        offset: 0,
    })
}

fn parse_html_fence_block(lines: &[&str], start: usize) -> Result<(Block, usize), CompileError> {
    let mut i = start + 1;
    let mut parts: Vec<String> = Vec::new();
    while i < lines.len() {
        if lines[i].trim() == ":::" {
            return Ok((
                Block::HtmlBlock {
                    content: parts.join("\n"),
                },
                i + 1,
            ));
        }
        parts.push(lines[i].to_string());
        i += 1;
    }
    Err(CompileError {
        message: "Unclosed :::html fence (expected closing ::: on its own line)".into(),
        offset: 0,
    })
}

fn parse_display_math(lines: &[&str], start: usize) -> Result<(Block, usize), CompileError> {
    let open = lines[start].trim();
    let is_bracket = open.starts_with("\\[");
    let mut content = Vec::new();
    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i];
        if is_bracket && line.trim() == "\\]" {
            return Ok((
                Block::DisplayMath {
                    content: content.join("\n").trim().to_string(),
                },
                i + 1,
            ));
        }
        if !is_bracket && line.trim() == "$$" {
            return Ok((
                Block::DisplayMath {
                    content: content.join("\n").trim().to_string(),
                },
                i + 1,
            ));
        }
        content.push(line);
        i += 1;
    }
    Err(CompileError {
        message: "Unclosed display math".into(),
        offset: byte_offset_from_lines(lines, start, 0),
    })
}

fn parse_heading(line: &str) -> Option<(u8, Option<String>, &str)> {
    let rest = line.trim_start();
    if !rest.starts_with('#') {
        return None;
    }
    let after_hash = rest[1..].trim_start();
    let (level, after_level) = if after_hash.starts_with('[') {
        let close = after_hash.find(']')?;
        let n: u8 = after_hash[1..close].parse().ok()?;
        (n, after_hash[close + 1..].trim_start())
    } else {
        (1, after_hash)
    };
    let (text, id) = if let Some(brace) = after_level.find("{#") {
        let end = after_level[brace + 2..].find('}')?;
        (
            after_level[..brace].trim(),
            Some(after_level[brace + 2..brace + 2 + end].to_string()),
        )
    } else {
        (after_level, None)
    };
    Some((level, id, text))
}

/// Leading spaces count (indentation).
fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Approximate byte offset in the original source given line/column in the
/// `lines` view used by `parse_blocks`. Assumes one `\n` between lines.
fn byte_offset_from_lines(lines: &[&str], line_idx: usize, col: usize) -> usize {
    let mut offset = 0usize;
    for (i, line) in lines.iter().enumerate() {
        if i == line_idx {
            break;
        }
        offset += line.len() + 1; // +1 for '\n'
    }
    offset + col
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Unordered,
    Ordered { style: &'static str },
}

/// True if `label` is a non-empty Roman numeral fragment (e.g. `ii` for second item in an `i`-style list).
fn roman_numeral_marker_label(label: &str) -> bool {
    !label.is_empty()
        && label.chars().all(|c| {
            matches!(
                c,
                'i' | 'I' | 'v' | 'V' | 'x' | 'X' | 'l' | 'L' | 'c' | 'C' | 'm' | 'M'
            )
        })
}

/// Detect if a trimmed line starts with any list marker.
fn is_list_marker(trimmed: &str) -> bool {
    trimmed.starts_with("- ") || trimmed.starts_with("-[") || trimmed.starts_with("=[")
}

/// Classify a list marker and return (kind, rest_of_line_after_marker).
fn classify_marker(trimmed: &str, default_ordered: Option<ListKind>) -> Option<(ListKind, &str)> {
    // New ordered syntax: =[x]
    if let Some(rest) = trimmed.strip_prefix("=[") {
        let close = rest.find(']')?;
        let label = &rest[..close];
        let after = rest[close + 1..].trim_start();
        let style = match label {
            "a" | "A" => "a",
            "i" | "I" => "i",
            _ if label.chars().all(|c| c.is_ascii_digit()) => "1",
            _ if roman_numeral_marker_label(label) => "i",
            // Continuation markers for alphabetic lists: =[b], =[c], …
            _ if label.len() == 1
                && label
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() && !matches!(c, 'i' | 'I')) =>
            {
                "a"
            }
            _ => return None,
        };
        return Some((ListKind::Ordered { style }, after));
    }

    // Shorthand ordered syntax: =1. text, =a. text, =i. text
    if let Some(rest) = trimmed.strip_prefix('=') {
        // label_raw is everything up to the first space, e.g. "1." in "=1. Item"
        let mut iter = rest.splitn(2, char::is_whitespace);
        let label_raw = iter.next().unwrap_or("");
        let after = iter.next().unwrap_or("").trim_start();
        if !label_raw.is_empty() {
            // Strip common punctuation like '.' or ')' from the end of the label.
            let label = label_raw.trim_end_matches(&['.', ')'][..]);
            let style = match label {
                "a" | "A" => "a",
                "i" | "I" => "i",
                _ if label.chars().all(|c| c.is_ascii_digit()) => "1",
                _ => return None,
            };
            return Some((ListKind::Ordered { style }, after));
        }
    }

    // Legacy ordered syntax: -[x]
    if let Some(rest) = trimmed.strip_prefix("-[") {
        let close = rest.find(']')?;
        let label = &rest[..close];
        let after = rest[close + 1..].trim_start();
        let style = match label {
            "a" | "A" => "a",
            "i" | "I" => "i",
            _ if label.chars().all(|c| c.is_ascii_digit()) => "1",
            _ => return None,
        };
        return Some((ListKind::Ordered { style }, after));
    }

    // Unordered: "- " at the current depth.
    if let Some(rest) = trimmed.strip_prefix("- ") {
        // If the list is already known to be ordered, treat "- " as another ordered
        // item at the same depth (labels inferred by position).
        if let Some(ListKind::Ordered { style }) = default_ordered {
            return Some((ListKind::Ordered { style }, rest.trim_start()));
        }
        return Some((ListKind::Unordered, rest.trim_start()));
    }

    None
}

fn parse_list(
    lines: &[&str],
    start: usize,
    base_indent: usize,
    indent_unit: &mut Option<usize>,
) -> Result<(Block, usize), CompileError> {
    let mut items = Vec::new();
    let mut i = start;

    if i >= lines.len() {
        return Err(CompileError {
            message: "Empty list".into(),
            offset: byte_offset_from_lines(lines, start, 0),
        });
    }

    // Determine list kind from the first item at this base_indent.
    let first_line = lines[i];
    let first_trimmed = first_line.trim_start();
    let (kind, _first_rest) = classify_marker(first_trimmed, None).ok_or(CompileError {
        message: "Invalid list marker".into(),
        offset: byte_offset_from_lines(lines, start, 0),
    })?;

    let (ordered, style_opt) = match kind {
        ListKind::Unordered => (false, None),
        ListKind::Ordered { style } => (true, Some(style.to_string())),
    };

    // Parse items at this indentation until we hit a line that belongs to the parent
    // (indent < base_indent) or a non-list line at this level.
    while i < lines.len() {
        let line = lines[i];
        let indent = leading_spaces(line);
        if indent < base_indent {
            break;
        }
        let trimmed = line[indent..].trim_start();
        // Only treat markers exactly at this indent as siblings in this list.
        if indent == base_indent {
            if let Some((item_kind, rest)) = classify_marker(trimmed, Some(kind)) {
                // Enforce consistent list kind at this level.
                match (kind, item_kind) {
                    (ListKind::Unordered, ListKind::Ordered { .. })
                    | (ListKind::Ordered { .. }, ListKind::Unordered) => {
                        return Err(CompileError {
                            message: "Mixed ordered/unordered markers in the same list".into(),
                            offset: byte_offset_from_lines(lines, i, 0),
                        });
                    }
                    _ => {}
                }
                let (item, next) =
                    parse_list_item(lines, i, base_indent, &kind, rest, indent_unit)?;
                items.push(item);
                i = next;
                continue;
            } else {
                break;
            }
        } else {
            // This line is more indented than base_indent and therefore belongs to the
            // previous list item; stop this list and let the caller's list-item parser
            // handle it.
            break;
        }
    }

    Ok((
        Block::List {
            ordered,
            style: style_opt,
            items,
        },
        i,
    ))
}

fn parse_list_item(
    lines: &[&str],
    start: usize,
    base_indent: usize,
    _kind: &ListKind,
    first_rest: &str,
    indent_unit: &mut Option<usize>,
) -> Result<(ListItem, usize), CompileError> {
    let mut blocks = Vec::new();

    // First line's text becomes the leading paragraph for this item (if non-empty).
    if !first_rest.is_empty() {
        let inlines = parse_inline(first_rest)?;
        blocks.push(Block::Paragraph { inlines });
    }

    let mut i = start + 1;
    while i < lines.len() {
        let line = lines[i];
        let indent = leading_spaces(line);
        if indent <= base_indent {
            break;
        }
        let trimmed = line[indent..].trim_start();

        // Initialize or validate indent unit when we first see a nested line.
        if let Some(unit) = indent_unit {
            let delta = indent.saturating_sub(base_indent);
            if delta % *unit != 0 {
                return Err(CompileError {
                    message: "Inconsistent list indentation".into(),
                    offset: byte_offset_from_lines(lines, i, 0),
                });
            }
        } else {
            let delta = indent.saturating_sub(base_indent);
            if delta == 0 {
                return Err(CompileError {
                    message: "Invalid list indentation".into(),
                    offset: byte_offset_from_lines(lines, i, 0),
                });
            }
            *indent_unit = Some(delta);
        }

        // Nested list?
        if is_list_marker(trimmed) {
            let (nested, next) = parse_list(lines, i, indent, indent_unit)?;
            blocks.push(nested);
            i = next;
            continue;
        }

        // Nested code block or display math?
        if trimmed.starts_with("```") {
            let (block, next) = parse_code_block(lines, i)?;
            blocks.push(block);
            i = next;
            continue;
        }
        // Display math inside lists:
        // - Single-line $$...$$ (at this indent)
        // - Multi-line $$ on its own line, or \[ ... \] form
        if trimmed.starts_with("$$") {
            if let Some(close_idx) = trimmed[2..].find("$$") {
                let content = trimmed[2..2 + close_idx].trim().to_string();
                blocks.push(Block::DisplayMath { content });
                i += 1;
                continue;
            }
        }
        if trimmed.starts_with("$$") || trimmed.starts_with("\\[") {
            // parse_display_math handles both multi-line $$ and \[...\] forms.
            let (block, next) = parse_display_math(lines, i)?;
            blocks.push(block);
            i = next;
            continue;
        }

        // Otherwise, treat as paragraph content at this item's body.
        let (para, next) = parse_paragraph(lines, i)?;
        blocks.push(para);
        i = next;
    }

    Ok((ListItem { blocks }, i))
}

fn parse_paragraph(lines: &[&str], start: usize) -> Result<(Block, usize), CompileError> {
    let mut parts = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() {
            break;
        }
        parts.push(line);
        i += 1;
    }
    let text = parts.join("\n").trim().to_string();
    let inlines = parse_inline(&text)?;
    Ok((Block::Paragraph { inlines }, i))
}
