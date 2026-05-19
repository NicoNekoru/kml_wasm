//! Block parser: consumes expanded source line-by-line.
//! Frontmatter, headings, lists, code blocks, display math, paragraphs.

use crate::ast::{Block, Inline, ListItem, TableAlignment, TableCell, TableRow};
use crate::escape::{find_unescaped_sequence, is_escaped_at};
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
            if let Some(close_idx) = find_unescaped_sequence(trimmed, 2, "$$") {
                let content = trimmed[2..close_idx].trim().to_string();
                blocks.push(Block::DisplayMath { content });
                let tail = trimmed[close_idx + 2..].trim();
                if !tail.is_empty() {
                    blocks.extend(parse_paragraph_text(tail)?);
                }
                i += 1;
                continue;
            }
        }
        if trimmed.starts_with("\\[") {
            if let Some(close_idx) = find_unescaped_sequence(trimmed, 2, "\\]") {
                let content = trimmed[2..close_idx].trim().to_string();
                blocks.push(Block::DisplayMath { content });
                let tail = trimmed[close_idx + 2..].trim();
                if !tail.is_empty() {
                    blocks.extend(parse_paragraph_text(tail)?);
                }
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
        if let Some((table_block, next)) = parse_table(&lines, i)? {
            blocks.push(table_block);
            i = next;
            continue;
        }

        let (paragraph_blocks, next) = parse_paragraph(&lines, i)?;
        blocks.extend(paragraph_blocks);
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

#[derive(Debug, Clone)]
struct RawTableRow {
    source_line: usize,
    cells: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TableMergeMarker {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone)]
struct MutableTableCell {
    origin_row: usize,
    origin_col: usize,
    in_header_section: bool,
    inlines: Vec<Inline>,
    header: bool,
    align: Option<TableAlignment>,
    rowspan: usize,
    colspan: usize,
}

impl MutableTableCell {
    fn include(&mut self, row: usize, col: usize) {
        self.rowspan = self.rowspan.max(row - self.origin_row + 1);
        self.colspan = self.colspan.max(col - self.origin_col + 1);
    }

    fn into_cell(self) -> TableCell {
        TableCell {
            inlines: self.inlines,
            header: self.header,
            align: self.align,
            rowspan: self.rowspan,
            colspan: self.colspan,
        }
    }
}

fn parse_table(lines: &[&str], start: usize) -> Result<Option<(Block, usize)>, CompileError> {
    let Some(delimiter_idx) = find_table_delimiter(lines, start) else {
        return Ok(None);
    };

    let delimiter_cells = split_table_row(lines[delimiter_idx]);
    let alignments = parse_table_delimiter(&delimiter_cells).ok_or_else(|| CompileError {
        message: "Invalid table delimiter row".into(),
        offset: byte_offset_from_lines(lines, delimiter_idx, 0),
    })?;
    let width = alignments.len();

    let mut raw_rows = Vec::new();
    for line_idx in start..delimiter_idx {
        raw_rows.push(parse_table_row(lines, line_idx, width)?);
    }
    let header_rows = raw_rows.len();

    let mut next = delimiter_idx + 1;
    while next < lines.len() {
        if lines[next].trim().is_empty() || !is_table_row_like(lines[next]) {
            break;
        }
        raw_rows.push(parse_table_row(lines, next, width)?);
        next += 1;
    }

    let vertical_separator_cols = find_vertical_separator_cols(&raw_rows, width);
    let rows = resolve_table_rows(
        lines,
        &raw_rows,
        header_rows,
        &alignments,
        &vertical_separator_cols,
    )?;

    Ok(Some((Block::Table { rows, header_rows }, next)))
}

fn find_table_delimiter(lines: &[&str], start: usize) -> Option<usize> {
    let mut i = start;
    while i < lines.len() {
        if lines[i].trim().is_empty() || !is_table_row_like(lines[i]) {
            break;
        }
        let cells = split_table_row(lines[i]);
        if i > start && parse_table_delimiter(&cells).is_some() {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn parse_table_row(
    lines: &[&str],
    line_idx: usize,
    width: usize,
) -> Result<RawTableRow, CompileError> {
    let cells = split_table_row(lines[line_idx]);
    if cells.len() != width {
        return Err(CompileError {
            message: format!(
                "Table row has {} cells but delimiter row has {}",
                cells.len(),
                width
            ),
            offset: byte_offset_from_lines(lines, line_idx, 0),
        });
    }
    Ok(RawTableRow {
        source_line: line_idx,
        cells,
    })
}

fn is_table_row_like(line: &str) -> bool {
    split_table_row(line).len() >= 2
}

fn split_table_row(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut code_run: Option<usize> = None;

    while i < line.len() {
        let c = line[i..].chars().next().unwrap();
        if c == '`' && !is_escaped_at(line, i) {
            let run = count_char_run(line, i, '`');
            if code_run == Some(run) {
                code_run = None;
            } else if code_run.is_none() {
                code_run = Some(run);
            }
            i += byte_len_char_run(line, i, run);
            continue;
        }

        if c == '|' && code_run.is_none() && !is_escaped_at(line, i) {
            cells.push(line[start..i].to_string());
            i += c.len_utf8();
            start = i;
            continue;
        }

        i += c.len_utf8();
    }
    cells.push(line[start..].to_string());

    if starts_with_table_pipe(line) && cells.first().is_some_and(|cell| cell.trim().is_empty()) {
        cells.remove(0);
    }
    if ends_with_table_pipe(line) && cells.last().is_some_and(|cell| cell.trim().is_empty()) {
        cells.pop();
    }

    cells
}

fn starts_with_table_pipe(line: &str) -> bool {
    line.char_indices()
        .find(|(_, c)| !c.is_whitespace())
        .is_some_and(|(idx, c)| c == '|' && !is_escaped_at(line, idx))
}

fn ends_with_table_pipe(line: &str) -> bool {
    line.char_indices()
        .rev()
        .find(|(_, c)| !c.is_whitespace())
        .is_some_and(|(idx, c)| c == '|' && !is_escaped_at(line, idx))
}

fn count_char_run(s: &str, byte_idx: usize, needle: char) -> usize {
    s[byte_idx..].chars().take_while(|c| *c == needle).count()
}

fn byte_len_char_run(s: &str, byte_idx: usize, run: usize) -> usize {
    s[byte_idx..].chars().take(run).map(char::len_utf8).sum()
}

fn parse_table_delimiter(cells: &[String]) -> Option<Vec<Option<TableAlignment>>> {
    if cells.len() < 2 {
        return None;
    }

    cells
        .iter()
        .map(|cell| parse_table_alignment(cell.trim()))
        .collect()
}

fn parse_table_alignment(spec: &str) -> Option<Option<TableAlignment>> {
    if spec.is_empty() {
        return None;
    }

    let left_colon = spec.starts_with(':');
    let right_colon = spec.ends_with(':');
    let dashes = spec.trim_start_matches(':').trim_end_matches(':');
    if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
        return None;
    }

    let align = match (left_colon, right_colon) {
        (true, true) => Some(TableAlignment::Center),
        (false, true) => Some(TableAlignment::Right),
        (true, false) => Some(TableAlignment::Left),
        (false, false) => None,
    };
    Some(align)
}

fn find_vertical_separator_cols(rows: &[RawTableRow], width: usize) -> Vec<bool> {
    (0..width)
        .map(|col| {
            !rows.is_empty()
                && rows
                    .iter()
                    .all(|row| is_unescaped_dash_cell(&row.cells[col]))
        })
        .collect()
}

fn is_unescaped_dash_cell(raw: &str) -> bool {
    let trimmed = raw.trim();
    !trimmed.starts_with('\\') && !trimmed.is_empty() && trimmed.chars().all(|c| c == '-')
}

fn table_merge_marker(raw: &str) -> Option<TableMergeMarker> {
    match raw.trim() {
        ">" | "<" => Some(TableMergeMarker::Horizontal),
        "^" => Some(TableMergeMarker::Vertical),
        _ => None,
    }
}

fn resolve_table_rows(
    lines: &[&str],
    raw_rows: &[RawTableRow],
    header_rows: usize,
    alignments: &[Option<TableAlignment>],
    vertical_separator_cols: &[bool],
) -> Result<Vec<TableRow>, CompileError> {
    let row_count = raw_rows.len();
    let width = alignments.len();
    let first_vertical_separator = vertical_separator_cols.iter().position(|is_sep| *is_sep);
    let mut mutable_cells: Vec<MutableTableCell> = Vec::new();
    let mut occupancy: Vec<Vec<Option<usize>>> = vec![vec![None; width]; row_count];

    for row in 0..row_count {
        for col in 0..width {
            if vertical_separator_cols[col] {
                continue;
            }

            let raw = &raw_rows[row].cells[col];
            match table_merge_marker(raw) {
                Some(TableMergeMarker::Horizontal) => {
                    if col == 0 {
                        return Err(table_error(
                            lines,
                            raw_rows,
                            row,
                            "Horizontal table merge marker has no cell to its left",
                        ));
                    }
                    let Some(cell_id) = occupancy[row][col - 1] else {
                        return Err(table_error(
                            lines,
                            raw_rows,
                            row,
                            "Horizontal table merge marker has no cell to its left",
                        ));
                    };
                    mutable_cells[cell_id].include(row, col);
                    occupancy[row][col] = Some(cell_id);
                }
                Some(TableMergeMarker::Vertical) => {
                    if row == 0 {
                        return Err(table_error(
                            lines,
                            raw_rows,
                            row,
                            "Vertical table merge marker has no cell above it",
                        ));
                    }
                    let Some(cell_id) = occupancy[row - 1][col] else {
                        return Err(table_error(
                            lines,
                            raw_rows,
                            row,
                            "Vertical table merge marker has no cell above it",
                        ));
                    };
                    let current_header_section = row < header_rows;
                    if mutable_cells[cell_id].in_header_section != current_header_section {
                        return Err(table_error(
                            lines,
                            raw_rows,
                            row,
                            "Table merge cannot cross the header/body boundary",
                        ));
                    }
                    mutable_cells[cell_id].include(row, col);
                    occupancy[row][col] = Some(cell_id);
                }
                None => {
                    let content = unescape_table_cell(raw);
                    let in_header_section = row < header_rows;
                    let vertical_header = first_vertical_separator.is_some_and(|sep| col < sep);
                    let header = in_header_section || vertical_header;
                    let cell_id = mutable_cells.len();
                    mutable_cells.push(MutableTableCell {
                        origin_row: row,
                        origin_col: col,
                        in_header_section,
                        inlines: parse_inline(&content)?,
                        header,
                        align: alignments[col],
                        rowspan: 1,
                        colspan: 1,
                    });
                    occupancy[row][col] = Some(cell_id);
                }
            }
        }
    }

    validate_table_merges(
        lines,
        raw_rows,
        &mutable_cells,
        &occupancy,
        vertical_separator_cols,
    )?;

    let mut rows: Vec<TableRow> = (0..row_count)
        .map(|_| TableRow { cells: Vec::new() })
        .collect();
    for cell in mutable_cells {
        rows[cell.origin_row].cells.push(cell.into_cell());
    }

    Ok(rows)
}

fn validate_table_merges(
    lines: &[&str],
    raw_rows: &[RawTableRow],
    mutable_cells: &[MutableTableCell],
    occupancy: &[Vec<Option<usize>>],
    vertical_separator_cols: &[bool],
) -> Result<(), CompileError> {
    for (cell_id, cell) in mutable_cells.iter().enumerate() {
        for row in cell.origin_row..cell.origin_row + cell.rowspan {
            for col in cell.origin_col..cell.origin_col + cell.colspan {
                if vertical_separator_cols[col] || occupancy[row][col] != Some(cell_id) {
                    return Err(table_error(
                        lines,
                        raw_rows,
                        row,
                        "Table merge markers must form a rectangle",
                    ));
                }
            }
        }
    }
    Ok(())
}

fn table_error(
    lines: &[&str],
    raw_rows: &[RawTableRow],
    row: usize,
    message: &str,
) -> CompileError {
    CompileError {
        message: message.into(),
        offset: byte_offset_from_lines(lines, raw_rows[row].source_line, 0),
    }
}

fn unescape_table_cell(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.peek().copied() {
                if matches!(next, '|' | '>' | '<' | '^') {
                    out.push(next);
                    chars.next();
                    continue;
                }
            }
        }
        out.push(c);
    }
    out
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
            if let Some(close_idx) = find_unescaped_sequence(trimmed, 2, "$$") {
                let content = trimmed[2..close_idx].trim().to_string();
                blocks.push(Block::DisplayMath { content });
                let tail = trimmed[close_idx + 2..].trim();
                if !tail.is_empty() {
                    blocks.extend(parse_paragraph_text(tail)?);
                }
                i += 1;
                continue;
            }
        }
        if trimmed.starts_with("\\[") {
            if let Some(close_idx) = find_unescaped_sequence(trimmed, 2, "\\]") {
                let content = trimmed[2..close_idx].trim().to_string();
                blocks.push(Block::DisplayMath { content });
                let tail = trimmed[close_idx + 2..].trim();
                if !tail.is_empty() {
                    blocks.extend(parse_paragraph_text(tail)?);
                }
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
        if let Some((table_block, next)) = parse_table(lines, i)? {
            blocks.push(table_block);
            i = next;
            continue;
        }

        // Otherwise, treat as paragraph content at this item's body.
        let (paragraph_blocks, next) = parse_paragraph(lines, i)?;
        blocks.extend(paragraph_blocks);
        i = next;
    }

    Ok((ListItem { blocks }, i))
}

fn parse_paragraph(lines: &[&str], start: usize) -> Result<(Vec<Block>, usize), CompileError> {
    let mut parts = Vec::new();
    let mut i = start;
    while i < lines.len() {
        let line = lines[i];
        if line.trim().is_empty() || (i > start && is_block_start_at(lines, i)) {
            break;
        }
        parts.push(line);
        i += 1;
    }
    let text = parts.join("\n").trim().to_string();
    Ok((parse_paragraph_text(&text)?, i))
}

fn parse_paragraph_text(text: &str) -> Result<Vec<Block>, CompileError> {
    let mut blocks = Vec::new();
    let mut start = 0usize;

    while start < text.len() {
        let Some((open, close, after_close)) = find_display_math_span(text, start)? else {
            let tail = text[start..].trim();
            if !tail.is_empty() {
                blocks.push(Block::Paragraph {
                    inlines: parse_inline(tail)?,
                });
            }
            return Ok(blocks);
        };

        let before = text[start..open].trim();
        if !before.is_empty() {
            blocks.push(Block::Paragraph {
                inlines: parse_inline(before)?,
            });
        }
        blocks.push(Block::DisplayMath {
            content: text[open + 2..close].trim().to_string(),
        });
        start = after_close;
    }

    Ok(blocks)
}

fn find_display_math_span(
    text: &str,
    start: usize,
) -> Result<Option<(usize, usize, usize)>, CompileError> {
    let mut i = start;
    while i < text.len() {
        if text[i..].starts_with('`') && !is_escaped_at(text, i) {
            i = skip_backtick_span(text, i);
            continue;
        }
        if text[i..].starts_with("$$") && !is_escaped_at(text, i) {
            let close = find_unescaped_sequence(text, i + 2, "$$").ok_or_else(|| CompileError {
                message: "Unclosed $$ math".into(),
                offset: i,
            })?;
            return Ok(Some((i, close, close + 2)));
        }
        if text[i..].starts_with("\\[") && !is_escaped_at(text, i) {
            let close =
                find_unescaped_sequence(text, i + 2, "\\]").ok_or_else(|| CompileError {
                    message: "Unclosed \\[ math".into(),
                    offset: i,
                })?;
            return Ok(Some((i, close, close + 2)));
        }
        i += text[i..].chars().next().map(char::len_utf8).unwrap_or(1);
    }
    Ok(None)
}

fn skip_backtick_span(text: &str, start: usize) -> usize {
    let run = text[start..].chars().take_while(|c| *c == '`').count();
    if run == 0 {
        return start + 1;
    }
    let fence = "`".repeat(run);
    let content_start = start + run;
    find_unescaped_sequence(text, content_start, &fence)
        .map(|close| close + run)
        .unwrap_or(content_start)
}

fn is_block_start_at(lines: &[&str], idx: usize) -> bool {
    is_block_start_line(lines[idx]) || find_table_delimiter(lines, idx).is_some()
}

fn is_block_start_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed.starts_with("```")
        || trimmed == ":::html"
        || trimmed.starts_with("$$")
        || trimmed.starts_with("\\[")
        || parse_heading(trimmed).is_some()
        || is_list_marker(line.trim_start())
}
