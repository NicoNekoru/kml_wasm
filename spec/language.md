# Intro
We are designing a markup language that is designed for technical blogposts which will eventually be compiled into html. Technical blogposts have some minimal kinds of actual rich text formatting -- code, bold, italics, lists, hyperlinks, and footnotes -- but might also include math display environments via tex as well as potentially interactive canvases or embeded iframes.

# Parsing Model
1. Parsing is two-phase:  
   a. **Block phase** – recognises block constructs (headings, lists, code fences, display-math, raw HTML blocks, iframe blocks).  
   b. **Inline phase** – runs inside paragraphs only; code and math suppress all further inline parsing.  
2. A paragraph is any sequence of lines terminated by **two consecutive newlines** or the macro `\n`.  
   - `\n` expands to `<br>` and **does not** end the paragraph.  
   - `\n\n` (or blank line) ends the paragraph and starts a new one.  
3. Indentation is **strict after the first nested list**:  
   - The first indented list item sets the file-wide indent unit (tab, 2-spaces, 4-spaces, …).  
   - Any later inconsistent indentation is a compile-time error.  
4. Macro expansion is a single pass after pre-lexing and before block/inline parsing.
   - Document macros use `\name`; they **do not** expand inside math regions.  
   - Math regions are identified **before** macro expansion by a fast pre-lexer that only looks for unescaped `$...$`, `$$...$$`, `\(...\)`, `\[...\]`.
   - Inside math, only TeX macros (loaded from sty files) are recognised.  

# Syntax
All of these will be given in some regular expression, since it is an easy way to represent the specific pattern. This does not necessarily imply that parsing should be done via regular expression

## Escapes

In normal text, a backslash escapes the next delimiter character. The backslash is removed and the next character is emitted literally. Escape handling is based on an odd number of immediately preceding backslashes: `\$` is a literal dollar sign, while `\\$...$` leaves the dollar sign active because the dollar is preceded by two backslashes.

Escapable inline characters are `\`, `$`, `*`, `` ` ``, `[`, `]`, `^`, `_`, `{`, `}`, `(`, `)`, `<`, `>`, and `|`.

Important cases:
- `\$50` renders as `$50` and does not start inline math.
- `\\n` renders as the literal text `\n`; unescaped `\n` expands to `<br>`.
- `[label]` is ordinary text unless it is immediately followed by `(url)`.
- `[label](url)` is a hyperlink. To write literal link-shaped text, escape the closing bracket: `[label\](url)`.
- `\[` and `\]` are display-math delimiters, not the preferred way to write literal brackets. Use plain brackets for ordinary text, or `\\[` / `\\]` if the literal backslash-bracket sequence is required.

* /\*(.+?)\*/ -- (*text*) for italics.
* /\*\*(.+?)\*\*/ -- (**text**) for bold.
    - Bold may contain italics; italics may contain bold.  
    - Chronological left-to-right tokenisation; longest delimiter wins.  
    - Unbalanced delimiters are a compile-time error.  
* /^# / -- (# text)for headings
    * /^#\[(\d+)\]/ -- (#[n] text) for n-th level headings
    - Optional explicit ID: `#[n] text {#id}`  
* /^\s*- / -- (- text) at the start of the line for unordered list
    * /^\s*=\[(a|i|1)\] / -- (= [a] text) for an ordered list with label `a` (`a` for \alph*, `i` for \roman*, and `1` for \arabic*)
    * NOTE: The default ordered list hierarchy is arabic -> alphabet -> roman -> arabic -> ...
    * NOTE: List depth is determined by the file-wide indent unit set at the first nested list.
    * NOTE: For some level of list depth, that number of tabs/spaces is treated as the start of the line. This means that we can next code blocks, lists, math environments, etc. within some list level.
* Markdown tables use pipe-delimited rows and a dash delimiter row: `| H1 | H2 |\n| --- | --- |\n| A | B |`.
    - Delimiter cells may use `:---`, `:---:`, or `---:` for left, center, and right alignment.
    - Multiple rows before the delimiter row are emitted as header rows.
    - Inline parsing runs inside table cells. Escape table pipes as `\|`.
    - A cell containing only `>` or `<` merges into the visible cell to its left; a cell containing only `^` merges into the visible cell above it. Use `\>`, `\<`, or `\^` for literal markers.
    - Merge markers must describe a rectangular HTML span. Non-rectangular spans are compile errors.
    - A column whose cells are all dashes, e.g. `-`, is a vertical header separator. The separator column is omitted; body cells to its left are emitted as row headers.
* /(`)+(.+?)\1/ -- (`text` or ``text``) for code/inline monospaced
* /```(.*?)\n((.|\n)+?)\n``` -- (```code \n text \n ```) for multiline code/monospaced formatted according to the code language (hljs)
    * NOTE: If the code block "```" starts at some indentation, then the rendered code should trim that much internal indentation as well, unlike existing code blocks/verbatim/minted environments in md and latex.
    * NOTE: Closing fence must match the indentation of the opening fence.
* /\[(.+?)\]\(.*?\)/ -- ([link](href)) for hyperlinks
    - URLs may contain literal parentheses; unmatched closing parentheses are **kept** in the URL (author must encode if full match is desired).  
    - Plain brackets without a following URL, e.g. `[label]`, are normal text.
    - Escape the closing bracket for literal link-shaped text: `[label\](href)`.
* /\^\{(.*?)\}/ -- (text^{sup}) for superscript
* /\_\{(.*?)\}/ -- (text_{sub}) for subscript
    - Braces must be balanced; `\{` and `\}` are allowed.  
* /\^\[(.*?)\]\((.*?)\)/ -- (text^[note](url)) for footnotes/citations
    * NOTE: Citations automatically order/enumerate
    * NOTE: As an implementation detail which doesn't matter too much for now: for our particular rendering/compilation, the note is sticky on the footer of the screen, so long as the footnoted item is visible and we have not reached the actual end of the document.
    * NOTE: Perhaps we will use a proper bibliography with bibtex citation formats later on. For now, just enumerated arabic superscripts with href like ^[[1](href)] with a consolidated bibliography that is an ordered list of urls at the end.
* /\\\((.*?)\\\)|\$(.*?)\$/ -- (\(text\) or $text$) for inline math environments
* /\\\[((.|\n)*?)\\\]|\$\$((.|\n)*?)\$\$/ -- (\[ \n text\] or $$text\n$$) for display math environments
    - Display math is a block; its bounding box inherits the indentation of its opening delimiter.  
    - A display-math span may appear inside paragraph text; it splits the surrounding text into paragraph/display/paragraph blocks.
* /\(\w+) / -- (\macro) for document macros
    - Expanded only outside math; inside math, only TeX macros are recognised.  
* /\n\n|\\n/ -- (\n\n) for newlines
    - `\n` is a literal `<br>` and does **not** end the paragraph.

# Latex
For the math parsing, we effectively only care about things within latex math environments. Yes, we need to write logic to parse the bare minimum latex commands to load any sty file (latex.ltx), however we do not need to care about non-math things i.e. no `align`, yes `aligned`. We only need to parse the sty files into a modular/reusable format once per sty file, then use that modular extension for compilation for future documents. 

Again, we compile these to HTML. Mathjax chooses to do this by parsing as svgs, which can allow copy paste with annotation and carefully used user-select. However, we should consider that this may not be optimal.

# HTML
Using explicit HTML is allowed. No sanitisation is performed; trust level is the same as raw HTML.

## Raw HTML fence (compiler)
A block starting with a line `:::html` and closed by a line containing only `:::` passes the inner lines through verbatim into the HTML output (no escaping). Use this for trusted embeds—for example a `<div class="kml-playground-embed" data-kml-default-b64="…">` marker that the site hydrates with the live KML editor (see project README).

# Parsing
If there is invalid syntax or a parsing error, do not try to error correct. Fail quickly. Whether the HTML be invalid, or the tex be invalid, fail to compile.

Compilation order:
1. Pre-lex to identify math/code regions.
2. Expand document macros (top-to-bottom, no recursion).
3. Block-phase parsing.
4. Inline-phase parsing inside paragraphs.
5. Math rendering (external WASM).
6. Final HTML emission.
