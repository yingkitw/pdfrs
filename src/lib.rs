pub mod compression;
pub mod elements;
pub mod image;
pub mod markdown;
pub mod pdf;
pub mod pdf_generator;
pub mod pdf_ops;

#[cfg(test)]
mod tests {
    use crate::markdown::markdown_to_text;

    #[test]
    fn test_markdown_to_text() {
        let markdown = "# Header\n\nThis is **bold** and *italic* text.\n\n- Item 1\n- Item 2";
        let expected = "Header\nThis is bold and italic text.\n• Item 1\n• Item 2\n";
        let result = markdown_to_text(markdown);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_markdown_table_to_text() {
        let markdown = "| Name | Age |\n|------|-----|\n| John | 25  |\n| Jane | 30  |";
        let expected = "Name  Age  \n------  -----  \nJohn  25  \nJane  30  \n";
        let result = markdown_to_text(markdown);
        assert_eq!(result, expected);
    }

    #[test]
    fn test_markdown_code_blocks() {
        let markdown = "Here is some code:\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nMore text.";
        let expected =
            "Here is some code:\n\nfn main() {\n    println!(\"Hello\");\n}\n\nMore text.\n";
        let result = markdown_to_text(markdown);
        assert_eq!(result, expected);
    }
}
