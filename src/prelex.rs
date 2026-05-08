//! Pre-lexer: single forward scan to identify math/code region boundaries.
//! Code delimiters take priority over math. No nesting.
//! All span positions are byte offsets.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpanKind {
    Normal,
    MathInline,
    MathDisplay,
    CodeInline,
    CodeDisplay,
}

#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: SpanKind,
}

#[derive(Debug)]
pub struct CompileError {
    pub message: String,
    pub offset: usize,
}

fn count_run_backticks(s: &str, byte_start: usize) -> usize {
    let mut count = 0;
    for (_, c) in s[byte_start..].char_indices() {
        if c == '`' {
            count += 1;
        } else {
            break;
        }
    }
    count
}

fn byte_len_backticks(s: &str, byte_start: usize, n: usize) -> usize {
    let mut taken = 0;
    let mut bytes = 0;
    for (i, c) in s[byte_start..].char_indices() {
        if taken >= n {
            break;
        }
        if c == '`' {
            taken += 1;
            bytes = i + c.len_utf8();
        } else {
            break;
        }
    }
    bytes
}

fn peek_non_space(s: &str, byte_start: usize) -> Option<char> {
    s[byte_start..].chars().find(|c| !c.is_whitespace())
}

pub fn pre_lex(source: &str) -> Result<Vec<Span>, CompileError> {
    let mut spans = Vec::new();
    let mut i = 0u32 as usize;
    let mut mode = Mode::Normal;
    let mut span_start = 0;
    let mut code_inline_len = 1usize;
    let n = source.len();

    fn do_start_span(spans: &mut Vec<Span>, span_start: &mut usize, idx: usize) {
        if *span_start < idx {
            spans.push(Span {
                start: *span_start,
                end: idx,
                kind: SpanKind::Normal,
            });
        }
        *span_start = idx;
    }
    fn do_end_span(
        spans: &mut Vec<Span>,
        span_start: &mut usize,
        mode: &mut Mode,
        kind: SpanKind,
        idx: usize,
    ) {
        spans.push(Span {
            start: *span_start,
            end: idx,
            kind,
        });
        *span_start = idx;
        *mode = Mode::Normal;
    }

    while i < n {
        let rest = &source[i..];
        let c = match rest.chars().next() {
            Some(ch) => ch,
            None => break,
        };
        let c_len = c.len_utf8();

        match mode {
            Mode::CodeInline => {
                if c == '`' {
                    let backticks = count_run_backticks(source, i);
                    if backticks >= code_inline_len {
                        let step_full = byte_len_backticks(source, i, backticks);
                        do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::CodeInline, i);
                        // For ``` always advance past all 3, else we'd treat code-block fence as opening CodeInline.
                        // For `` only skip advance when adjacent `` could open next span (`` `code` with `` backticks ``).
                        let advance_by = if backticks >= 3 {
                            step_full
                        } else {
                            let at_end = i + step_full >= n;
                            if at_end {
                                step_full
                            } else {
                                let remainder = &source[i + step_full..];
                                let next_pos = remainder.char_indices().find(|(_, c)| *c == '`').map(|(bo, _)| i + step_full + bo);
                                let has_adjacent_double = next_pos
                                    .map(|p| count_run_backticks(source, p) == 2)
                                    .unwrap_or(false);
                                if has_adjacent_double {
                                    0
                                } else {
                                    step_full
                                }
                            }
                        };
                        i += advance_by;
                        continue;
                    }
                }
                i += c_len;
            }
            Mode::CodeDisplay => {
                if rest.starts_with("```") {
                    do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::CodeDisplay, i + 3);
                    i += 3;
                    continue;
                }
                i += c_len;
            }
            Mode::MathInline => {
                if rest.starts_with("\\)") {
                    do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::MathInline, i + 2);
                    i += 2;
                    continue;
                }
                if c == '$' && (i == 0 || source[..i].chars().last() != Some('\\')) {
                    let next_pos = i + c_len;
                    if next_pos < n {
                        let next_ch = peek_non_space(source, next_pos);
                        if next_ch != Some('$') {
                            do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::MathInline, i + c_len);
                            i += c_len;
                            continue;
                        }
                    } else {
                        do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::MathInline, i + c_len);
                        i += c_len;
                        continue;
                    }
                }
                i += c_len;
            }
            Mode::MathDisplay => {
                if rest.starts_with("\\]") {
                    do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::MathDisplay, i + 2);
                    i += 2;
                    continue;
                }
                if rest.starts_with("$$") {
                    do_end_span(&mut spans, &mut span_start, &mut mode, SpanKind::MathDisplay, i + 2);
                    i += 2;
                    continue;
                }
                i += c_len;
            }
            Mode::Normal => {
                if rest.starts_with("```") {
                    do_start_span(&mut spans, &mut span_start, i);
                    i += 3;
                    mode = Mode::CodeDisplay;
                    continue;
                }
                if c == '`' {
                    let backticks = count_run_backticks(source, i);
                    if backticks >= 1 {
                        code_inline_len = if backticks >= 3 { 3 } else { backticks };
                        if code_inline_len == 3 {
                            do_start_span(&mut spans, &mut span_start, i);
                            i += 3;
                            mode = Mode::CodeDisplay;
                        } else {
                            let step = byte_len_backticks(source, i, code_inline_len);
                            do_start_span(&mut spans, &mut span_start, i);
                            i += step;
                            mode = Mode::CodeInline;
                        }
                        continue;
                    }
                }
                if rest.starts_with("\\(") {
                    do_start_span(&mut spans, &mut span_start, i);
                    i += 2;
                    mode = Mode::MathInline;
                    continue;
                }
                if rest.starts_with("\\[") {
                    do_start_span(&mut spans, &mut span_start, i);
                    i += 2;
                    mode = Mode::MathDisplay;
                    continue;
                }
                if c == '$' {
                    if rest.starts_with("$$") {
                        do_start_span(&mut spans, &mut span_start, i);
                        i += 2;
                        mode = Mode::MathDisplay;
                    } else {
                        do_start_span(&mut spans, &mut span_start, i);
                        i += c_len;
                        mode = Mode::MathInline;
                    }
                    continue;
                }
                i += c_len;
            }
        }
    }

    if mode != Mode::Normal {
        return Err(CompileError {
            message: format!("Unclosed {:?} region", mode),
            offset: span_start,
        });
    }
    if span_start < n {
        spans.push(Span {
            start: span_start,
            end: n,
            kind: SpanKind::Normal,
        });
    }
    Ok(spans)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    MathInline,
    MathDisplay,
    CodeInline,
    CodeDisplay,
}
