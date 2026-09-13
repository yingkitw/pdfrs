//! Typed error model for the pdfrs library.
//!
//! Every fallible library API returns [`Result`], whose error type is
//! [`PdfError`]. The enum is deliberately small: it classifies failures by
//! *domain* (parsing, crypto, images, …) rather than enumerating every
//! call site. [`PdfError::Context`] preserves the chain of underlying
//! causes, and `PdfError` implements [`std::error::Error`], so it converts
//! into `anyhow::Error`, `Box<dyn Error>`, and friends with a plain `?`.

use std::fmt;

/// Convenience alias used throughout the library.
pub type Result<T, E = PdfError> = std::result::Result<T, E>;

/// Errors produced by pdfrs library operations.
#[derive(Debug)]
pub enum PdfError {
    /// Filesystem or I/O failure.
    Io(std::io::Error),
    /// The input is not a PDF at all (bad header, missing `%%EOF`, …).
    InvalidPdf(String),
    /// The input is a PDF but could not be parsed (broken xref, truncated
    /// object, malformed dictionary, …).
    Parse(String),
    /// The requested page index does not exist in the document.
    PageNotFound(usize),
    /// Encryption, decryption, signing, or certificate failure.
    Crypto(String),
    /// Image decoding, encoding, or embedding failure.
    Image(String),
    /// SVG document or path parsing failure.
    Svg(String),
    /// A caller-supplied parameter is invalid (empty password, malformed
    /// region, unknown profile, …).
    InvalidInput(String),
    /// The input is valid but uses a feature pdfrs does not support.
    Unsupported(String),
    /// Additional context wrapped around an underlying [`PdfError`].
    Context {
        message: String,
        source: Box<PdfError>,
    },
    /// Anything that does not fit another variant (task panics, …).
    Other(String),
}

impl PdfError {
    /// Wraps `source` with additional human-readable `message` context.
    pub fn ctx(message: impl Into<String>, source: impl Into<PdfError>) -> Self {
        PdfError::Context {
            message: message.into(),
            source: Box::new(source.into()),
        }
    }

    /// Walks the [`PdfError::Context`] chain and returns the innermost error.
    pub fn root_cause(&self) -> &PdfError {
        let mut cur = self;
        while let PdfError::Context { source, .. } = cur {
            cur = source;
        }
        cur
    }
}

impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PdfError::Io(e) => write!(f, "I/O error: {}", e),
            PdfError::InvalidPdf(m) => write!(f, "invalid PDF: {}", m),
            PdfError::Parse(m) => write!(f, "PDF parse error: {}", m),
            PdfError::PageNotFound(page) => write!(f, "page {} not found (out of range)", page),
            PdfError::Crypto(m) => write!(f, "crypto error: {}", m),
            PdfError::Image(m) => write!(f, "image error: {}", m),
            PdfError::Svg(m) => write!(f, "SVG error: {}", m),
            PdfError::InvalidInput(m) => write!(f, "invalid input: {}", m),
            PdfError::Unsupported(m) => write!(f, "unsupported: {}", m),
            PdfError::Context { message, source } => {
                write!(f, "{}: {}", message, source)
            }
            PdfError::Other(m) => write!(f, "{}", m),
        }
    }
}

impl std::error::Error for PdfError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PdfError::Io(e) => Some(e),
            PdfError::Context { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<std::io::Error> for PdfError {
    fn from(e: std::io::Error) -> Self {
        PdfError::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_variant() {
        assert_eq!(
            PdfError::InvalidPdf("missing %%EOF".into()).to_string(),
            "invalid PDF: missing %%EOF"
        );
        assert_eq!(
            PdfError::PageNotFound(3).to_string(),
            "page 3 not found (out of range)"
        );
    }

    #[test]
    fn context_chain_and_root_cause() {
        let io = PdfError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no such file",
        ));
        let wrapped = PdfError::ctx("Failed to load doc.pdf", io);
        let s = wrapped.to_string();
        assert!(s.starts_with("Failed to load doc.pdf: I/O error:"), "{}", s);
        assert!(matches!(wrapped.root_cause(), PdfError::Io(_)));
        assert!(std::error::Error::source(&wrapped).is_some());
    }

    #[test]
    fn io_error_converts_with_question_mark() -> Result<()> {
        fn inner() -> Result<()> {
            Err(std::io::Error::other("boom"))?;
            Ok(())
        }
        assert!(matches!(inner(), Err(PdfError::Io(_))));
        Ok(())
    }

    #[test]
    fn converts_into_anyhow_and_box_dyn() {
        let e = PdfError::Crypto("bad key".into());
        let boxed: Box<dyn std::error::Error> = Box::new(e);
        assert!(boxed.to_string().contains("bad key"));
    }
}
