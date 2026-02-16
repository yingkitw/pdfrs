# PDF-CLI Architecture Documentation

## Overview

PDF-CLI is architectured as a modular system with clear separation of concerns. The design prioritizes maintainability, extensibility, and performance while implementing PDF functionality from scratch without external dependencies.

## High-Level Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        PDF-CLI CLI                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │   PDF I/O   │  │ Markdown I/O│  │   Image I/O         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │  PDF Core   │  │  Markdown   │  │   Image Processing │  │
│  │  Engine     │  │  Parser     │  │   Engine           │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐  │
│  │Compression  │  │   Utility   │  │    Error            │  │
│  │   Module    │  │  Functions  │  │   Handling          │  │
│  └─────────────┘  └─────────────┘  └─────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Module Architecture

### 1. CLI Module (`src/main.rs`)

**Purpose**: Command-line interface and application orchestration

**Responsibilities**:

- Parse command-line arguments using `clap`
- Route commands to appropriate handlers
- Coordinate between modules
- Handle application-level error reporting

**Key Components**:

```rust
struct Cli {
    command: Commands,
}

enum Commands {
    PdfToMd { input: String, output: String },
    MdToPdf { input: String, output: String, font: String, font_size: f32 },
    Extract { input: String },
    Create { output: String, text: String, font: String, font_size: f32 },
    AddImage { pdf_file: String, image_file: String, x: f32, y: f32, width: f32, height: f32 },
}
```

### 2. PDF Core Engine (`src/pdf.rs`)

**Purpose**: PDF parsing and text extraction

**Architecture Pattern**: Document Object Model (DOM) parser

**Key Classes**:

```rust
pub struct PdfDocument {
    pub version: String,
    pub objects: HashMap<u32, PdfObject>,
    pub catalog: u32,
    pub pages: Vec<u32>,
}

pub enum PdfObject {
    Dictionary(HashMap<String, PdfValue>),
    Stream { dictionary: HashMap<String, PdfValue>, data: Vec<u8> },
    Array(Vec<PdfValue>),
    String(String),
    Number(f64),
    Boolean(bool),
    Null,
    Reference(u32, u32),
    Name(String),
}
```

**Parsing Pipeline**:

```
PDF File → Header Parser → XRef Parser → Object Parser → Document Builder → Text Extractor
```

**Design Decisions**:

- Lazy loading of objects for memory efficiency
- Simple object model for maintainability
- Stream-based processing for large files

### 3. PDF Generator (`src/pdf_generator.rs`)

**Purpose**: Create PDF files from scratch

**Architecture Pattern**: Builder Pattern

**Key Classes**:

```rust
pub struct PdfGenerator {
    objects: Vec<PdfObject>,
    next_id: u32,
}

struct PdfObject {
    id: u32,
    generation: u32,
    content: String,
    is_stream: bool,
    stream_data: Option<Vec<u8>>,
}
```

**Generation Pipeline**:

```
Text Input → Page Builder → Content Stream Generator → Object Manager → File Writer
```

**Design Decisions**:

- Object ID management for proper references
- Content stream optimization
- PDF compliance with version 1.4 specification

### 4. Structured Elements (`src/elements.rs`)

**Purpose**: Define and parse structured document elements from Markdown

**Architecture Pattern**: Line Scanner → Element Classifier → Element Tree

**Key Types**:

```rust
pub enum Element {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    UnorderedListItem { text: String, depth: u8 },
    OrderedListItem { number: u32, text: String, depth: u8 },
    TaskListItem { checked: bool, text: String },
    CodeBlock { language: String, code: String },
    InlineCode { code: String },
    TableRow { cells: Vec<String>, is_separator: bool, alignments: Vec<TableAlignment> },
    BlockQuote { text: String, depth: u8 },
    DefinitionItem { term: String, definition: String },
    Footnote { label: String, text: String },
    Link { text: String, url: String },
    Image { alt: String, path: String },
    StyledText { text: String, bold: bool, italic: bool },
    PageBreak,
    HorizontalRule,
    EmptyLine,
}
```

**Key Functions**:

- `parse_markdown()`: Parse markdown text into `Vec<Element>`
- `strip_inline_formatting()`: Remove bold/italic/code/link/strikethrough syntax

**Design Decisions**:

- Elements carry formatting intent (heading level, list depth, checked state)
- Inline formatting stripped at parse time; structure preserved for PDF rendering
- Enables font size variations, indentation, and page numbering in PDF output

### 5. Markdown Converter (`src/markdown.rs`)

**Purpose**: Orchestrate Markdown-to-PDF and Markdown-to-text conversion

**Architecture Pattern**: Pipeline Coordinator

**Parsing Pipeline**:

```
Markdown Text → elements::parse_markdown() → Vec<Element> → pdf_generator::create_pdf_from_elements()
```

**Key Functions**:

- `markdown_to_text()`: Convert Markdown to plain text (legacy, uses elements internally)
- `markdown_to_pdf_with_options()`: Convert with styling via structured elements
- `elements_to_text()`: Render elements back to plain text

### 6. Image Processing (`src/image.rs`)

**Purpose**: Handle image embedding in PDFs

**Architecture Pattern**: Strategy Pattern for different image formats

**Key Components**:

```rust
pub fn create_jpeg_image_object(
    generator: &mut PdfGenerator,
    jpeg_data: Vec<u8>,
    width: u32,
    height: u32
) -> Result<u32>

pub fn create_image_content_stream(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    image_name: &str
) -> Result<Vec<u8>>
```

**Supported Formats**:

- JPEG (with DCTDecode)
- PNG (planned)
- BMP (planned)

### 7. PDF Operations (`src/pdf_ops.rs`)

**Purpose**: High-level PDF manipulation operations

**Key Functions**:

- `merge_pdfs()`: Combine multiple PDFs into one
- `split_pdf()`: Extract page range into a new PDF
- `rotate_pdf()`: Apply rotation (0/90/180/270°) to all pages
- `create_pdf_with_metadata()`: Generate PDF with Info dictionary (title, author, etc.)
- `create_pdf_with_annotations()`: Generate PDF with text and link annotations
- `create_pdf_with_images()`: Place multiple JPEG images on a single page

**Key Types**:

- `PdfMetadata`: Document properties (title, author, subject, keywords, creator)
- `TextAnnotation`: Positioned text note on a page
- `LinkAnnotation`: Clickable URI region on a page
- `HighlightAnnotation`: Colored highlight rectangle with QuadPoints
- `Color`: RGB color struct for text rendering
- `TextAlign`: Left/Center alignment enum

### 8. PDF Validation (`src/pdf.rs` — validation functions)

**Purpose**: Validate PDF structural integrity

**Key Functions**:

- `validate_pdf()`: Validate a PDF file on disk
- `validate_pdf_bytes()`: Validate PDF bytes in memory (no filesystem needed)
- `parse_xref_stream()`: Parse PDF 1.5+ cross-reference streams
- `parse_object_stream()`: Parse compressed object streams (/Type /ObjStm)

**Key Types**:

```rust
pub struct PdfValidation {
    pub valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
    pub page_count: usize,
    pub object_count: usize,
}
```

**Validation Checks**: PDF header, %%EOF marker, xref table, trailer, /Catalog, /Pages, page count, object/endobj pairing, stream/endstream pairing, /Root reference.

### 9. Compression Module (`src/compression.rs`)

**Purpose**: Handle PDF stream compression

**Architecture Pattern**: Strategy Pattern for compression algorithms

**Key Functions**:

- `decompress_deflate()`: Decompress zlib/deflate streams
- `compress_deflate()`: Compress data streams
- `decode_hex_string()`: Decode hex-encoded strings
- `encode_hex_string()`: Encode data as hex strings

## Data Flow Architecture

### PDF Generation Flow

```
Input Text → Markdown Parser → Content Builder → PDF Generator → File Writer
     ↓              ↓              ↓                ↓             ↓
Raw String → Structured Text → Content Streams → PDF Objects → Binary File
```

### PDF Parsing Flow

```
PDF File → Header Parser → Object Locator → Object Parser → Document Builder → Text Extractor
    ↓           ↓            ↓              ↓              ↓             ↓
Binary File → PDF Version → Object Offsets → PDF Objects → Document Model → Plain Text
```

### Conversion Flow

```
Source File → Appropriate Parser → Text Processing → Target Generator → Output File
     ↓              ↓                  ↓                ↓               ↓
PDF/MD File → PDF/MD Parser → Text Transformation → Target Generator → Target Format
```

## Design Patterns Used

### 1. Builder Pattern

- **Location**: `PdfGenerator`
- **Purpose**: Construct complex PDF objects step by step
- **Benefits**: Fluent interface, flexible configuration

### 2. Strategy Pattern

- **Location**: `Image` and `Compression` modules
- **Purpose**: Handle different formats and algorithms
- **Benefits**: Extensibility, interchangeable algorithms

### 3. Command Pattern

- **Location**: CLI module
- **Purpose**: Encapsulate user commands as objects
- **Benefits**: Decoupling, undo/redo capabilities

### 4. Iterator Pattern

- **Location**: PDF parsing
- **Purpose**: Traverse PDF objects and collections
- **Benefits**: Uniform interface, lazy evaluation

## Performance Considerations

### Memory Management

- **Strategy**: Streaming for large files, lazy loading
- **Implementation**: Buffered readers, object pools
- **Benefits**: Reduced memory footprint, better scalability

### CPU Optimization

- **Strategy**: Efficient string handling, minimal allocations
- **Implementation**: String builders, buffer reuse
- **Benefits**: Faster processing, lower CPU usage

### I/O Optimization

- **Strategy**: Buffered I/O, batch operations
- **Implementation**: Buffered readers/writers, bulk operations
- **Benefits**: Fewer system calls, better throughput

## Error Handling Architecture

### Error Hierarchy

```
Error
├── ParseError (PDF parsing failures)
├── IoError (File system issues)
├── FormatError (Unsupported formats)
└── ValidationError (Invalid input)
```

### Error Propagation

- **Strategy**: Result<T, Error> throughout the codebase
- **Implementation**: Question mark operator (?) for propagation
- **Benefits**: Explicit error handling, clear error paths

### Recovery Mechanisms

- **Partial Processing**: Continue processing other objects on failure
- **Graceful Degradation**: Fallback to simpler processing modes
- **User Feedback**: Clear error messages and suggestions

## Testing Architecture

### Test Organization

```
tests/
├── unit/ (Module-specific tests)
│   ├── pdf_tests.rs
│   ├── markdown_tests.rs
│   └── image_tests.rs
├── integration/ (End-to-end tests)
│   ├── conversion_tests.rs
│   └── cli_tests.rs
└── performance/ (Benchmarks)
    ├── parsing_benchmarks.rs
    └── generation_benchmarks.rs
```

### Test Strategies

- **Unit Tests**: Individual module functionality
- **Integration Tests**: Module interactions
- **Property Tests**: Input validation and edge cases
- **Performance Tests**: Resource usage and speed

## Extensibility Design

### Plugin Architecture (Future)

```
Plugin Interface
├── Parser Plugins (New formats)
├── Generator Plugins (New output formats)
├── Filter Plugins (Text processing)
└── Compression Plugins (New algorithms)
```

### Configuration System (Future)

- File-based configuration
- Runtime parameter adjustment
- Feature toggles
- Performance tuning options

## Security Architecture

### Input Validation

- File type verification
- Size limitations
- Content sanitization
- Path traversal prevention

### Resource Management

- Memory limits
- File handle limits
- Processing timeouts
- Temporary file cleanup

## Future Architecture Enhancements

### Multi-threading Support

- Parallel PDF parsing
- Concurrent image processing
- Background I/O operations
- Worker thread pools

### Caching System

- Object caching for repeated operations
- Result memoization
- Temporary file caching
- Metadata caching

### Plugin System

- Dynamic loading of parsers
- Custom output generators
- Extensible filter system
- Third-party integrations

This architecture provides a solid foundation for the current implementation while allowing for future enhancements and maintainability.
