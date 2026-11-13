//! Python bindings (PyPI: `pip install pdfrs`) via PyO3 / maturin.

use crate::pdf::validate_pdf_bytes as validate_pdf_bytes_rust;
use crate::pdf_ops::merge_pdfs_from_bytes;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

fn map_err(err: crate::PdfError) -> PyErr {
    PyValueError::new_err(err.to_string())
}

fn font_defaults(font: Option<&str>, font_size: Option<f32>) -> (&str, f32) {
    (font.unwrap_or("Helvetica"), font_size.unwrap_or(12.0))
}

#[pyfunction]
fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[pyfunction]
#[pyo3(signature = (markdown, font=None, font_size=None))]
fn render_markdown_to_pdf(
    markdown: &str,
    font: Option<&str>,
    font_size: Option<f32>,
) -> PyResult<Vec<u8>> {
    let elements = crate::elements::parse_markdown(markdown);
    let layout = crate::pdf_generator::PageLayout::portrait();
    let (font, font_size) = font_defaults(font, font_size);
    crate::pdf_generator::generate_pdf_bytes(&elements, font, font_size, layout).map_err(map_err)
}

#[pyfunction]
#[pyo3(signature = (html, font=None, font_size=None))]
fn html_to_pdf_bytes(html: &str, font: Option<&str>, font_size: Option<f32>) -> PyResult<Vec<u8>> {
    let (font, font_size) = font_defaults(font, font_size);
    crate::html::html_to_pdf_bytes(html, font, font_size).map_err(map_err)
}

#[pyfunction]
fn merge_pdfs(pdfs: Vec<Vec<u8>>) -> PyResult<Vec<u8>> {
    merge_pdfs_from_bytes(&pdfs).map_err(map_err)
}

#[pyfunction]
#[pyo3(name = "validate_pdf_bytes")]
fn validate_pdf_bytes(py: Python<'_>, data: &[u8]) -> PyResult<PyObject> {
    let v = validate_pdf_bytes_rust(data);
    let dict = PyDict::new(py);
    dict.set_item("valid", v.valid)?;
    dict.set_item("errors", v.errors)?;
    dict.set_item("warnings", v.warnings)?;
    dict.set_item("page_count", v.page_count)?;
    dict.set_item("object_count", v.object_count)?;
    Ok(dict.into())
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(render_markdown_to_pdf, m)?)?;
    m.add_function(wrap_pyfunction!(html_to_pdf_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(merge_pdfs, m)?)?;
    m.add_function(wrap_pyfunction!(validate_pdf_bytes, m)?)?;
    Ok(())
}
