#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpanKind {
    Normal,
    MathInline,
    MathDisplay,
    CodeInline,
    CodeDisplay,
}

#[derive(Clone, Debug)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    pub kind: SpanKind,
}

#[derive(Clone, Debug)]
pub enum Block {
    Heading {
        level: u8,
        id: Option<String>,
        text: String,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    List {
        ordered: bool,
        style: Option<String>,
        items: Vec<ListItem>,
    },
    CodeBlock {
        lang: String,
        content: String,
    },
    DisplayMath { content: String },
    HtmlBlock { content: String },
    FrontMatter { content: String },
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub depth: usize,
    pub blocks: Vec<Block>,
}

#[derive(Clone, Debug)]
pub enum Inline {
    Text { content: String },
    Bold { children: Vec<Inline> },
    Italic { children: Vec<Inline> },
    Code { content: String },
    InlineMath { content: String },
    Link { text: String, href: String },
    Footnote { note: String, href: String },
    Superscript { content: String },
    Subscript { content: String },
    LineBreak,
}
