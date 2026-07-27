use pdfrs::pdf_ops::merge_pdfs;

fn main() -> anyhow::Result<()> {
    let inputs = vec!["examples/output/basic.pdf", "examples/output/basic.pdf"];

    merge_pdfs(&inputs, "examples/output/merged.pdf")?;
    println!(
        "Merged {} PDFs into examples/output/merged.pdf",
        inputs.len()
    );
    Ok(())
}
