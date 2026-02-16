use pdfrs::markdown;
use std::fs;
use std::path::Path;

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
