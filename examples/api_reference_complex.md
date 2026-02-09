# PDF-RS Library API Reference

**Version**: 0.1.0 | **Edition**: Rust 2024 | **License**: MIT

> This document serves as the complete API reference for the pdf-rs library, covering all public modules, types, functions, and their usage patterns.

---

## Module Overview

| Module | Purpose | Key Types | Functions |
|:-------|:--------|:----------|----------:|
| `elements` | Document element model | `Element`, `TableAlignment` | 3 |
| `pdf` | PDF parsing and validation | `PdfDocument`, `PdfValidation` | 8 |
| `pdf_generator` | PDF creation | `PageLayout`, `Color`, `ContentStreamBuilder` | 6 |
| `pdf_ops` | PDF manipulation | `PdfMetadata`, annotations | 12 |
| `markdown` | Markdown conversion | — | 4 |
| `image` | Image embedding | — | 5 |
| `compression` | Stream compression | — | 4 |
| `security` | Password protection | `PdfSecurity`, `PdfPermissions` | 3 |

---

## 1. Elements Module (`elements`)

### 1.1 Element Enum

The `Element` enum is the core document model with 17 variants[^1]:

```rust
pub enum Element {
    // Block-level elements
    Heading { level: u8, text: String },
    Paragraph { text: String },
    CodeBlock { language: String, code: String },
    BlockQuote { text: String, depth: u8 },
    HorizontalRule,
    PageBreak,
    EmptyLine,

    // List elements
    UnorderedListItem { text: String, depth: u8 },
    OrderedListItem { number: u32, text: String, depth: u8 },
    TaskListItem { checked: bool, text: String },

    // Inline-rich elements
    InlineCode { code: String },
    Link { text: String, url: String },
    Image { alt: String, path: String },
    StyledText { text: String, bold: bool, italic: bool },

    // Table elements
    TableRow { cells: Vec<String>, is_separator: bool, alignments: Vec<TableAlignment> },

    // Reference elements
    DefinitionItem { term: String, definition: String },
    Footnote { label: String, text: String },
}
```

### 1.2 TableAlignment Enum

```rust
pub enum TableAlignment {
    Left,
    Center,
    Right,
    None,
}
```

### 1.3 Functions

#### `parse_markdown(input: &str) -> Vec<Element>`

Parses a markdown string into a vector of structured elements.

- **Input**: Raw markdown text
- **Output**: Ordered vector of `Element` variants
- **Complexity**: O(n) where n is the number of lines

```rust
let elements = pdf_rs::elements::parse_markdown("# Hello\n\nWorld");
assert_eq!(elements.len(), 3); // Heading + EmptyLine + Paragraph
```

> **Note**: Inline formatting (bold, italic, strikethrough) is stripped during parsing. The structural intent (heading level, list depth, checked state) is preserved in the element variants.

#### `strip_inline_formatting(text: &str) -> String`

Removes markdown inline formatting syntax from text.

- Strips `**bold**`, `*italic*`, `` `code` ``, `~~strikethrough~~`
- Strips link syntax `[text](url)` → `text`
- Preserves plain text content

```rust
let clean = pdf_rs::elements::strip_inline_formatting("**bold** and *italic*");
assert_eq!(clean, "bold and italic");
```

---

## 2. PDF Module (`pdf`)

### 2.1 PdfDocument

```rust
pub struct PdfDocument {
    pub version: String,
    pub objects: HashMap<u32, PdfObject>,
    pub catalog: u32,
    pub pages: Vec<u32>,
}
```

### 2.2 PdfValidation

```rust
pub struct PdfValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub page_count: usize,
    pub object_count: usize,
}
```

### 2.3 Validation Functions

#### `validate_pdf(path: &str) -> PdfValidation`

Validates a PDF file on disk. Performs 11 structural checks:

1. PDF header presence (`%PDF-x.x`)
2. EOF marker (`%%EOF`)
3. Cross-reference table or stream
4. `startxref` pointer
5. Trailer dictionary
6. Document catalog (`/Type /Catalog`)
7. Pages tree (`/Type /Pages`)
8. Page object count
9. Object count and `obj`/`endobj` pairing
10. Stream/endstream pairing
11. `/Root` reference in trailer

#### `validate_pdf_bytes(data: &[u8]) -> PdfValidation`

Same validation as `validate_pdf` but operates on in-memory bytes[^2].

```rust
let pdf_bytes = pdf_rs::pdf_generator::generate_pdf_bytes(
    &elements, "Helvetica", 12.0, layout
).unwrap();
let result = pdf_rs::pdf::validate_pdf_bytes(&pdf_bytes);
assert!(result.valid);
println!("Pages: {}, Objects: {}", result.page_count, result.object_count);
```

#### `parse_pdf(path: &str) -> Result<PdfDocument>`

Parses a PDF file into a `PdfDocument` structure.

- Handles PDF 1.0 through 1.7
- Supports compressed streams (FlateDecode)
- Parses cross-reference streams (PDF 1.5+)
- Parses object streams (`/Type /ObjStm`)

#### `extract_text(doc: &PdfDocument) -> String`

Extracts all text content from a parsed PDF document.

- Processes `Tj` and `TJ` text operators
- Handles font encodings (WinAnsiEncoding, MacRomanEncoding)
- Tracks text positioning via `Td` and `Tm` operators
- Handles escaped parentheses and octal sequences

---

## 3. PDF Generator Module (`pdf_generator`)

### 3.1 PageLayout

```rust
pub struct PageLayout {
    pub width: f32,
    pub height: f32,
    pub margin_top: f32,
    pub margin_bottom: f32,
    pub margin_left: f32,
    pub margin_right: f32,
}

impl PageLayout {
    pub fn portrait() -> Self;
    pub fn landscape() -> Self;
}
```

Portrait
: 612 x 792 points (8.5 x 11 inches, US Letter)

Landscape
: 792 x 612 points (11 x 8.5 inches, US Letter rotated)

### 3.2 Color

```rust
pub struct Color {
    pub r: f32,  // 0.0 - 1.0
    pub g: f32,
    pub b: f32,
}
```

Predefined colors used internally:

- **Black** (0.0, 0.0, 0.0) — default text
- **Blue** (0.0, 0.0, 0.8) — links
- **Gray** (0.3, 0.3, 0.3) — inline code and code blocks
- **Light Gray** (0.95, 0.95, 0.95) — code block backgrounds

### 3.3 TextAlign

```rust
pub enum TextAlign {
    Left,
    Center,
}
```

### 3.4 Key Functions

#### `generate_pdf_bytes(elements, font, font_size, layout) -> Result<Vec<u8>>`

Generates a complete PDF document as bytes in memory.

```rust
use pdf_rs::{elements, pdf_generator};

let elems = elements::parse_markdown("# Title\n\nContent here.");
let layout = pdf_generator::PageLayout::portrait();
let bytes = pdf_generator::generate_pdf_bytes(
    &elems, "Helvetica", 12.0, layout
).unwrap();
// bytes contains a valid PDF ready to write or transmit
```

> This is the primary library API for PDF generation. No filesystem access is required.

#### `create_pdf_from_elements_with_layout(filename, elements, font, font_size, layout) -> Result<()>`

Generates a PDF and writes it directly to a file.

#### `render_elements_to_builder(builder, elements, base_font_size)`

Renders elements into a `ContentStreamBuilder`. Used internally but available for advanced customization.

---

## 4. PDF Operations Module (`pdf_ops`)

### 4.1 Document Manipulation

| Function | Description | Parameters |
|:---------|:------------|:-----------|
| `merge_pdfs` | Combine multiple PDFs | input paths, output path |
| `split_pdf` | Extract page range | input, output, start, end |
| `rotate_pdf` | Rotate all pages | input, output, angle (0/90/180/270) |
| `reorder_pages` | Reorder pages | input, output, page order |
| `watermark_pdf` | Add text watermark | input, output, text, size, opacity |

### 4.2 Metadata

```rust
pub struct PdfMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub custom_fields: HashMap<String, String>,
}
```

### 4.3 Annotations

Three annotation types are supported:

TextAnnotation
: A positioned text note displayed as a popup icon on the page

LinkAnnotation
: A clickable rectangular region that opens a URI when clicked

HighlightAnnotation
: A colored highlight overlay defined by QuadPoints coordinates

```rust
pub struct TextAnnotation {
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
    pub text: String,
}

pub struct LinkAnnotation {
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
    pub url: String,
}

pub struct HighlightAnnotation {
    pub x: f32, pub y: f32,
    pub width: f32, pub height: f32,
    pub color: (f32, f32, f32),
}
```

---

## 5. Markdown Module (`markdown`)

### 5.1 Conversion Functions

#### `markdown_to_pdf_with_options(input, output, font, font_size, landscape) -> Result<()>`

Full pipeline: reads markdown file → parses elements → generates PDF.

#### `elements_to_text(elements: &[Element]) -> String`

Converts elements back to plain text representation. Used for round-trip validation.

- Headings rendered with `#` prefix
- Lists rendered with bullets and numbers
- Code blocks wrapped in triple backticks
- Tables rendered with pipe separators
- Links rendered as `[text](url)`
- Images rendered as `[Image: alt] (path)`

---

## 6. Error Handling

All fallible functions return `anyhow::Result<T>`. Common error scenarios:

1. **File I/O errors**
   - File not found
   - Permission denied
   - Disk full
2. **Parse errors**
   - Malformed PDF header
   - Corrupted cross-reference table
   - Invalid object references
   - Unsupported compression filters
3. **Generation errors**
   - Invalid font name
   - Content exceeds page boundaries
   - Image file not found or unsupported format

> **Best Practice**: Always validate generated PDFs with `validate_pdf_bytes()` in test code to catch structural issues early.

---

## 7. Usage Examples

### 7.1 Basic Round-Trip

```rust
use pdf_rs::{elements, pdf_generator, pdf, markdown};

// Parse markdown
let md = "# Hello World\n\nThis is a test document.\n\n- Item one\n- Item two";
let elems = elements::parse_markdown(md);

// Generate PDF bytes
let layout = pdf_generator::PageLayout::portrait();
let bytes = pdf_generator::generate_pdf_bytes(&elems, "Helvetica", 12.0, layout)?;

// Validate
let validation = pdf::validate_pdf_bytes(&bytes);
assert!(validation.valid);
assert_eq!(validation.page_count, 1);

// Write to file
std::fs::write("output.pdf", &bytes)?;
```

### 7.2 Complex Document Generation

```rust
use pdf_rs::elements::Element;
use pdf_rs::pdf_generator::{PageLayout, generate_pdf_bytes};

let elements = vec![
    Element::Heading { level: 1, text: "Annual Report 2026".into() },
    Element::Paragraph { text: "Executive summary content...".into() },
    Element::HorizontalRule,
    Element::Heading { level: 2, text: "Financial Overview".into() },
    Element::TableRow {
        cells: vec!["Quarter".into(), "Revenue".into(), "Growth".into()],
        is_separator: false,
        alignments: vec![],
    },
    Element::TableRow {
        cells: vec!["---".into(), "---".into(), "---".into()],
        is_separator: true,
        alignments: vec![],
    },
    Element::TableRow {
        cells: vec!["Q1".into(), "$2.4M".into(), "+12%".into()],
        is_separator: false,
        alignments: vec![],
    },
    Element::PageBreak,
    Element::Heading { level: 2, text: "Appendix".into() },
    Element::CodeBlock {
        language: "sql".into(),
        code: "SELECT quarter, revenue FROM financials ORDER BY quarter;".into(),
    },
];

let bytes = generate_pdf_bytes(&elements, "Times-Roman", 11.0, PageLayout::portrait())?;
```

### 7.3 Landscape Report with Metadata

```bash
pdf-cli md-to-pdf-meta report.md report.pdf \
  --title "Q1 Performance Report" \
  --author "Engineering Team" \
  --subject "System Performance" \
  --keywords "performance,latency,throughput" \
  --landscape
```

---

## Appendix: Supported PDF Features Matrix

| Feature | Read | Write | Validate | Notes |
|:--------|:----:|:-----:|:--------:|:------|
| Text content | Yes | Yes | — | Tj, TJ operators |
| Font selection | Yes | Yes | — | Type 1 fonts only |
| Multi-page | Yes | Yes | Yes | Automatic pagination |
| Compression | Yes | Yes | — | FlateDecode |
| Images (JPEG) | — | Yes | — | DCTDecode |
| Tables | — | Yes | — | Via elements |
| Metadata | Yes | Yes | — | Info dictionary |
| Annotations | — | Yes | — | Text, Link, Highlight |
| Watermarks | — | Yes | — | Diagonal text |
| Page rotation | Yes | Yes | — | 0/90/180/270 |
| Merge/Split | Yes | Yes | — | Page-level ops |
| Password protection | — | Yes | — | RC4/AES |
| Tagged PDF | — | Partial | — | Structure types defined |
| XRef streams | Yes | — | Yes | PDF 1.5+ |
| Object streams | Yes | — | — | /Type /ObjStm |

[^1]: The 17 element variants cover all common markdown constructs plus PDF-specific features like page breaks.
[^2]: In-memory validation is preferred for testing pipelines where filesystem access should be minimized.
[^3]: Font support is limited to the 14 standard Type 1 fonts defined in the PDF specification.

---

*Generated by pdf-rs v0.1.0*
