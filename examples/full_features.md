# PDF-RS Full Feature Showcase

This document exercises every supported element type in the pdf-rs library.

## Text Formatting

Regular paragraph with plain text content that spans multiple words.

This paragraph contains **bold text**, *italic text*, and ***bold italic*** together. It also has `inline code` and ~~strikethrough~~ text.

## Lists

### Unordered Lists

- First item at depth zero
- Second item at depth zero
  - Nested item at depth one
  - Another nested item
    - Deep nested item at depth two
- Back to depth zero

### Ordered Lists

1. First numbered item
2. Second numbered item
3. Third numbered item

### Task Lists

- [x] Completed task one
- [x] Completed task two
- [ ] Pending task three
- [ ] Pending task four

## Code Blocks

Here is a Rust code example:

```rust
fn fibonacci(n: u64) -> u64 {
    match n {
        0 => 0,
        1 => 1,
        _ => fibonacci(n - 1) + fibonacci(n - 2),
    }
}

fn main() {
    for i in 0..10 {
        println!("fib({}) = {}", i, fibonacci(i));
    }
}
```

And a Python example:

```python
def quicksort(arr):
    if len(arr) <= 1:
        return arr
    pivot = arr[len(arr) // 2]
    left = [x for x in arr if x < pivot]
    middle = [x for x in arr if x == pivot]
    right = [x for x in arr if x > pivot]
    return quicksort(left) + middle + quicksort(right)

print(quicksort([3, 6, 8, 10, 1, 2, 1]))
```

## Tables

| Feature | Status | Priority |
|:--------|:------:|--------:|
| PDF Generation | Done | High |
| Text Extraction | Done | High |
| Image Support | Done | Medium |
| Watermarks | Done | Low |

## Blockquotes

> This is a simple blockquote with important information.

> First level quote
>> Nested quote inside the first
>>> Triple nested quote for emphasis

## Definition Lists

Rust
: A systems programming language focused on safety and performance

PDF
: Portable Document Format, a file format for presenting documents

API
: Application Programming Interface

## Footnotes

This document demonstrates footnote support[^1]. Multiple footnotes can be used throughout the text[^2].

[^1]: This is the first footnote with detailed explanation.
[^2]: This is the second footnote referencing additional material.

## Links and Images

[Visit the Rust website](https://www.rust-lang.org)

[PDF Specification](https://www.adobe.com/devnet/pdf/pdf_reference.html)

![Rust Logo](images/rust-logo.png)

![PDF Icon](images/pdf-icon.svg)

## Horizontal Rules

Content above the rule.

---

Content below the rule.

***

Another section after a different rule style.

## Page Breaks

This content appears before the page break.

<!-- pagebreak -->

This content appears on a new page after the break.

## Mixed Content Section

Here is a complex section combining multiple elements:

1. **Step One**: Initialize the project
   - Create directory structure
   - Set up dependencies
2. **Step Two**: Implement core features
   - PDF parsing engine
   - Content stream builder
3. **Step Three**: Add tests
   - Unit tests for each module
   - Integration tests for workflows

> **Note**: Always run `cargo test` before committing changes.

| Module | Tests | Coverage |
|--------|------:|:--------:|
| pdf.rs | 18 | 85% |
| elements.rs | 22 | 92% |
| pdf_generator.rs | 15 | 78% |
| pdf_ops.rs | 20 | 88% |

```bash
cargo test --all
cargo build --release
./target/release/pdf-cli md-to-pdf examples/full_features.md output.pdf
```

## Summary

This document has demonstrated all 17 element types supported by pdf-rs:

- [x] Heading (H1 through H3)
- [x] Paragraph
- [x] UnorderedListItem (with nesting)
- [x] OrderedListItem
- [x] TaskListItem (checked and unchecked)
- [x] CodeBlock (multiple languages)
- [x] TableRow (with alignment)
- [x] BlockQuote (with nesting)
- [x] DefinitionItem
- [x] Footnote
- [x] Link
- [x] Image
- [x] HorizontalRule
- [x] PageBreak
- [x] EmptyLine

[^1]: First footnote definition.
[^2]: Second footnote definition.
