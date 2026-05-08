//! AST types for Kernel ML / KML (see `spec/implementation.md`).

#[derive(Debug, Clone)]
pub enum Block {
    Heading {
        level: u8,
        id: Option<String>,
        text: String,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    /// List of items at a single indentation level.
    /// Nested lists appear as `Block::List` entries inside `ListItem.blocks`.
    List {
        /// `true` for `<ol>`, `false` for `<ul>`.
        ordered: bool,
        /// For ordered lists, optional style:
        /// - `"1"` -> `<ol type="1">`
        /// - `"a"` -> `<ol type="a">`
        /// - `"i"` -> `<ol type="i">`
        /// `None` for unordered lists.
        style: Option<String>,
        items: Vec<ListItem>,
    },
    CodeBlock {
        lang: String,
        content: String,
    },
    DisplayMath {
        content: String,
    },
    HtmlBlock {
        content: String,
    },
}

#[derive(Debug, Clone)]
pub struct ListItem {
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub enum Inline {
    Text { content: String },
    LineBreak,
    Bold { children: Vec<Inline> },
    Italic { children: Vec<Inline> },
    Code { content: String },
    InlineMath { content: String },
    Link { text: String, href: String },
    Footnote { note: String, href: String },
    Superscript { content: String },
    Subscript { content: String },
}
