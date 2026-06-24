use pdfrs::pdf_ops::watermark_pdf;

fn main() -> anyhow::Result<()> {
    let input = "examples/output/basic.pdf";
    let output = "examples/output/watermarked.pdf";

    watermark_pdf(input, output, "CONFIDENTIAL", 48.0, 0.3)?;
    println!("Added watermark to {} -> {}", input, output);
    Ok(())
}
