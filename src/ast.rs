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
        items: Vec<ListItem>,
    },
    Blockquote {
        blocks: Vec<Block>,
    },
    Table {
        rows: Vec<TableRow>,
        header_rows: usize,
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
    /// Visible marker for ordered lists. `None` for unordered list items.
    pub marker: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<TableCell>,
}

#[derive(Debug, Clone)]
pub struct TableCell {
    pub inlines: Vec<Inline>,
    pub header: bool,
    pub align: Option<TableAlignment>,
    pub rowspan: usize,
    pub colspan: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
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
