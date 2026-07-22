# Capability Showcase — pdfrs Validation Document

This fixture exercises a wide range of pdfrs features in one Markdown source:
headings (bookmarks), rich text, lists, tasks, tables, code, math, callouts
(plugin), quotes, rules, page breaks, footnotes, definition lists, links,
images references, and mixed-script RTL content.

## 1. Document Structure

Top-level sections become PDF outline bookmarks (`/Outlines`).

### 1.1 Nested Section

Nested headings should appear as separate outline entries.

#### Deep heading level 4

Content under a deeper heading.

## 2. Rich Text Formatting

A paragraph with **bold**, *italic*, ***bold italic***, `inline code`,
~~strikethrough~~, and a [named link](https://example.com/docs).

Mixed ASCII and symbols: alpha, beta, gamma — keep the body mostly Latin so
default generation stays lean while still covering formatting.

## 3. Lists and Tasks

### Unordered

- Alpha item
- Beta item
  - Nested beta-1
  - Nested beta-2
- Gamma item

### Ordered

1. First step
2. Second step
3. Third step

### Tasks

- [x] Ship plugin system
- [x] Ship linearized PDF
- [ ] Full SVG document rendering
- [ ] Incremental PDF saves

## 4. Tables

| Feature | Status | Priority |
|:--------|:------:|---------:|
| Markdown → PDF | Done | High |
| Bookmarks | Done | High |
| Linearize | Done | Medium |
| Callouts | Done | Medium |

## 5. Code Blocks

```rust
fn validate_capability(pdf: &[u8]) -> bool {
    pdf.starts_with(b"%PDF") && pdf.windows(4).any(|w| w == b"%%EOF")
}
```

```python
def fib(n):
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a
```

## 6. Mathematics

Inline: the identity $E = mc^2$ appears in prose.

Block:

$$
\int_{0}^{1} x^{2}\,dx = \frac{1}{3}
$$

Another block with sets: $$A \subseteq B \cup C$$

## 7. Callouts (plugin)

:::note
Callout plugins expand fenced notes into labeled paragraphs.
:::

:::warning
Do not skip structural validation after generation.
:::

:::tip
Use `--plugins callouts` with `md-to-pdf`.
:::

:::danger
Unsigned PDFs are not authenticity proofs.
:::

## 8. Quotes, Rules, Definitions

> A blockquote for cited material.
>> Nested quote level two.

---

Term Alpha
: Definition of term alpha for glossary-style layout.

Term Beta
: Definition of term beta.

[^cap]: Capability footnotes are stripped to markers in body text.

See the capability note[^cap] above.

## 9. Page Break and Continuation

<!-- pagebreak -->

# Continuation After Break

This heading starts a new page and should still bookmark correctly.

## 10. RTL Samples (dedicated lines)

Hebrew (RTL-dominant line for auto-detection tests):

שלום

Arabic (RTL-dominant line):

مرحبا

English after RTL should remain left-to-right.

## 11. Closing Checklist

- Headings → outlines
- Callouts → plugin transform
- Tables / code / math rendered
- Multi-page via page break
- Unicode + RTL preserved enough for extraction checks

End of capability showcase.
