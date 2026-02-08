# Enhanced Markdown Features Demo

This document tests all the **new Phase 2 features** added to pdf-cli.

## Task Lists

### Sprint Backlog

- [x] Implement structured element parser
- [x] Add header font size variations
- [x] Add page numbering support
- [ ] Implement image embedding
- [ ] Add PDF encryption
- [x] Write integration tests

### Release Checklist

- [x] All unit tests passing
- [x] Roundtrip validation complete
- [ ] Performance benchmarks run
- [ ] Documentation updated
- [x] Code review approved

## Blockquotes

> This is a simple blockquote. It should be rendered with proper indentation.

> "The best way to predict the future is to invent it." — Alan Kay

>> This is a nested blockquote. It goes deeper.

>>> Triple-nested blockquote for emphasis.

> Code and quotes work together:
> The function `parse_markdown` handles all element types.

## Strikethrough Text

The old API used ~~synchronous calls~~ asynchronous streams for better performance.

We ~~removed~~ replaced the legacy parser with a structured element system.

## Heading Hierarchy

# Level 1: Main Title

## Level 2: Section

### Level 3: Subsection

#### Level 4: Detail

##### Level 5: Fine Detail

###### Level 6: Smallest

## Complex Nested Lists

- Architecture decisions
  - Use Rust for core engine
    - Performance critical paths
    - Memory safety guarantees
  - Use clap for CLI
- Testing strategy
  - Unit tests per module
  - Integration tests for roundtrip
  - Property-based tests (planned)

### Ordered with Nesting

1. Parse markdown input
2. Build element tree
   1. Identify block elements
   2. Parse inline formatting
   3. Handle special syntax
3. Generate PDF content streams
4. Assemble final PDF
   1. Create page objects
   2. Build page tree
   3. Write xref and trailer

## Tables with Mixed Content

| Feature | Status | Priority | Notes |
|---------|--------|----------|-------|
| Task lists | Done | High | Checkbox rendering |
| Blockquotes | Done | High | Nested support |
| Strikethrough | Done | Medium | Inline formatting |
| Page numbers | Done | High | Footer placement |
| Header sizes | Done | High | H1-H6 scaling |
| Code blocks | Done | Medium | Reduced font size |

## Code Examples

### Rust Element Parser

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum Element {
    Heading { level: u8, text: String },
    Paragraph { text: String },
    TaskListItem { checked: bool, text: String },
    BlockQuote { text: String, depth: u8 },
    HorizontalRule,
}

fn parse_markdown(input: &str) -> Vec<Element> {
    let mut elements = Vec::new();
    for line in input.lines() {
        elements.push(parse_line(line));
    }
    elements
}
```

### Python Test Script

```python
import subprocess
import difflib

def test_roundtrip(md_file: str) -> dict:
    pdf_file = md_file.replace('.md', '.pdf')
    out_file = md_file.replace('.md', '_roundtrip.md')
    
    subprocess.run(['pdf-cli', 'md-to-pdf', md_file, pdf_file])
    subprocess.run(['pdf-cli', 'pdf-to-md', pdf_file, out_file])
    
    with open(md_file) as f:
        original = f.read()
    with open(out_file) as f:
        roundtrip = f.read()
    
    diff = difflib.unified_diff(
        original.splitlines(),
        roundtrip.splitlines(),
        lineterm=''
    )
    return {'diff_lines': list(diff)}
```

## Horizontal Rules

Content above the rule.

---

Content between rules.

---

Content below the rule.

## Mixed Formatting Paragraph

This paragraph combines **bold text**, *italic text*, `inline code`, ~~strikethrough~~, and [links](https://example.com) all together in a single flowing paragraph to test the inline formatting stripping.

## Summary

All enhanced features are now functional:

1. Task lists with checkboxes
2. Blockquotes with nesting
3. Strikethrough text
4. Header font size hierarchy
5. Page numbering
6. Horizontal rules
7. Nested lists with indentation
8. Code blocks with reduced font size

---

*Document generated to validate Phase 2 features of pdf-cli.*
