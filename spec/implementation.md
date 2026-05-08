```
Source text
    │
    ▼
┌─────────────┐
│  Pre-lexer  │  Identifies math/code region boundaries only.
│             │  Output: source text annotated with region types.
└─────────────┘
    │
    ▼
┌──────────────────┐
│  Macro expander  │  Textual substitution, top-to-bottom.
│                  │  Skips annotated math/code regions.
└──────────────────┘
    │
    ▼
┌──────────────────┐
│   Block parser   │  Consumes lines. Produces block-level AST nodes.
│                  │  Handles indentation, fences, headings, lists.
│                  │  Math/code blocks are opaque leaf nodes at this stage.
└──────────────────┘
    │
    ▼
┌───────────────────┐
│   Inline parser   │  Runs over paragraph text nodes only.
│                   │  Left-to-right, stack-based.
│                   │  Produces inline AST nodes.
└───────────────────┘
    │
    ▼
┌────────────────────┐
│   Math renderer    │  Walks AST, finds math leaf nodes, calls WASM.
│                    │  Replaces opaque nodes with rendered output.
└────────────────────┘
    │
    ▼
┌──────────────────┐
│   HTML emitter   │  Walks final AST, emits HTML string.
│                  │  Resolves footnote numbering, heading IDs, labels.
└──────────────────┘
    │
    ▼
HTML output
```

# Pre-lexer

Single forward scan over the raw source characters. Maintains a `mode` state variable with values `{normal, math_inline, math_display, code_inline, code_display}`. On each character, checks for opening/closing delimiters and transitions mode. Outputs a flat list of annotated spans:

```text
Span { start: usize, end: usize, kind: SpanKind }
```

where `SpanKind ∈ { Normal, MathInline, MathDisplay, CodeInline, CodeDisplay }`.

Key rules:
- Code delimiters take priority over math delimiters (checked first).
- Transitions are only valid from `normal` mode — no nesting.
- Unmatched delimiters -> immediate compile error with position.

---

# Macro Expander

Iterates over the span list from the pre-lexer. For `Normal` spans only, performs a single top-to-bottom linear scan, maintaining a macro table built up as macros are encountered. Each macro definition is consumed and added to the table; each macro invocation is replaced with its expansion text. Non-normal spans are passed through verbatim.

Key rules:
- Macro table is append-only and ordered.
- A macro may only reference macros defined strictly before it.
- Expansion is single-depth textual substitution — no re-scanning of expanded text.
- Output is a new source string with the same span annotations adjusted for any length changes.

---

# Block Parser

Consumes the post-expansion source line by line. Maintains an indentation stack. Produces a tree of block nodes.

```text
Block =
  | Heading { level, id, text }
  | Paragraph { inlines }
  | List { ordered, style, items: [ListItem] }
  | ListItem { depth, blocks: [Block] }
  | CodeBlock { lang, content }
  | DisplayMath { content }
  | HtmlBlock { content }
  | FrontMatter { content }
```

Algorithm:
1. If first line is `---`, consume until closing `---` as frontmatter.
2. For each line, determine its indent level against the indent stack.
3. Dispatch to sub-parsers based on the line's leading token (`#`, `-`, ` ``` `, `$$`, `\[`, or plain text).
4. Paragraph accumulates lines until a blank line or block-level token is encountered.
5. List items push/pop the indent stack as depth changes.
6. Code and math blocks are consumed as opaque content — no further parsing inside them at this stage.

---

# Inline Parser

Runs over the `text` field of `Paragraph` nodes (and heading text). Left-to-right character scan with an explicit stack.

```text
Inline =
  | Text { content }
  | Bold { children: [Inline] }
  | Italic { children: [Inline] }
  | Code { content }
  | InlineMath { content }
  | Link { text, href }
  | Footnote { note, href }
  | Superscript { content }
  | Subscript { content }
```

Algorithm:
1. Scan left to right, tokenizing delimiters (`**`, `*`, `` ` ``, `$`, `\(`, `[`, `^{`, `_{`).
2. On opening token, push frame onto stack: `Frame { kind, start_pos }`.
3. On closing token, pop matching frame and emit inline node.
4. If closing token has no matching open frame -> compile error.
5. If end of input with non-empty stack -> compile error.
6. On code or math open token, enter suppression mode: consume raw characters until matching close, emit opaque inline node, exit suppression mode.

---

# Math Renderer

Walks the AST looking for `DisplayMath`, `InlineMath`, and `CodeBlock` nodes. For math nodes, calls the WASM renderer synchronously with the raw content string. Replaces the node in-place with a `RenderedMath { html: string }` node. Errors from the WASM renderer propagate as compile errors.

---

# HTML Emitter

Single recursive AST walk. Maintains two global tables built during the walk:
- `heading_table: Map<id, slug>` — populated on `Heading` nodes.
- `footnote_table: [ { note, href } ]` — populated in order of encounter on `Footnote` nodes.

Emission rules are straightforward node-to-tag mappings. After the full walk, appends the bibliography as an ordered list and the frontmatter metadata as appropriate `<meta>` tags.