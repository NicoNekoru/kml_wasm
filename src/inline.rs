//! Inline parser: left-to-right over paragraph/heading text.
//! Bold, italic, code, link, inline math, footnote, sup, sub. Suppression inside code/math.

use crate::ast::Inline;
use crate::escape::{find_unescaped_sequence, is_escaped_at};
use crate::prelex::CompileError;
use std::result::Result;

pub fn parse_inline(text: &str) -> Result<Vec<Inline>, CompileError> {
    let mut result = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = text.chars().collect();
    let char_count = chars.len();

    while i < char_count {
        let rest = &text[char_offset_to_byte(text, i)..];

        if rest.starts_with("<br>") {
            result.push(Inline::LineBreak);
            i += 4; // <br> is 4 chars
            continue;
        }
        if chars[i] == '`' && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            let close_char = find_closing_backticks(text, i).ok_or_else(|| CompileError {
                message: "Unclosed inline code".into(),
                offset: 0,
            })?;
            let content = text
                [char_offset_to_byte(text, i + 1)..char_offset_to_byte(text, close_char)]
                .replace('`', "");
            result.push(Inline::Code { content });
            i = close_char + 1;
            continue;
        }
        if chars[i] == '$'
            && (i + 1 >= char_count || chars[i + 1] != '$')
            && !is_escaped_at(text, char_offset_to_byte(text, i))
        {
            let close = find_dollar_close(text, i);
            let close_char = close.ok_or_else(|| CompileError {
                message: "Unclosed inline math".into(),
                offset: 0,
            })?;
            let content = text
                [char_offset_to_byte(text, i + 1)..char_offset_to_byte(text, close_char)]
                .trim()
                .to_string();
            result.push(Inline::InlineMath { content });
            i = close_char + 1;
            continue;
        }
        if rest.starts_with("\\(") && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            let open_byte = char_offset_to_byte(text, i);
            let close_byte =
                find_unescaped_sequence(text, open_byte + 2, "\\)").ok_or_else(|| {
                    CompileError {
                        message: "Unclosed \\( math".into(),
                        offset: 0,
                    }
                })?;
            let content = text[open_byte + 2..close_byte].trim().to_string();
            let consumed_chars = text[open_byte..close_byte + 2].chars().count();
            result.push(Inline::InlineMath { content });
            i += consumed_chars;
            continue;
        }
        if chars[i] == '\\' && i + 1 < char_count && is_escapable_inline_char(chars[i + 1]) {
            result.push(Inline::Text {
                content: chars[i + 1].to_string(),
            });
            i += 2;
            continue;
        }
        if rest.starts_with("**") && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            let inner_start_byte = char_offset_to_byte(text, i + 2);
            let close_char = find_closing_double_star(text, i + 2);
            let close_char = close_char.ok_or_else(|| CompileError {
                message: "Unclosed **".into(),
                offset: 0,
            })?;
            let inner =
                parse_inline(&text[inner_start_byte..char_offset_to_byte(text, close_char)])?;
            result.push(Inline::Bold { children: inner });
            i = close_char + 2;
            continue;
        }
        if chars[i] == '*'
            && (i + 1 >= char_count || chars[i + 1] != '*')
            && !is_escaped_at(text, char_offset_to_byte(text, i))
        {
            let close = find_closing_star(text, i + 1);
            let close_char = close.ok_or_else(|| CompileError {
                message: "Unclosed *".into(),
                offset: 0,
            })?;
            let inner = parse_inline(
                &text[char_offset_to_byte(text, i + 1)..char_offset_to_byte(text, close_char)],
            )?;
            result.push(Inline::Italic { children: inner });
            i = close_char + 1;
            continue;
        }
        if rest.starts_with("^[") && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            if let Some((note, href, consumed)) = parse_footnote_impl(text, i) {
                result.push(Inline::Footnote { note, href });
                i += consumed;
                continue;
            }
        }
        if chars[i] == '[' && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            if let Some((link_text, href, consumed)) = parse_link(text, i) {
                result.push(Inline::Link {
                    text: link_text,
                    href,
                });
                i += consumed;
                continue;
            }
        }
        if rest.starts_with("^{") && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            let end = find_balanced_braces(text, i);
            let end_char = end.ok_or_else(|| CompileError {
                message: "Unclosed ^{".into(),
                offset: 0,
            })?;
            let content = text
                [char_offset_to_byte(text, i + 2)..char_offset_to_byte(text, end_char)]
                .to_string();
            result.push(Inline::Superscript { content });
            i = end_char + 1;
            continue;
        }
        if rest.starts_with("_{") && !is_escaped_at(text, char_offset_to_byte(text, i)) {
            let end = find_balanced_braces(text, i);
            let end_char = end.ok_or_else(|| CompileError {
                message: "Unclosed _{".into(),
                offset: 0,
            })?;
            let content = text
                [char_offset_to_byte(text, i + 2)..char_offset_to_byte(text, end_char)]
                .to_string();
            result.push(Inline::Subscript { content });
            i = end_char + 1;
            continue;
        }
        let j = next_inline_delimiter(text, i);
        let j = std::cmp::max(j, i + 1);
        let text_content =
            text[char_offset_to_byte(text, i)..char_offset_to_byte(text, j)].to_string();
        result.push(Inline::Text {
            content: text_content,
        });
        i = j;
    }

    Ok(result)
}

fn char_offset_to_byte(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

fn is_escapable_inline_char(c: char) -> bool {
    matches!(
        c,
        '\\' | '$' | '*' | '`' | '[' | ']' | '^' | '_' | '{' | '}' | '(' | ')' | '<' | '>' | '|'
    )
}

fn find_closing_backticks(text: &str, start_char: usize) -> Option<usize> {
    let start_byte = char_offset_to_byte(text, start_char);
    let after = &text[start_byte + 1..];
    let mut char_idx = start_char + 1;
    for (byte_offset, c) in after.char_indices() {
        let byte_idx = start_byte + 1 + byte_offset;
        if c == '`' && !is_escaped_at(text, byte_idx) {
            return Some(char_idx);
        }
        char_idx += 1;
    }
    None
}

fn find_dollar_close(text: &str, start_char: usize) -> Option<usize> {
    let start_byte = char_offset_to_byte(text, start_char);
    let after = &text[start_byte + 1..];
    let mut char_idx = start_char + 1;
    for (byte_offset, c) in after.char_indices() {
        let byte_idx = start_byte + 1 + byte_offset;
        if c == '$' && !is_escaped_at(text, byte_idx) {
            return Some(char_idx);
        }
        char_idx += 1;
    }
    None
}

fn find_closing_star(text: &str, start_char: usize) -> Option<usize> {
    let total_chars = text.chars().count();
    let mut char_idx = start_char;
    while char_idx < total_chars {
        let byte_idx = char_offset_to_byte(text, char_idx);
        let rest = &text[byte_idx..];
        if rest.starts_with("**") && !is_escaped_at(text, byte_idx) {
            char_idx += 2;
            continue;
        }
        if rest.starts_with('*') && !is_escaped_at(text, byte_idx) {
            return Some(char_idx);
        }
        char_idx += 1;
    }
    None
}

fn find_closing_double_star(text: &str, start_char: usize) -> Option<usize> {
    for char_idx in start_char..text.chars().count() {
        let byte_idx = char_offset_to_byte(text, char_idx);
        let rest = &text[byte_idx..];
        if rest.starts_with("**") && !is_escaped_at(text, byte_idx) {
            return Some(char_idx);
        }
    }
    None
}

fn find_balanced_braces(text: &str, start_char: usize) -> Option<usize> {
    let start_byte = char_offset_to_byte(text, start_char + 2);
    let mut depth = 1u32;
    let mut i = start_byte;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if i + 1 < bytes.len()
            && bytes[i] == b'\\'
            && (bytes[i + 1] == b'{' || bytes[i + 1] == b'}')
        {
            i += 2;
            continue;
        }
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 {
                return Some(start_char + 2 + text[start_byte..i].chars().count());
            }
        }
        i += 1;
    }
    None
}

fn parse_link(text: &str, start_char: usize) -> Option<(String, String, usize)> {
    let start_byte = char_offset_to_byte(text, start_char);
    let rest = &text[start_byte..];
    if !rest.starts_with('[') {
        return None;
    }
    let close_bracket = find_unescaped_sequence(text, start_byte + 1, "]")? - start_byte;
    let text_content = rest[1..close_bracket].to_string();
    let after = &rest[close_bracket..];
    if !after.starts_with("](") {
        return None;
    }
    let paren_start = 2usize;
    let mut paren_depth = 1u32;
    let mut i = paren_start;
    let bytes = after.as_bytes();
    while i < bytes.len() {
        let absolute_i = start_byte + close_bracket + i;
        if bytes[i] == b'(' && !is_escaped_at(text, absolute_i) {
            paren_depth += 1;
        } else if bytes[i] == b')' && !is_escaped_at(text, absolute_i) {
            paren_depth -= 1;
            if paren_depth == 0 {
                let href = after[paren_start..i].to_string();
                let total_byte_len = close_bracket + i + 1;
                let total_char_count = rest[..total_byte_len].chars().count();
                return Some((text_content, href, total_char_count));
            }
        }
        i += 1;
    }
    None
}

pub(crate) fn parse_footnote_impl(
    text: &str,
    start_char: usize,
) -> Option<(String, String, usize)> {
    let start_byte = char_offset_to_byte(text, start_char);
    let rest = &text[start_byte..];
    if !rest.starts_with("^[") {
        return None;
    }
    // Find the ] that matches the [ after ^, so the note can contain "](url)" (e.g. links).
    let mut depth = 1u32;
    let mut close_bracket_byte = None;
    let bytes = rest.as_bytes();
    for pos in 2..bytes.len() {
        let b = bytes[pos];
        let absolute_pos = start_byte + pos;
        if b == b'[' && !is_escaped_at(text, absolute_pos) {
            depth += 1;
        } else if b == b']' && !is_escaped_at(text, absolute_pos) {
            depth -= 1;
            if depth == 0 {
                close_bracket_byte = Some(pos);
                break;
            }
        }
    }
    let close_bracket = close_bracket_byte?;
    let note = rest[2..close_bracket].to_string();
    let after = &rest[close_bracket..];
    let (href, total_byte_len) = if after.starts_with("](") {
        let mut paren_depth = 1u32;
        let mut close_paren_byte = None;
        let after_url = &after[2..];
        for (pos, b) in after_url.bytes().enumerate() {
            let absolute_pos = start_byte + close_bracket + 2 + pos;
            if b == b'(' && !is_escaped_at(text, absolute_pos) {
                paren_depth += 1;
            } else if b == b')' && !is_escaped_at(text, absolute_pos) {
                paren_depth -= 1;
                if paren_depth == 0 {
                    close_paren_byte = Some(pos);
                    break;
                }
            }
        }
        let close_paren = close_paren_byte?;
        let href = after[2..2 + close_paren].to_string();
        let len = close_bracket + 2 + close_paren + 1;
        (href, len)
    } else {
        let href = extract_first_link_url(&note).unwrap_or_default();
        (href, close_bracket + 1)
    };
    let total_char_count = rest[..total_byte_len].chars().count();
    Some((note, href, total_char_count))
}

fn extract_first_link_url(note: &str) -> Option<String> {
    let start = note.find('[')?;
    let rest = &note[start..];
    let close = rest.find(']')?;
    let after = &rest[close..];
    if !after.starts_with("](") {
        return None;
    }
    let mut depth = 1u32;
    for (i, b) in after[2..].bytes().enumerate() {
        if b == b'(' {
            depth += 1;
        } else if b == b')' {
            depth -= 1;
            if depth == 0 {
                return Some(after[2..2 + i].to_string());
            }
        }
    }
    None
}

fn next_inline_delimiter(text: &str, start_char: usize) -> usize {
    let total_chars = text.chars().count();
    for char_idx in start_char..total_chars {
        let byte_start = char_offset_to_byte(text, char_idx);
        let rest = &text[byte_start..];
        if let Some(c) = rest.chars().next() {
            if c == '\\' {
                if let Some(next) = rest.chars().nth(1) {
                    if is_escapable_inline_char(next) {
                        return char_idx;
                    }
                }
            }
        }
        if rest.starts_with("<br>")
            || rest.starts_with("**")
            || rest.starts_with("\\(")
            || rest.starts_with("^{")
            || rest.starts_with("_{")
        {
            if !is_escaped_at(text, byte_start) {
                return char_idx;
            }
        }
        if let Some(c) = rest.chars().next() {
            if (c == '`' || c == '[') && !is_escaped_at(text, byte_start) {
                return char_idx;
            }
            if c == '^' && rest.chars().nth(1) == Some('[') && !is_escaped_at(text, byte_start) {
                return char_idx;
            }
            if c == '$' && rest.chars().nth(1) != Some('$') {
                if !is_escaped_at(text, byte_start) {
                    return char_idx;
                }
            }
            if c == '*' && rest.chars().nth(1) != Some('*') && !is_escaped_at(text, byte_start) {
                return char_idx;
            }
        }
    }
    total_chars
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_footnote_nested_link() {
        let text = "A claim^[Source: [doc](https://example.com/doc)].";
        let r = parse_footnote_impl(text, 7);
        assert!(r.is_some(), "parse_footnote should succeed");
        let (note, href, consumed) = r.unwrap();
        assert_eq!(
            note, "Source: [doc](https://example.com/doc)",
            "note should contain nested link"
        );
        assert_eq!(
            href, "https://example.com/doc",
            "href extracted from link in note"
        );
        assert!(consumed > 0);
    }
}
