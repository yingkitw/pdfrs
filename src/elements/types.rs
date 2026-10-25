//! Core element model: `Element`, text segments, chart and table types.

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
    Justify,
}

/// Text segment with inline formatting
#[derive(Debug, Clone, PartialEq)]
pub enum TextSegment {
    Plain(String),
    Bold(String),
    Italic(String),
    BoldItalic(String),
    Code(String),
    Strikethrough(String),
    MathInline(String),
    Link {
        text: String,
        url: String,
    },
    /// Inline citation `[@key]` rendered as \[n\].
    Citation {
        key: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Heading {
        level: u8,
        text: String,
    },
    Paragraph {
        text: String,
    },
    /// Rich paragraph with multiple styled segments
    RichParagraph {
        segments: Vec<TextSegment>,
    },
    UnorderedListItem {
        text: String,
        depth: u8,
    },
    OrderedListItem {
        number: u32,
        text: String,
        depth: u8,
    },
    TaskListItem {
        checked: bool,
        text: String,
    },
    CodeBlock {
        language: String,
        code: String,
    },
    InlineCode {
        code: String,
    },
    TableRow {
        cells: Vec<String>,
        is_separator: bool,
        alignments: Vec<TableAlignment>,
        colspans: Vec<u32>,
        rowspans: Vec<u32>,
    },
    BlockQuote {
        text: String,
        depth: u8,
    },
    DefinitionItem {
        term: String,
        definition: String,
    },
    Footnote {
        label: String,
        text: String,
    },
    Link {
        text: String,
        url: String,
    },
    Image {
        alt: String,
        path: String,
    },
    StyledText {
        text: String,
        bold: bool,
        italic: bool,
    },
    MathBlock {
        expression: String,
    },
    MathInline {
        expression: String,
    },
    PageBreak,
    HorizontalRule,
    EmptyLine,
    /// Switch multi-column layout for following content (`<!-- columns:N -->`).
    Columns {
        count: u8,
    },
    /// Chart from a fenced ` ```chart …` ` block (bar / line / pie / stacked-bar).
    Chart {
        kind: ChartKind,
        title: Option<String>,
        /// `(label, value)` pairs — used for single-series charts.
        points: Vec<(String, f32)>,
        /// Named series for multi-series charts (e.g. stacked bar).
        /// Each series has a name and values aligned with `points` labels.
        series: Vec<ChartSeries>,
    },
    /// Page folio style (`<!-- pagenumber:roman|arabic|none -->`).
    PageNumberMode {
        style: PageNumberStyle,
    },
    /// Running header toggle (`<!-- running-header:on|off -->`).
    RunningHeaderMode {
        enabled: bool,
    },
    /// Insert an in-document table of contents (`<!-- toc -->`).
    Toc,
    /// Emit numbered bibliography from citations (`<!-- bibliography -->`).
    Bibliography,
    /// Citation definition: `[@key]: full reference text`.
    CitationDef {
        key: String,
        text: String,
    },
}

/// Chart type for [`Element::Chart`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartKind {
    Bar,
    Line,
    Pie,
    /// Stacked bar chart (multi-series).
    StackedBar,
}

/// A named data series for multi-series charts.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f32>,
}

/// Page number / folio style for thesis front matter vs body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageNumberStyle {
    #[default]
    Arabic,
    Roman,
    None,
}

/// Parse alignment from a table separator cell like `:---`, `:---:`, `---:`
pub(super) fn parse_cell_alignment(cell: &str) -> TableAlignment {
    let t = cell.trim();
    let starts = t.starts_with(':');
    let ends = t.ends_with(':');
    if starts && ends {
        TableAlignment::Center
    } else if ends {
        TableAlignment::Right
    } else {
        TableAlignment::Left
    }
}
