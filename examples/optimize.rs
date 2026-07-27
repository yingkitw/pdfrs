use pdfrs::optimization::{OptimizationProfile, optimize_pdf_bytes};
use std::fs;

fn main() -> anyhow::Result<()> {
    let input_path = "examples/output/basic.pdf";
    let output_path = "examples/output/optimized_web.pdf";

    let pdf_data = fs::read(input_path)?;
    let original_size = pdf_data.len();

    let settings = OptimizationProfile::Web.settings();
    let optimized = optimize_pdf_bytes(&pdf_data, settings)?;

    fs::write(output_path, &optimized)?;

    let new_size = optimized.len();
    let savings = if original_size > 0 {
        ((original_size - new_size) as f64 / original_size as f64) * 100.0
    } else {
        0.0
    };

    println!(
        "Optimized {} -> {} ({} bytes -> {} bytes, {:.1}% reduction)",
        input_path, output_path, original_size, new_size, savings
    );
    Ok(())
}
