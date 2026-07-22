# Examples and Test Cases

This directory contains comprehensive examples demonstrating the unicode, math, and code conversion capabilities of the PDF processing library.

## Capability Showcase

A multi-feature validation fixture lives at `tests/fixtures/capability_showcase.md`
with automated checks in `tests/capability_validation.rs`:

```bash
cargo test --test capability_validation
```

This exercises plugins (callouts), outlines/bookmarks, multi-page breaks, RTL
samples, linearization, incremental updates, vector/SVG/3D side channels, and
writes sample PDFs under `tests/output/capability_*.pdf`.

## Quick Start

### Run All Validation Tests
```bash
./validate_examples.sh
```

This will generate PDFs from all example markdown files and validate the output.

## Example Files

### 1. Unicode Showcase (`unicode_showcase.md`)
Demonstrates comprehensive unicode support including:
- **Multiple Scripts:** Chinese (中文), Japanese (日本語), Korean (한국어), Arabic (العربية), Greek (Ελληνικά), Cyrillic (Русский)
- **Mathematical Symbols:** ∀ ∃ ∈ ∉ ∑ ∏ ∫ ∂ ∇ √ ∞ ≠ ≈ ≤ ≥
- **Currency Symbols:** $ € £ ¥ ₹ ₽ ₩ ₿
- **Special Characters:** Arrows, stars, checkmarks, emojis
- **Diacritical Marks:** Àà Áá Ââ Ãã Ää Åå Çç Éé Íí Ññ Öö Üü

**Convert to PDF:**
```bash
cargo run --release --bin pdfcli -- md-to-pdf examples/unicode_showcase.md output/unicode_showcase.pdf
```

### 2. Math Showcase (`math_showcase.md`)
Demonstrates mathematical expression support:
- **Inline Math:** `$E = mc^2$`, `$a^2 + b^2 = c^2$`
- **Block Math:** Integrals, derivatives, matrices, summations
- **Calculus:** `$$\int_a^b f(x) dx = F(b) - F(a)$$`
- **Linear Algebra:** Matrices and vectors
- **Statistics:** Mean, variance, standard deviation
- **Set Theory:** Union, intersection, subset notation

**Convert to PDF:**
```bash
cargo run --release --bin pdfcli -- md-to-pdf examples/math_showcase.md output/math_showcase.pdf
```

### 3. Code Showcase (`code_showcase.md`)
Demonstrates code syntax highlighting for multiple languages:
- **Languages:** Rust, Python, JavaScript, TypeScript, Go, Java, SQL, Bash
- **Features:** Syntax highlighting, inline code, code comments
- **Examples:** Algorithms (Fibonacci, quicksort, binary tree, FFT)

**Convert to PDF:**
```bash
cargo run --release --bin pdfcli -- md-to-pdf examples/code_showcase.md output/code_showcase.pdf
```

### 4. Comprehensive Test (`comprehensive_test.md`)
Combines all features in one document:
- Multilingual content with math expressions
- Code blocks with unicode comments
- Tables with mixed content
- Special characters and symbols
- Escape sequences

**Convert to PDF:**
```bash
cargo run --release --bin pdfcli -- md-to-pdf examples/comprehensive_test.md output/comprehensive_test.pdf
```

## Running Tests

### Unit Tests
```bash
# Run all unit tests
cargo test --lib

# Run specific unicode tests
cargo test --lib -- test_decode --nocapture

# Run specific math/code tests
cargo test --lib -- test_unescape --nocapture
```

### Integration Tests
```bash
# Run all integration tests
cargo test --test unicode_integration_test

# Run with output
cargo test --test unicode_integration_test -- --nocapture
```

### Validation Script
```bash
# Make executable (first time only)
chmod +x examples/validate_examples.sh

# Run validation
./examples/validate_examples.sh
```

## Test Coverage

### Unicode Support
- ✅ Octal escape sequences (`\101` → "A")
- ✅ Hex string decoding (`48656C6C6F` → "Hello")
- ✅ UTF-16BE with BOM (`FEFF00480065006C006C006F` → "Hello")
- ✅ Surrogate pairs for emojis (`FEFFD83DDE00` → "😀")
- ✅ Multiple scripts (Chinese, Japanese, Korean, Arabic, Greek, Cyrillic)
- ✅ Special characters (math symbols, currency, arrows)

### Math Expression Support
- ✅ Inline math: `$expression$`
- ✅ Block math: `$$expression$$`
- ✅ Greek letters: α β γ δ ε ζ η θ
- ✅ Mathematical operators: ∑ ∫ ∂ ∇ √ ∞
- ✅ Complex formulas with fractions and exponents

### Code Block Support
- ✅ Syntax highlighting for 10+ languages
- ✅ Inline code with backticks
- ✅ Code comments (including unicode)
- ✅ Proper indentation and formatting

## Output Files

Generated PDFs are saved to:
- `examples/output/` - Example file conversions
- `tests/output/` - Integration test outputs

## Validation Results

All tests passing:
- **Unit Tests:** 141/141 ✅
- **Integration Tests:** 7/7 ✅
- **Example Conversions:** 6/6 ✅

See `docs/VALIDATION_REPORT.md` for detailed results.

## API Usage Examples

### Convert Markdown to PDF
```rust
use pdfrs::markdown;

fn main() -> anyhow::Result<()> {
    markdown::markdown_to_pdf(
        "input.md",
        "output.pdf"
    )?;
    Ok(())
}
```

### Extract Text from PDF (with Unicode support)
```rust
use pdfrs::pdf;

fn main() -> anyhow::Result<()> {
    let text = pdf::extract_text("input.pdf")?;
    println!("{}", text);
    Ok(())
}
```

### Decode Hex Strings
```rust
use pdfrs::pdf::decode_pdf_hex_string;

fn main() {
    // ASCII hex string
    let text1 = decode_pdf_hex_string("48656C6C6F");
    assert_eq!(text1, "Hello");
    
    // UTF-16BE with BOM
    let text2 = decode_pdf_hex_string("FEFF4F60597D");
    assert_eq!(text2, "你好");
    
    // Unicode symbols
    let text3 = decode_pdf_hex_string("FEFF03B103B203B3");
    assert_eq!(text3, "αβγ");
}
```

### Unescape PDF Strings
```rust
use pdfrs::pdf::unescape_pdf_string;

fn main() {
    // Octal escape sequences
    let text1 = unescape_pdf_string(r"\101\102\103");
    assert_eq!(text1, "ABC");
    
    // Standard escapes
    let text2 = unescape_pdf_string(r"Hello\nWorld");
    assert_eq!(text2, "Hello\nWorld");
}
```

## Troubleshooting

### PDF Generation Issues
- Ensure input markdown file exists
- Check file permissions for output directory
- Verify markdown syntax is correct

### Unicode Display Issues
- Some fonts may not support all unicode characters
- Use Helvetica (default) for best unicode support
- Check terminal encoding for text extraction

### Test Failures
- Run `cargo clean` and rebuild
- Ensure all dependencies are up to date
- Check file paths are absolute when needed

## Additional Resources

- **Main Documentation:** `../README.md`
- **Validation Report:** `../docs/VALIDATION_REPORT.md`
- **Architecture:** `../docs/ARCHITECTURE.md`
- **TODO:** `../docs/TODO.md`

## Contributing

When adding new examples:
1. Create markdown file in `examples/`
2. Add test case to `tests/unicode_integration_test.rs`
3. Update `validate_examples.sh`
4. Run validation to ensure it works
5. Update this README

## License

Apache-2.0
