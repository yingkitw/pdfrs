//! Extracting certificates embedded in signed PDFs.

use std::fs;

use super::super::extract_pdf_dict_value;
use super::regexes::re_sig_obj;
use crate::error::{PdfError, Result};

/// Extract embedded X.509 certificates from PDF signature dictionaries.
pub fn extract_certificates_from_pdf_bytes(
    data: &[u8],
) -> Result<Vec<crate::security::SigningCertificate>> {
    let text = String::from_utf8_lossy(data);
    let obj_re = re_sig_obj();
    let mut certs = Vec::new();
    let mut index = 0usize;

    for caps in obj_re.captures_iter(&text) {
        let dict_content = &caps[2];
        if (dict_content.contains("/Type /Sig") || dict_content.contains("/Type/Sig"))
            && let Some(hex) = extract_pdf_dict_value(dict_content, "/Cert")
            && let Ok(cert) = der_hex_to_certificate(&hex, index)
        {
            certs.push(cert);
            index += 1;
        }
    }

    Ok(certs)
}

/// Extract embedded certificates from a PDF file.
pub fn extract_certificates_from_pdf(
    input_file: &str,
) -> Result<Vec<crate::security::SigningCertificate>> {
    let data = fs::read(input_file)?;
    extract_certificates_from_pdf_bytes(&data)
}

fn der_hex_to_certificate(hex: &str, index: usize) -> Result<crate::security::SigningCertificate> {
    let cleaned: String = hex.chars().filter(|c| c.is_ascii_hexdigit()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return Err(PdfError::Crypto("Invalid certificate hex length".into()));
    }
    let der: Vec<u8> = cleaned
        .as_bytes()
        .chunks(2)
        .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| PdfError::Crypto(e.to_string()))?;
    let b64 = encode_base64(&der);
    let pem = format!("-----BEGIN CERTIFICATE-----\n{b64}\n-----END CERTIFICATE-----\n");
    crate::security::parse_certificate_pem(format!("cert-{index}"), &pem)
}

fn encode_base64(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let triple = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub(super) fn parse_cert_hex_metadata(hex: &str) -> Option<(String, String)> {
    let cert = der_hex_to_certificate(hex, 0).ok()?;
    Some((cert.subject, cert.fingerprint_sha256))
}
