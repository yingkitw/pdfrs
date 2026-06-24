use pdfrs::elements;
use pdfrs::pdf_generator::{generate_pdf_bytes, PageLayout};
use std::fs;

fn main() -> anyhow::Result<()> {
    let markdown = r#"# Hello PDF

This is a basic example of generating a PDF from Markdown using the **pdfrs** library.

## Features

- Headings
- Paragraphs
- *Italic* and **bold** text
- Bullet lists
  - Nested items

## Code Block

```rust
fn hello() {
    println!("Hello from pdfrs!");
}
```

> A block quote for emphasis.
"#;

    let elements = elements::parse_markdown(markdown);
    let layout = PageLayout::portrait();
    let pdf_bytes = generate_pdf_bytes(&elements, "Helvetica", 12.0, layout)?;

    fs::write("examples/output/basic.pdf", &pdf_bytes)?;
    println!("Generated examples/output/basic.pdf ({} bytes)", pdf_bytes.len());
    Ok(())
}
