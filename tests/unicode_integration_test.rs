use pdfrs::markdown;
use pdfrs::pdf;
use std::fs;
use std::path::Path;

fn assert_contains_any(haystack: &str, candidates: &[&str], label: &str) {
    assert!(
        candidates.iter().any(|c| haystack.contains(c)),
        "Expected {} to contain one of {:?}, got: {}",
        label,
        candidates,
        haystack
    );
}

#[test]
fn test_unicode_pdf_generation() {
    let test_md = "tests/fixtures/unicode_test.md";
    let test_pdf = "tests/output/unicode_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    let content = r#"# Unicode Test

## Chinese
你好世界

## Japanese
こんにちは

## Korean
안녕하세요

## Greek
Γεια σου κόσμε

## Math Symbols
∑ ∫ ∞ ≈ ≠ ± × ÷

## Currency
$ € £ ¥ ₹
"#;

    fs::write(test_md, content).expect("Failed to write test markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());

    assert!(Path::new(test_pdf).exists(), "PDF file was not created");

    let metadata = fs::metadata(test_pdf).expect("Failed to read PDF metadata");
    assert!(metadata.len() > 0, "PDF file is empty");

    fs::remove_file(test_md).ok();
}

#[test]
fn test_complex_math_formula_roundtrip_extraction() {
    let test_md = "tests/fixtures/complex_math_formula_test.md";
    let test_pdf = "tests/output/complex_math_formula_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    let content = r#"# Complex Math Formula Test

Inline limit and fraction: $\lim_{x\to0} \frac{\sin x}{x}$

Set membership: $x \notin A$, and root: $\sqrt{a^2 + b^2}$

Block formulas:

$$
\int_0^1 x^2 dx + \sum_{i=1}^{n} a_i
\prod_{k=1}^{m} b_k
$$

Quantifiers:

$$
\forall x \in \mathbb{R},\; x \ge 0 \Rightarrow \sqrt{x} \in \mathbb{R}
$$
"#;

    fs::write(test_md, content).expect("Failed to write complex math markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());
    assert!(Path::new(test_pdf).exists(), "PDF file was not created");

    let extracted = pdf::extract_text(test_pdf).expect("Failed to extract text from generated PDF");

    assert_contains_any(&extracted, &["lim(x→0)", "lim(x->0)"], "limit");
    assert_contains_any(
        &extracted,
        &["(sin x)/(x)", "sin x⁄x", "(sin x)⁄(x)"],
        "fraction",
    );
    assert_contains_any(&extracted, &["∉", "not-in"], "not-in operator");
    assert_contains_any(
        &extracted,
        &["√(a² + b²)", "√(a^(2) + b^(2))", "sqrt(a^(2) + b^(2))"],
        "square root",
    );
    // Display math places limits as separate positioned runs.
    assert_contains_any(
        &extracted,
        &["∫₀¹", "∫[0→1]", "int[0->1]", "∫"],
        "integral with limits",
    );
    assert!(
        extracted.contains('0') && extracted.contains('1') && extracted.contains('∫'),
        "Expected integral limits 0/1 near ∫, got: {}",
        extracted
    );
    assert_contains_any(
        &extracted,
        &["∑ᵢ₌₁ⁿ", "∑[i=1→n]", "sum[i=1->n]", "∑"],
        "summation with limits",
    );
    assert!(
        (extracted.contains("i=1") || extracted.contains("ᵢ")) && extracted.contains('∑'),
        "Expected sum lower limit near ∑, got: {}",
        extracted
    );
    assert_contains_any(
        &extracted,
        &["∏ₖ₌₁ᵐ", "∏[k=1→m]", "prod[k=1->m]", "∏"],
        "product with limits",
    );
    assert_contains_any(&extracted, &["∀ x", "forall x"], "quantifier");
    assert_contains_any(&extracted, &["ℝ", " R"], "real-number set");
    assert_contains_any(&extracted, &["≥ 0", ">= 0"], "greater-than-or-equal");
    assert_contains_any(&extracted, &["⇒", "=>"], "implication");

    fs::remove_file(test_md).ok();
}

#[test]
fn test_unicode_roundtrip_extraction() {
    let test_md = "tests/fixtures/unicode_extract_test.md";
    let test_pdf = "tests/output/unicode_extract_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    let content = "Unicode extraction: 你好 Γεια";
    fs::write(test_md, content).expect("Failed to write unicode extraction markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());

    let extracted = pdf::extract_text(test_pdf).expect("Failed to extract text from generated PDF");
    assert!(
        extracted.contains("你好"),
        "Expected extracted text to contain Chinese unicode, got: {}",
        extracted
    );
    assert!(
        extracted.contains("Γεια"),
        "Expected extracted text to contain Greek unicode, got: {}",
        extracted
    );

    fs::remove_file(test_md).ok();
}

#[test]
fn test_math_pdf_generation() {
    let test_md = "tests/fixtures/math_test.md";
    let test_pdf = "tests/output/math_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    let content = r#"# Math Test

Inline math: $E = mc^2$

Block math:

$$
\int_a^b f(x) dx = F(b) - F(a)
$$

More inline: $\pi \approx 3.14159$

Another block:

$$
\sum_{i=1}^{n} i = \frac{n(n+1)}{2}
$$
"#;

    fs::write(test_md, content).expect("Failed to write test markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());

    assert!(Path::new(test_pdf).exists(), "PDF file was not created");

    fs::remove_file(test_md).ok();
}

#[test]
fn test_code_pdf_generation() {
    let test_md = "tests/fixtures/code_test.md";
    let test_pdf = "tests/output/code_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    let content = r#"# Code Test

## Rust Code

```rust
fn main() {
    println!("Hello, world!");
}
```

## Python Code

```python
def hello():
    print("Hello, world!")
```

Inline code: `let x = 42;`
"#;

    fs::write(test_md, content).expect("Failed to write test markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());

    assert!(Path::new(test_pdf).exists(), "PDF file was not created");

    fs::remove_file(test_md).ok();
}

#[test]
fn test_comprehensive_pdf_generation() {
    let test_md = "tests/fixtures/comprehensive_test.md";
    let test_pdf = "tests/output/comprehensive_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    let content = r#"# Comprehensive Test

## Unicode
中文: 你好
日本語: こんにちは
한국어: 안녕하세요

## Math
Inline: $a^2 + b^2 = c^2$

Block:
$$
E = mc^2
$$

## Code

```rust
fn fibonacci(n: u32) -> u32 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n-1) + fibonacci(n-2),
    }
}
```

## Symbols
∑ ∫ ∞ ≈ ≠ € ¥ £
"#;

    fs::write(test_md, content).expect("Failed to write test markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());

    assert!(Path::new(test_pdf).exists(), "PDF file was not created");

    let metadata = fs::metadata(test_pdf).expect("Failed to read PDF metadata");
    assert!(metadata.len() > 1000, "PDF file seems too small");

    fs::remove_file(test_md).ok();
}

#[test]
fn test_pdf_hex_string_extraction() {
    use pdfrs::pdf::{decode_pdf_hex_string, unescape_pdf_string};

    assert_eq!(decode_pdf_hex_string("48656C6C6F"), "Hello");
    assert_eq!(decode_pdf_hex_string("576F726C64"), "World");

    assert_eq!(decode_pdf_hex_string("FEFF00480065006C006C006F"), "Hello");
    assert_eq!(decode_pdf_hex_string("FEFF4F60597D"), "你好");

    assert_eq!(decode_pdf_hex_string("FEFF03B103B203B3"), "αβγ");

    assert_eq!(unescape_pdf_string(r"\101\102\103"), "ABC");
    assert_eq!(unescape_pdf_string(r"Hello\40World"), "Hello World");
}

#[test]
fn test_missing_glyph_falls_back_to_question_mark() {
    let test_md = "tests/fixtures/missing_glyph_test.md";
    let test_pdf = "tests/output/missing_glyph_test.pdf";

    fs::create_dir_all("tests/fixtures").ok();
    fs::create_dir_all("tests/output").ok();

    // 😀 / ₹ / ₿ are typically absent from Arial Unicode; they must not pollute ToUnicode.
    let content = "CJK 你好 then missing 😀 ₹ ₿ and euro €";
    fs::write(test_md, content).expect("Failed to write missing-glyph markdown");

    let result = markdown::markdown_to_pdf(test_md, test_pdf);
    assert!(result.is_ok(), "Failed to generate PDF: {:?}", result.err());

    let extracted = pdf::extract_text(test_pdf).expect("Failed to extract text");
    assert!(
        extracted.contains("你好"),
        "Expected CJK to survive, got: {}",
        extracted
    );
    assert!(
        extracted.contains('€'),
        "Expected supported currency to survive, got: {}",
        extracted
    );
    assert!(
        extracted.contains('?'),
        "Expected missing glyphs to become '?', got: {}",
        extracted
    );
    assert!(
        !extracted.contains('😀') && !extracted.contains('₿'),
        "Missing glyphs should not round-trip as themselves, got: {}",
        extracted
    );

    fs::remove_file(test_md).ok();
}

#[test]
fn test_octal_escape_sequences() {
    use pdfrs::pdf::unescape_pdf_string;

    assert_eq!(unescape_pdf_string(r"\101"), "A");
    assert_eq!(unescape_pdf_string(r"\102"), "B");
    assert_eq!(unescape_pdf_string(r"\103"), "C");
    assert_eq!(unescape_pdf_string(r"\60"), "0");
    assert_eq!(unescape_pdf_string(r"\61"), "1");

    assert_eq!(unescape_pdf_string(r"\141\142\143"), "abc");

    assert_eq!(unescape_pdf_string(r"Test\40String"), "Test String");
}

#[test]
fn test_utf16be_surrogate_pairs() {
    use pdfrs::pdf::decode_pdf_hex_string;

    let emoji_hex = "FEFFD83DDE00";
    let result = decode_pdf_hex_string(emoji_hex);
    assert_eq!(result, "😀");

    let emoji_hex2 = "FEFFD83DDE01";
    let result2 = decode_pdf_hex_string(emoji_hex2);
    assert_eq!(result2, "😁");
}
