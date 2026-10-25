//! Signature inspection and verification.

use std::fs;

use super::super::extract_pdf_dict_value;
use super::certificates::parse_cert_hex_metadata;
use super::regexes::re_sig_obj;
use crate::error::Result;

/// Information about a detected digital signature in a PDF
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureInfo {
    /// Name of the signer
    pub signer_name: String,
    /// Reason for signing
    pub reason: Option<String>,
    /// Signing location
    pub location: Option<String>,
    /// Signing date
    pub date: Option<String>,
    /// Byte range string
    pub byte_range: Option<String>,
    /// Certificate subject from embedded `/Cert` entry
    pub certificate_subject: Option<String>,
    /// SHA-256 fingerprint of embedded certificate DER
    pub certificate_fingerprint: Option<String>,
    /// Whether the signature is cryptographically valid.
    ///
    /// `true` means the `/Contents` held a CMS `SignedData` whose signature
    /// verified against an embedded certificate over the `/ByteRange` ranges
    /// (no trust-chain evaluation). `false` means no CMS container was found
    /// (e.g. the legacy [`sign_pdf`](crate::pdf_ops::sign_pdf) placeholder) or validation failed.
    pub valid: bool,
}

/// Verify the digital signatures in a PDF.
///
/// Checks for the presence of signature fields and reports basic signature
/// metadata. When `/Contents` holds a CMS `SignedData` container with an
/// embedded certificate (as produced by [`sign_pdf_cms`](crate::pdf_ops::sign_pdf_cms)), the signature is
/// **cryptographically validated**: the digest is recomputed over the
/// `/ByteRange` ranges and verified against the embedded certificate's public
/// key (RSA PKCS#1 v1.5 or ECDSA P-256, SHA-256).
///
/// For non-CMS contents (e.g. signatures produced by [`sign_pdf`](crate::pdf_ops::sign_pdf)) no
/// cryptographic check is possible and [`SignatureInfo::valid`](crate::pdf_ops::SignatureInfo::valid) stays
/// `false`. Note that `valid: false` does not prove tampering for such
/// legacy placeholder signatures, and a `true` value validates the signature
/// over the covered byte ranges — it does not build a trust chain to a
/// trusted root CA.
pub fn verify_pdf_signature(input_file: &str) -> Result<Vec<SignatureInfo>> {
    let pdf_bytes = fs::read(input_file)?;
    verify_pdf_signature_bytes(&pdf_bytes)
}

/// In-memory variant of [`verify_pdf_signature`].
pub fn verify_pdf_signature_bytes(pdf_bytes: &[u8]) -> Result<Vec<SignatureInfo>> {
    let text = String::from_utf8_lossy(pdf_bytes);
    let mut results = Vec::new();

    // Find all "N 0 obj" blocks and check for signature dictionaries
    // Use [\s\S] instead of . to match newlines inside dictionary content
    let obj_re = re_sig_obj();
    for caps in obj_re.captures_iter(&text) {
        let dict_content = &caps[2];
        if dict_content.contains("/Type /Sig") || dict_content.contains("/Type/Sig") {
            let name = extract_pdf_dict_value(dict_content, "/Name").unwrap_or_default();
            let reason = extract_pdf_dict_value(dict_content, "/Reason");
            let location = extract_pdf_dict_value(dict_content, "/Location");
            let date = extract_pdf_dict_value(dict_content, "/M");
            let byte_range = extract_pdf_dict_value(dict_content, "/ByteRange");
            let contents_hex = extract_pdf_dict_value(dict_content, "/Contents");
            let cert_hex = extract_pdf_dict_value(dict_content, "/Cert");
            let (certificate_subject, certificate_fingerprint) = cert_hex
                .as_ref()
                .and_then(|hex| parse_cert_hex_metadata(hex))
                .map(|(subject, fp)| (Some(subject), Some(fp)))
                .unwrap_or((None, None));

            // Cryptographic validation when a CMS container is present.
            let valid = if let (Some(range), Some(contents)) = (&byte_range, &contents_hex) {
                #[cfg(feature = "sign")]
                let valid = crate::pdf_ops::signing::try_cryptographic_validation(
                    pdf_bytes,
                    Some(range.as_str()),
                    contents,
                )?
                .unwrap_or_default();
                #[cfg(not(feature = "sign"))]
                let valid = {
                    let _ = (range, contents);
                    false
                };
                valid
            } else {
                false
            };

            results.push(SignatureInfo {
                signer_name: name,
                reason,
                location,
                date,
                byte_range,
                certificate_subject,
                certificate_fingerprint,
                valid,
            });
        }
    }

    Ok(results)
}
