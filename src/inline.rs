//! Inline parser: left-to-right over paragraph/heading text.
//! Bold, italic, code, link, inline math, footnote, sup, sub. Suppression inside code/math.

use crate::ast::Inline;
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
        if chars[i] == '`' {
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
        if rest.starts_with("$$") {
            let close_byte = rest[2..].find("$$").ok_or_else(|| CompileError {
                message: "Unclosed $$ math".into(),
                offset: 0,
            })?;
            let content = rest[2..2 + close_byte].trim().to_string();
            let consumed_chars = rest[..2 + close_byte + 2].chars().count();
            result.push(Inline::InlineMath { content });
            i += consumed_chars;
            continue;
        }
        if chars[i] == '$' && (i + 1 >= char_count || chars[i + 1] != '$') {
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
        if rest.starts_with("\\(") {
            let close_byte = rest.find("\\)").ok_or_else(|| CompileError {
                message: "Unclosed \\( math".into(),
                offset: 0,
            })?;
            let content = rest[2..close_byte].trim().to_string();
            let consumed_chars = rest[..close_byte + 2].chars().count();
            result.push(Inline::InlineMath { content });
            i += consumed_chars;
            continue;
        }
        if rest.starts_with("**") {
            let after_open = &rest[2..];
            let inner_start_byte = char_offset_to_byte(text, i + 2);
            let close_char = after_open
                .find("**")
                .map(|cb| i + 2 + after_open[..cb].chars().count());
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
        if chars[i] == '*' && (i + 1 >= char_count || chars[i + 1] != '*') {
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
        if rest.starts_with("^[") {
            if let Some((note, href, consumed)) = parse_footnote_impl(text, i) {
                result.push(Inline::Footnote { note, href });
                i += consumed;
                continue;
            }
        }
        if chars[i] == '[' {
            if let Some((link_text, href, consumed)) = parse_link(text, i) {
                result.push(Inline::Link {
                    text: link_text,
                    href,
                });
                i += consumed;
                continue;
            }
        }
        if rest.starts_with("^{") {
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
        if rest.starts_with("_{") {
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

fn find_closing_backticks(text: &str, start_char: usize) -> Option<usize> {
    let start_byte = char_offset_to_byte(text, start_char);
    let after = &text[start_byte + 1..];
    let mut char_idx = start_char + 1;
    for (_, c) in after.char_indices() {
        if c == '`' {
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
    for (_, c) in after.char_indices() {
        if c == '$' {
            return Some(char_idx);
        }
        char_idx += 1;
    }
    None
}

fn find_closing_star(text: &str, start_char: usize) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    for i in start_char..chars.len() {
        if chars[i] == '*' && (i == 0 || chars[i - 1] != '*') {
            return Some(i);
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
    let close_bracket = rest.find(']')?;
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
        if bytes[i] == b'(' {
            paren_depth += 1;
        } else if bytes[i] == b')' {
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
        if b == b'[' {
            depth += 1;
        } else if b == b']' {
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
            if b == b'(' {
                paren_depth += 1;
            } else if b == b')' {
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
        if rest.starts_with("<br>")
            || rest.starts_with("**")
            || rest.starts_with("$$")
            || rest.starts_with("\\(")
            || rest.starts_with("^{")
            || rest.starts_with("_{")
        {
            return char_idx;
        }
        if let Some(c) = rest.chars().next() {
            if c == '`' || c == '[' {
                return char_idx;
            }
            if c == '^' && rest.chars().nth(1) == Some('[') {
                return char_idx;
            }
            if c == '$' && rest.chars().nth(1) != Some('$') {
                if char_idx == 0 || text[..byte_start].chars().last() != Some('\\') {
                    return char_idx;
                }
            }
            if c == '*' && rest.chars().nth(1) != Some('*') {
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
