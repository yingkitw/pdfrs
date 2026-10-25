//! PDF document model, parsing, text extraction, and structural diff
//!
//! Provides [`PdfDocument`] for loading PDFs from files or bytes,
//! navigating objects, extracting text with [`PdfDocument::get_text`], validating
//! structure, and comparing documents with [`diff_pdf_bytes`].

use std::sync::OnceLock;

macro_rules! pdf_regex {
    ($name:ident, $pat:literal) => {
        fn $name() -> &'static regex::Regex {
            static RE: OnceLock<regex::Regex> = OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

pdf_regex!(re_root, r"/Root\s+(\d+)\s+\d+\s+R");
pdf_regex!(re_root_any, r"/Root\s+\d+\s+\d+\s+R");
pdf_regex!(re_obj_ref, r"^(\d+) (\d+) R$");
pdf_regex!(re_obj, r"(\d+)\s+(\d+)\s+obj\b");
pdf_regex!(re_obj_count, r"\d+\s+\d+\s+obj\b");
pdf_regex!(re_tj, r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)\s*Tj");
pdf_regex!(re_tj_hex, r"<([0-9a-fA-F\s]+)>\s*Tj");
pdf_regex!(re_tj_array, r"\[((?:[^\]]*?))\]\s*TJ");
pdf_regex!(re_tj_str, r"\(((?:[^()\\]|\\.|(?:\([^()]*\)))*)\)");
pdf_regex!(re_tj_hex_str, r"<([0-9a-fA-F\s]+)>");
pdf_regex!(re_td, r"([\d.\-]+)\s+([\d.\-]+)\s+T[dD]");
pdf_regex!(
    re_tm,
    r"[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+[\d.\-]+\s+([\d.\-]+)\s+([\d.\-]+)\s+Tm"
);
pdf_regex!(re_page, r"/Type\s+/Page[^s]");
pdf_regex!(re_page_eol, r"/Type\s+/Page\s*\n");

mod decode;
mod diff;
mod objects;
mod parser;
mod sandbox;
mod text_extract;
/// Structural and compliance validation (PDF, PDF/A, PDF/UA, screen reader).
///
/// Public items are re-exported below so existing `crate::pdf::validate_*`
/// call sites continue to work without changes.
mod validation;

pub(crate) use decode::decode_pdf_hex_string_with_map;
pub use decode::{decode_pdf_hex_string, decode_with_encoding, unescape_pdf_string};
pub use diff::{PdfDiff, diff_pdf_bytes};
pub use objects::{PdfDocument, PdfObject, PdfValue};
pub use parser::{LazyPdfDocument, parse_object_stream, parse_xref_stream};
pub use sandbox::{JavaScriptAction, JavaScriptSandboxReport, sandbox_pdf_bytes};
pub(crate) use text_extract::collect_tounicode_gid_map;
pub use text_extract::extract_text;
pub use validation::{
    PdfAValidation, PdfUaValidation, PdfValidation, ScreenReaderComplianceReport,
    check_screen_reader_compliance, check_screen_reader_compliance_bytes, validate_pdf,
    validate_pdf_a, validate_pdf_a_bytes, validate_pdf_a3, validate_pdf_a3_bytes,
    validate_pdf_bytes, validate_pdf_ua, validate_pdf_ua_bytes,
};
