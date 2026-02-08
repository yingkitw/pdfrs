# Complex Markdown Test Document

This is a **comprehensive test document** designed to validate the PDF roundtrip functionality of pdf-rs.

## Text Formatting Examples

This paragraph demonstrates **bold text** and *italic text*. We can also have ***bold and italic*** together.

Here's some `inline code` within a sentence.

## Headers of Different Levels

### Level 3 Header

Some content under level 3.

#### Level 4 Header

More content here.

##### Level 5 Header

Even deeper nesting.

###### Level 6 Header

The smallest header.

## Lists

### Unordered List

- First item
- Second item with **bold text**
- Third item with `inline code`
- Fourth item
  - Nested item A
  - Nested item B
- Fifth item

### Ordered List

1. First step
2. Second step
3. Third step with *emphasis*
4. Fourth step
   1. Nested step 4.1
   2. Nested step 4.2
5. Fifth step

## Tables

| Name | Age | City | Occupation |
|------|-----|------|------------|
| Alice | 28 | New York | Engineer |
| Bob | 35 | San Francisco | Designer |
| Charlie | 42 | Boston | **Manager** |
| Diana | 31 | Seattle | Developer |

### Feature Comparison Table

| Feature | Supported | Notes |
|---------|-----------|-------|
| Bold text | Yes | `**text**` |
| Italic text | Yes | `*text*` |
| Code blocks | Yes | Triple backticks |
| Tables | Yes | Pipe syntax |
| Lists | Yes | Ordered and unordered |

## Code Blocks

Here is a Rust code example:

```rust
fn main() {
    println!("Hello, World!");
    let x = 42;
    let y = x * 2;
    println!("The answer is: {}", y);
}
```

And here is some Python code:

```python
def calculate_sum(numbers):
    total = 0
    for num in numbers:
        total += num
    return total

result = calculate_sum([1, 2, 3, 4, 5])
print(f"The sum is: {result}")
```

## Links and References

This is a link to [Rust Documentation](https://doc.rust-lang.org/).

You can also have [links with **formatting**](https://example.com) inside them.

## Special Characters and Punctuation

The following special characters should be rendered correctly:

- Quotation marks: "double" and 'single'
- Dashes: - (en dash), -- (em dash)
- Ellipsis: ...
- Ampersand: &
- At symbol: @
- Hash: #
- Dollar: $
- Percent: %
- Underscore: _

## Mathematical and Technical Content

The formula for calculating the area of a circle is: A = πr²

Some common mathematical symbols:
- Plus: +, Minus: -, Multiply: ×, Divide: ÷
- Less than: <, Greater than: >
- Equals: =, Not equals: ≠

## Lorem Ipsum Section

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.

### More Details

Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident.

## Mixed Formatting Test

This paragraph tests ***everything*** together: **bold**, *italic*, `code`, and [links](https://example.com) all in one place.

We can also have **bold with `code` inside** it.

And *italic with **bold** inside* it.

## Summary

This document contains:
- Headers at all 6 levels
- Bold and italic text
- Ordered and unordered lists
- Tables with multiple columns
- Code blocks with syntax highlighting
- Links
- Special characters
- Mixed formatting

---

*End of test document*
