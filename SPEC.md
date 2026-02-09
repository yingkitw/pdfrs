# PDF-CLI Technical Specification

## Overview

PDF-CLI is a command-line tool written in Rust that provides functionality for reading, writing, and converting PDF files to and from Markdown format. The implementation is designed to be self-contained, not relying on external PDF libraries, and implements core PDF specifications from scratch.

## Requirements

### Functional Requirements

#### FR1: PDF Generation

- **FR1.1**: Create PDF files from raw text input
- **FR1.2**: Support customizable fonts (Helvetica, Times-Roman, Courier)
- **FR1.3**: Support customizable font sizes
- **FR1.4**: Automatically split content into multiple pages when needed
- **FR1.5**: Generate PDFs compliant with PDF 1.4 specification

#### FR2: PDF Parsing

- **FR2.1**: Parse PDF file structure and extract objects
- **FR2.2**: Extract text content from PDF pages
- **FR2.3**: Handle compressed streams (deflate/zlib)
- **FR2.4**: Process PDF content streams and text operators
- **FR2.5**: Detect and handle different PDF encodings

#### FR3: Markdown Integration

- **FR3.1**: Parse Markdown syntax (headers, lists, emphasis, code blocks, tables)
- **FR3.2**: Convert Markdown to structured elements for rich PDF generation
- **FR3.3**: Convert extracted PDF text to Markdown format
- **FR3.4**: Preserve document structure during conversions
- **FR3.5**: Task list support (`- [x]` / `- [ ]`)
- **FR3.6**: Strikethrough text (`~~text~~`)
- **FR3.7**: Blockquote support with nesting (`>`, `>>`, `>>>`)
- **FR3.8**: Definition lists (`term` / `: definition`)
- **FR3.9**: Table alignment parsing (`:---`, `:---:`, `---:`)

#### FR4: Image Support

- **FR4.1**: Detect image formats (JPEG, PNG, BMP) with dimension parsing
- **FR4.2**: Embed JPEG images in PDF files (DCTDecode)
- **FR4.3**: Support image positioning and sizing with aspect-ratio scaling
- **FR4.4**: CLI `add-image` command fully wired

#### FR5: CLI Interface

- **FR5.1**: Provide subcommands for different operations
- **FR5.2**: Support command-line arguments for customization
- **FR5.3**: Provide helpful error messages and usage information
- **FR5.4**: Support input/output file specifications
- **FR5.5**: Page orientation (`--landscape` flag)

#### FR6: PDF Generation Enhancements

- **FR6.1**: Header font size hierarchy (H1=2x, H2=1.6x, H3=1.3x, H4=1.1x)
- **FR6.2**: Page numbering in footer
- **FR6.3**: Code block rendering with reduced font size (0.85x)
- **FR6.4**: Horizontal rule rendering
- **FR6.5**: Configurable page layout (portrait/landscape)
- **FR6.6**: Structured element pipeline (Markdown → Elements → PDF)

#### FR7: PDF Manipulation

- **FR7.1**: Merge multiple PDFs into a single output (`merge` command)
- **FR7.2**: Split PDF by page range (`split` command)
- **FR7.3**: Rotate all pages by 0/90/180/270° (`rotate` command)
- **FR7.4**: Document metadata embedding (title, author, subject, keywords) (`md-to-pdf-meta`)

#### FR8: Annotations and Multi-Image

- **FR8.1**: Text annotations with positioned notes on pages
- **FR8.2**: Link annotations with clickable URI actions
- **FR8.3**: Multiple JPEG images per page with independent positioning
- **FR8.4**: Highlight annotations with QuadPoints and color

#### FR9: Library API

- **FR9.1**: In-memory PDF generation via `generate_pdf_bytes()` (no filesystem needed)
- **FR9.2**: PDF structural validation via `validate_pdf_bytes()` returning `PdfValidation`
- **FR9.3**: Rich `Element` enum with 17 variants for document modeling
- **FR9.4**: Round-trip validation: generate → validate → parse → verify content
- **FR9.5**: Cross-reference stream parsing for PDF 1.5+ (`parse_xref_stream`)
- **FR9.6**: Object stream handling for compressed objects (`parse_object_stream`)

#### FR10: Extended Markdown Elements

- **FR10.1**: Image elements (`![alt](path)`) parsed and rendered
- **FR10.2**: Standalone link elements (`[text](url)`) parsed and rendered in blue
- **FR10.3**: Page break elements (`<!-- pagebreak -->` or `\pagebreak`)
- **FR10.4**: Inline code elements rendered with gray color
- **FR10.5**: Styled text elements (bold/italic) preserved
- **FR10.6**: Footnotes with label and text (`[^label]: text`)
- **FR10.7**: Definition lists (`term` / `: definition`)

#### FR11: Text Styling

- **FR11.1**: RGB color support via `Color` struct
- **FR11.2**: Text alignment (Left, Center) via `TextAlign` enum
- **FR11.3**: H1 headings centered, code blocks in gray, links in blue
- **FR11.4**: Watermarks with diagonal text, configurable opacity/size

### Non-Functional Requirements

#### NFR1: Performance

- **NFR1.1**: Process small PDF files (<1MB) in under 1 second
- **NFR1.2**: Handle large text files without memory issues
- **NFR1.3**: Efficient memory usage during PDF generation

#### NFR2: Compatibility

- **NFR2.1**: Support PDF files created by common applications
- **NFR2.2**: Generate PDFs readable by standard PDF viewers
- **NFR2.3**: Support common Markdown syntax variants

#### NFR3: Reliability

- **NFR3.1**: Handle malformed PDF files gracefully
- **NFR3.2**: Provide clear error messages for troubleshooting
- **NFR3.3**: Not crash on unexpected input

## System Architecture

### Core Components

#### 1. PDF Parser Module (`src/pdf.rs`)

```
PdfDocument
├── version: String
├── objects: HashMap<u32, PdfObject>
├── catalog: u32
└── pages: Vec<u32>

PdfObject
├── Dictionary(HashMap<String, PdfValue>)
├── Stream { dictionary, data }
├── Array(Vec<PdfValue>)
├── String(String)
├── Number(f64)
├── Boolean(bool)
├── Null
├── Reference(u32, u32)
└── Name(String)
```

**Responsibilities:**

- Parse PDF file structure
- Extract objects from PDF streams
- Handle compressed data
- Process content streams for text extraction

#### 2. PDF Generator Module (`src/pdf_generator.rs`)

```
PdfGenerator
├── objects: Vec<PdfObject>
└── next_id: u32

PdfObject
├── id: u32
├── generation: u32
├── content: String
├── is_stream: bool
└── stream_data: Option<Vec<u8>>
```

**Responsibilities:**

- Create PDF file structure
- Generate content streams
- Handle font resources
- Create page tree and catalog
- Write valid PDF format

#### 3. Markdown Parser (`src/markdown.rs`)

```
MarkdownParser
├── headers: Vec<Header>
├── paragraphs: Vec<Paragraph>
├── lists: Vec<List>
├── tables: Vec<Table>
└── code_blocks: Vec<CodeBlock>
```

**Responsibilities:**

- Parse Markdown syntax
- Convert to plain text
- Handle formatting preservation
- Process tables and lists

#### 4. Image Handler (`src/image.rs`)

```
ImageHandler
├── format_detector: FormatDetector
├── jpeg_processor: JpegProcessor
├── png_processor: PngProcessor
└── bmp_processor: BmpProcessor
```

**Responsibilities:**

- Detect image formats
- Process image data
- Create PDF image objects
- Generate image content streams

#### 5. Compression Module (`src/compression.rs`)

```
CompressionHandler
├── deflate_compressor: DeflateCompressor
├── hex_encoder: HexEncoder
└── stream_processor: StreamProcessor
```

**Responsibilities:**

- Compress and decompress streams
- Handle hex encoding/decoding
- Process compressed PDF objects

### Data Flow

#### PDF Generation Flow

```
Text Input → Markdown Parser → Text Processor → PDF Generator → PDF File
```

#### PDF Parsing Flow

```
PDF File → PDF Parser → Object Extractor → Text Processor → Text Output
```

#### Markdown to PDF Flow

```
Markdown File → Markdown Parser → Text Processor → PDF Generator → PDF File
```

## Algorithms

### PDF Object Parsing

1. Read PDF header to determine version
2. Locate and parse xref table
3. Extract objects based on xref references
4. Parse object dictionaries and streams
5. Handle compressed streams if present
6. Build object graph for document structure

### Text Extraction Algorithm

1. Iterate through page objects
2. Extract content streams from pages
3. Decompress streams if necessary
4. Parse content stream operators
5. Extract text strings from operators
6. Apply positioning and formatting
7. Combine text from all pages

### PDF Generation Algorithm

1. Create page objects with content streams
2. Generate font resources
3. Create page tree structure
4. Generate document catalog
5. Calculate object offsets
6. Generate xref table
7. Write trailer and EOF marker

### Markdown Parsing Algorithm

1. Tokenize input into lines
2. Identify block elements (headers, lists, code blocks, tables)
3. Parse inline elements (emphasis, links, code)
4. Build document structure
5. Convert to plain text representation

## Error Handling

### Error Types

1. **Parse Errors**: Malformed PDF structure
2. **IO Errors**: File access issues
3. **Format Errors**: Unsupported content
4. **Encoding Errors**: Invalid character encodings

### Error Recovery

- Skip malformed objects when possible
- Provide partial results when complete parsing fails
- Generate warnings for non-critical issues
- Fail gracefully with helpful error messages

## Security Considerations

### Input Validation

- Validate PDF file structure
- Check for buffer overflows
- Validate image file formats
- Sanitize text content

### Resource Limits

- Limit maximum file size
- Limit number of objects processed
- Limit recursion depth in parsing
- Monitor memory usage

## Performance Considerations

### Optimization Strategies

- Stream-based processing for large files
- Lazy loading of PDF objects
- Efficient string handling
- Minimal memory allocations

### Benchmarks

- Target: <1s for 1MB PDF processing
- Target: <100MB memory usage for typical operations
- Target: 10MB/s text extraction rate

## Testing Strategy

### Unit Tests

- PDF object parsing
- Text extraction algorithms
- Markdown parsing
- Image format detection
- Compression functions

### Integration Tests

- End-to-end PDF generation
- PDF to Markdown conversion
- Markdown to PDF conversion
- CLI command functionality

### Performance Tests

- Large file processing
- Memory usage profiling
- CPU usage monitoring
- Concurrency testing

## Future Enhancements

### Completed Features

- Advanced PDF parsing (xref streams, object streams, font encodings)
- Annotations (text, link, highlight)
- PDF manipulation (merge, split, rotate, reorder, watermark)
- Security (password protection, permissions)
- Library API (in-memory generation, validation)
- 17 element types with round-trip validation
- 251 tests (115 lib + 112 bin + 13 integration + 11 bench)

### Remaining Features

- Embedded/TrueType font support
- Full tagged PDF output for accessibility
- Vector graphics (SVG) support
- Digital signatures
- WebAssembly compilation
- Rustdoc API documentation with examples
