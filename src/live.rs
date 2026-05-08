//! Stateful live renderer for interactive editors.
//!
//! The live renderer still pre-lexes and expands the whole document, because
//! region boundaries and macro behavior are document-wide concerns. It avoids
//! the heavier repeated work by splitting the expanded source into top-level
//! chunks, caching parsed AST blocks for unchanged chunks, and re-emitting the
//! full block list so global output such as footnotes stays correct.

use crate::ast::Block;
use crate::block::parse_blocks;
use crate::emit::Emitter;
use crate::macros::expand_macros;
use crate::prelex::{pre_lex, CompileError};
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LiveRenderStats {
    pub render_count: u64,
    pub source_bytes: usize,
    pub expanded_bytes: usize,
    pub chunks: usize,
    pub parsed_chunks: usize,
    pub reused_chunks: usize,
    pub cache_entries: usize,
    pub html_bytes: usize,
}

impl LiveRenderStats {
    fn as_json(self) -> String {
        format!(
            "{{\"renderCount\":{},\"sourceBytes\":{},\"expandedBytes\":{},\"chunks\":{},\"parsedChunks\":{},\"reusedChunks\":{},\"cacheEntries\":{},\"htmlBytes\":{}}}",
            self.render_count,
            self.source_bytes,
            self.expanded_bytes,
            self.chunks,
            self.parsed_chunks,
            self.reused_chunks,
            self.cache_entries,
            self.html_bytes
        )
    }
}

#[derive(Debug, Default)]
pub struct LiveCompilerInner {
    cache: HashMap<String, Vec<Block>>,
    stats: LiveRenderStats,
    render_count: u64,
}

impl LiveCompilerInner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn render(&mut self, source: &str) -> Result<String, CompileError> {
        let spans = pre_lex(source)?;
        let (expanded, _adjusted_spans) = expand_macros(source, &spans);
        let (frontmatter, body) = split_frontmatter(&expanded)?;
        let chunks = split_live_chunks(body);

        let mut next_cache = HashMap::with_capacity(chunks.len());
        let mut blocks = Vec::new();
        let mut parsed_chunks = 0usize;
        let mut reused_chunks = 0usize;

        for chunk in chunks.iter() {
            let cached = self.cache.get(chunk).or_else(|| next_cache.get(chunk));
            let chunk_blocks = if let Some(cached_blocks) = cached {
                reused_chunks += 1;
                cached_blocks.clone()
            } else {
                let (_chunk_frontmatter, parsed) = parse_blocks(chunk)?;
                parsed_chunks += 1;
                parsed
            };
            blocks.extend(chunk_blocks.iter().cloned());
            next_cache.insert(chunk.clone(), chunk_blocks);
        }

        let mut emitter = Emitter::new();
        let html = emitter.emit_html(&blocks, frontmatter.as_deref());

        self.cache = next_cache;
        self.render_count += 1;
        self.stats = LiveRenderStats {
            render_count: self.render_count,
            source_bytes: source.len(),
            expanded_bytes: expanded.len(),
            chunks: chunks.len(),
            parsed_chunks,
            reused_chunks,
            cache_entries: self.cache.len(),
            html_bytes: html.len(),
        };

        Ok(html)
    }

    pub fn stats(&self) -> LiveRenderStats {
        self.stats
    }

    pub fn stats_json(&self) -> String {
        self.stats.as_json()
    }

    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.stats = LiveRenderStats::default();
        self.render_count = 0;
    }
}

#[wasm_bindgen]
pub struct LiveCompiler {
    inner: LiveCompilerInner,
}

#[wasm_bindgen]
impl LiveCompiler {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            inner: LiveCompilerInner::new(),
        }
    }

    pub fn render(&mut self, source: &str) -> Result<String, JsValue> {
        self.inner
            .render(source)
            .map_err(|e| JsValue::from_str(&format!("{} at offset {}", e.message, e.offset)))
    }

    pub fn stats_json(&self) -> String {
        self.inner.stats_json()
    }

    pub fn clear_cache(&mut self) {
        self.inner.clear_cache();
    }
}

impl Default for LiveCompiler {
    fn default() -> Self {
        Self::new()
    }
}

fn split_frontmatter(source: &str) -> Result<(Option<String>, &str), CompileError> {
    let lines: Vec<&str> = source.lines().collect();
    if lines.first().map(|line| line.trim()) != Some("---") {
        return Ok((None, source));
    }

    let mut i = 1usize;
    while i < lines.len() && lines[i].trim() != "---" {
        i += 1;
    }
    if i >= lines.len() {
        return Err(CompileError {
            message: "Unclosed frontmatter".into(),
            offset: 0,
        });
    }

    let frontmatter = lines[1..i].join("\n");
    let mut body_start = 0usize;
    for line in lines.iter().take(i + 1) {
        body_start += line.len();
        if body_start < source.len() {
            body_start += 1;
        }
    }

    Ok((Some(frontmatter), &source[body_start.min(source.len())..]))
}

fn split_live_chunks(source: &str) -> Vec<String> {
    let lines: Vec<&str> = source.lines().collect();
    let mut chunks = Vec::new();
    let mut i = 0usize;

    while i < lines.len() {
        if lines[i].trim().is_empty() {
            i += 1;
            continue;
        }

        let start = i;
        let trimmed = lines[i].trim();
        let indent = leading_spaces(lines[i]);

        if trimmed.starts_with("```") {
            i = consume_code_fence(&lines, i, indent);
        } else if trimmed == ":::html" {
            i = consume_html_fence(&lines, i);
        } else if starts_multiline_display_math(trimmed) {
            i = consume_display_math(&lines, i, trimmed.starts_with("\\["));
        } else if is_heading(trimmed) {
            i += 1;
        } else if is_list_marker(lines[i].trim_start()) {
            i = consume_list(&lines, i, indent);
        } else {
            i = consume_paragraph(&lines, i);
        }

        chunks.push(lines[start..i].join("\n"));
    }

    chunks
}

fn consume_code_fence(lines: &[&str], start: usize, indent: usize) -> usize {
    let mut i = start + 1;
    while i < lines.len() {
        let line_indent = leading_spaces(lines[i]);
        if line_indent == indent && lines[i].trim_start().starts_with("```") {
            return i + 1;
        }
        i += 1;
    }
    lines.len()
}

fn consume_html_fence(lines: &[&str], start: usize) -> usize {
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim() == ":::" {
            return i + 1;
        }
        i += 1;
    }
    lines.len()
}

fn starts_multiline_display_math(trimmed: &str) -> bool {
    if trimmed.starts_with("\\[") {
        return true;
    }
    trimmed.starts_with("$$") && trimmed[2..].find("$$").is_none()
}

fn consume_display_math(lines: &[&str], start: usize, is_bracket: bool) -> usize {
    let mut i = start + 1;
    while i < lines.len() {
        let trimmed = lines[i].trim();
        if (is_bracket && trimmed == "\\]") || (!is_bracket && trimmed == "$$") {
            return i + 1;
        }
        i += 1;
    }
    lines.len()
}

fn consume_list(lines: &[&str], start: usize, base_indent: usize) -> usize {
    let mut i = start + 1;
    while i < lines.len() {
        if lines[i].trim().is_empty() {
            break;
        }
        let indent = leading_spaces(lines[i]);
        if indent < base_indent {
            break;
        }
        if indent == base_indent && !is_list_marker(lines[i].trim_start()) {
            break;
        }
        i += 1;
    }
    i
}

fn consume_paragraph(lines: &[&str], start: usize) -> usize {
    let mut i = start + 1;
    while i < lines.len() && !lines[i].trim().is_empty() {
        i += 1;
    }
    i
}

fn is_heading(trimmed: &str) -> bool {
    trimmed.starts_with('#')
}

fn is_list_marker(trimmed: &str) -> bool {
    trimmed.starts_with("- ") || trimmed.starts_with("-[") || trimmed.starts_with("=[")
}

fn leading_spaces(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compile_inner;

    #[test]
    fn live_render_matches_full_compile() {
        let source = r#"---
title: Live
---

#[1] Title

This is **bold** with a footnote^[Source: [doc](https://example.com/doc)].

- first
- second
    =[1] nested

$$
x^2 + y^2 = z^2
$$
"#;
        let expected = compile_inner(source).expect("full compile");
        let mut compiler = LiveCompilerInner::new();
        let actual = compiler.render(source).expect("live render");
        assert_eq!(actual, expected);
    }

    #[test]
    fn live_render_reuses_unchanged_chunks() {
        let source = "#[1] One\n\nFirst paragraph.\n\nSecond paragraph.";
        let edited = "#[1] One\n\nFirst paragraph changed.\n\nSecond paragraph.";
        let mut compiler = LiveCompilerInner::new();
        compiler.render(source).expect("first render");
        compiler.render(edited).expect("second render");
        let stats = compiler.stats();
        assert_eq!(stats.chunks, 3);
        assert_eq!(stats.parsed_chunks, 1);
        assert_eq!(stats.reused_chunks, 2);
    }
}
