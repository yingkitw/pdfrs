//! PDF security: password protection, digital signatures, and certificate extraction.

use anyhow::{Result, anyhow};
use std::fs;

use sha2::{Digest, Sha256};

/// Apply password protection and permissions to a PDF.
///
/// This function adds security settings to a PDF document, including password protection
/// and permission restrictions. Note that this is a simplified implementation that adds
/// the encryption dictionary to the PDF trailer. For production use, you would need
/// proper cryptographic libraries (like RustCrypto or openssl) for actual encryption.
///
/// # Arguments
///
/// * `input_file` - Path to the input PDF file
/// * `output_file` - Path where the protected PDF will be written
/// * `security` - Security settings including passwords and permissions
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if protection fails.
///
/// # Example
///
/// ```rust,no_run
/// use pdfrs::{pdf_ops, security};
///
/// let sec = security::PdfSecurity::new()
///     .with_user_password("secret".to_string())
///     .with_permissions(security::PdfPermissions::read_only());
///
/// pdf_ops::protect_pdf("input.pdf", "protected.pdf", &sec)
///     .expect("Failed to protect PDF");
/// ```
///
/// # Errors
///
/// This function will return an error if:
/// - The input file cannot be read
/// - The security settings are invalid
/// - Writing the output file fails
pub fn protect_pdf(
    input_file: &str,
    output_file: &str,
    security: &crate::security::PdfSecurity,
) -> Result<()> {
    security.validate()?;

    // Honest gate: do not write fake "protected" PDFs that remain plaintext.
    if security.is_protected() {
        // Fail early before touching files when encryption is requested.
        let _ = security.create_encryption_dict()?;
        return Err(anyhow!(
            "Password protection is not implemented yet; refusing to write an unprotected PDF that claims encryption"
        ));
    }

    // No passwords configured — copy through unchanged.
    let content = fs::read(input_file)?;
    fs::write(output_file, content)?;
    Ok(())
}

/// Add a digital signature to a PDF document.
///
/// This creates the PDF signature field structure and computes a SHA-256
/// content digest over the signed byte ranges. The actual PKCS#7/CMS
/// container is stored as a placeholder; external tools can replace it.
///
/// # Arguments
/// * `input_file` - Path to the original PDF
/// * `output_file` - Path for the signed PDF output
/// * `signature` - Digital signature metadata (signer, reason, location, etc.)
///
/// # Example
/// ```no_run
/// use pdfrs::{security::DigitalSignature, pdf_ops::sign_pdf};
///
/// let sig = DigitalSignature::new("Alice")
///     .with_reason("I approve this document")
///     .with_location("New York");
/// sign_pdf("input.pdf", "signed.pdf", &sig).unwrap();
/// ```
pub fn sign_pdf(
    input_file: &str,
    output_file: &str,
    signature: &crate::security::DigitalSignature,
) -> Result<()> {
    sign_pdf_with_certificate(input_file, output_file, signature, None)
}

/// Sign a PDF and optionally embed an X.509 certificate in the signature dictionary.
pub fn sign_pdf_with_certificate(
    input_file: &str,
    output_file: &str,
    signature: &crate::security::DigitalSignature,
    certificate: Option<&crate::security::SigningCertificate>,
) -> Result<()> {
    let pdf_bytes = fs::read(input_file)?;

    // Build incremental update with signature objects
    let sig = signature.clone();

    // Placeholder for signature contents (8192 hex chars = 4096 bytes)
    let contents_placeholder = "0".repeat(8192);

    // Build signature dictionary with placeholder
    let mut sig_dict = format!(
        "<< /Type /Sig\n\
         /Filter /Adobe.PPKLite\n\
         /SubFilter /adbe.pkcs7.detached\n\
         /Contents <{}>\n\
         /ByteRange [0 0 0 0]\n",
        contents_placeholder
    );
    if let Some(ref date) = sig.date {
        sig_dict.push_str(&format!(" /M (D:{})\n", super::escape_pdf_meta(date)));
    }
    sig_dict.push_str(&format!(
        " /Name ({})\n",
        super::escape_pdf_meta(&sig.signer_name)
    ));
    if let Some(ref reason) = sig.reason {
        sig_dict.push_str(&format!(" /Reason ({})\n", super::escape_pdf_meta(reason)));
    }
    if let Some(ref location) = sig.location {
        sig_dict.push_str(&format!(
            " /Location ({})\n",
            super::escape_pdf_meta(location)
        ));
    }
    if let Some(ref contact) = sig.contact_info {
        sig_dict.push_str(&format!(
            " /ContactInfo ({})\n",
            super::escape_pdf_meta(contact)
        ));
    }
    if let Some(cert) = certificate {
        let der_hex = crate::security::certificate_pem_to_der_hex(&cert.pem)?;
        sig_dict.push_str(&format!(" /Cert <{}>\n", der_hex));
    }
    sig_dict.push_str(">>");

    // Rebuild with proper PDF objects
    let original_len = pdf_bytes.len();
    let mut output = pdf_bytes.clone();

    // Find the last %%EOF
    let last_eof = output.windows(5).rposition(|w| w == b"%%EOF").unwrap_or(0);
    let startxref_pos = output[..last_eof]
        .windows(9)
        .rposition(|w| w == b"startxref")
        .unwrap_or(0);
    let xref_offset: usize = String::from_utf8_lossy(&output[startxref_pos + 9..last_eof])
        .trim()
        .parse()
        .unwrap_or(0);

    // Find catalog reference in trailer
    let trailer_end = output[startxref_pos..]
        .iter()
        .position(|&b| b == b'>')
        .unwrap_or(0);
    let trailer_text = String::from_utf8_lossy(&output[startxref_pos..startxref_pos + trailer_end]);
    let catalog_ref = trailer_text
        .lines()
        .find(|l| l.contains("/Root"))
        .and_then(|l| {
            l.split("/Root")
                .nth(1)?
                .split_whitespace()
                .next()
                .map(|s| s.trim())
        })
        .unwrap_or("");

    // Build incremental update
    let update_start = original_len;
    let mut update = Vec::new();

    // Signature dictionary object
    let sig_obj_num = 999; // Use high number to avoid conflicts
    let sig_dict_obj = format!("{} 0 obj\n{}\nendobj\n", sig_obj_num, sig_dict);
    update.extend_from_slice(sig_dict_obj.as_bytes());

    // Signature field (widget annotation + form field)
    let field_obj_num = sig_obj_num + 1;
    let field_dict = format!(
        "{} 0 obj\n<< /Type /Annot\n\
         /Subtype /Widget\n\
         /FT /Sig\n\
         /T (Signature1)\n\
         /V {} 0 R\n\
         /P 1 0 R\n\
         /Rect [0 0 0 0]\n\
         /F 132\n\
         >>\nendobj\n",
        field_obj_num, sig_obj_num
    );
    update.extend_from_slice(field_dict.as_bytes());

    // New catalog with /AcroForm
    let new_catalog_num = sig_obj_num + 2;
    let new_catalog = format!(
        "{} 0 obj\n<< /Type /Catalog\n\
         /Pages {}\n\
         /AcroForm << /Fields [{} 0 R] /SigFlags 3 >>\n\
         >>\nendobj\n",
        new_catalog_num,
        if catalog_ref.is_empty() {
            "1 0 R".to_string()
        } else {
            catalog_ref.to_string()
        },
        field_obj_num
    );
    update.extend_from_slice(new_catalog.as_bytes());

    // New trailer pointing to new catalog
    let xref_offset_new = update_start;
    let xref = format!(
        "xref\n\
         0 1\n\
         0000000000 65535 f \n\
         {} 3\n\
         {:010} 00000 n \n\
         {:010} 00000 n \n\
         {:010} 00000 n \n",
        sig_obj_num,
        xref_offset_new,
        xref_offset_new + sig_dict_obj.len(),
        xref_offset_new + sig_dict_obj.len() + field_dict.len()
    );
    update.extend_from_slice(xref.as_bytes());

    let trailer = format!(
        "trailer\n<< /Size {} /Root {} 0 R /Prev {} >>\nstartxref\n{}\n%%EOF\n",
        new_catalog_num + 1,
        new_catalog_num,
        xref_offset,
        update_start
    );
    update.extend_from_slice(trailer.as_bytes());

    // Append update to output
    output.extend_from_slice(&update);

    // Now compute byte range and content hash
    let full_output = output.clone();
    let contents_marker = format!("Contents <{}", contents_placeholder);
    let contents_start = full_output
        .windows(contents_marker.len())
        .position(|w| w == contents_marker.as_bytes())
        .ok_or_else(|| anyhow!("Could not find signature contents placeholder"))?;

    // ByteRange: [0, contents_start_of_value, contents_end_of_value, remaining]
    let value_start = contents_start + 1; // Point to '<' in "Contents <"
    let value_end = contents_start + contents_marker.len() + 1; // After '>'

    let byte_range = [
        0u32,
        value_start as u32,
        value_end as u32,
        (full_output.len() - value_end) as u32,
    ];

    // Compute SHA-256 over the byte ranges
    let mut hasher = Sha256::new();
    hasher.update(&full_output[0..value_start]);
    hasher.update(&full_output[value_end..]);
    let hash = hasher.finalize();
    let hash_hex = hash
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();

    // Replace placeholder with hash (pad with zeros to maintain length)
    let padded_hash = format!("{:0<width$}", hash_hex, width = contents_placeholder.len());
    let old_marker = format!("Contents <{}", contents_placeholder);
    let new_marker = format!("Contents <{}", padded_hash);
    let output_str = String::from_utf8_lossy(&full_output);
    let final_output = output_str.replace(&old_marker, &new_marker);

    // Replace ByteRange placeholder
    let final_output = final_output.replace(
        "/ByteRange [0 0 0 0]",
        &format!(
            "/ByteRange [{} {} {} {}]",
            byte_range[0], byte_range[1], byte_range[2], byte_range[3]
        ),
    );

    fs::write(output_file, final_output)?;

    println!(
        "[sign] Signed {} -> {} (signer: {}, hash: {})",
        input_file,
        output_file,
        sig.signer_name,
        &hash_hex[..16]
    );

    Ok(())
}

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
    /// Whether the signature is cryptographically valid (always false in this simplified check)
    pub valid: bool,
}

/// Verify that a PDF contains a digital signature structure.
///
/// This checks for the presence of signature fields and reports
/// basic signature metadata. It does NOT cryptographically verify
/// the signature against a certificate chain.
///
/// Returns a list of signature info found in the document.
pub fn verify_pdf_signature(input_file: &str) -> Result<Vec<SignatureInfo>> {
    let pdf_bytes = fs::read(input_file)?;
    let text = String::from_utf8_lossy(&pdf_bytes);
    let mut results = Vec::new();

    // Find all "N 0 obj" blocks and check for signature dictionaries
    // Use [\s\S] instead of . to match newlines inside dictionary content
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj").unwrap();
    for caps in obj_re.captures_iter(&text) {
        let dict_content = &caps[2];
        if dict_content.contains("/Type /Sig") || dict_content.contains("/Type/Sig") {
            let name = super::extract_pdf_dict_value(dict_content, "/Name").unwrap_or_default();
            let reason = super::extract_pdf_dict_value(dict_content, "/Reason");
            let location = super::extract_pdf_dict_value(dict_content, "/Location");
            let date = super::extract_pdf_dict_value(dict_content, "/M");
            let byte_range = super::extract_pdf_dict_value(dict_content, "/ByteRange");
            let cert_hex = super::extract_pdf_dict_value(dict_content, "/Cert");
            let (certificate_subject, certificate_fingerprint) = cert_hex
                .as_ref()
                .and_then(|hex| parse_cert_hex_metadata(hex))
                .map(|(subject, fp)| (Some(subject), Some(fp)))
                .unwrap_or((None, None));

            results.push(SignatureInfo {
                signer_name: name,
                reason,
                location,
                date,
                byte_range,
                certificate_subject,
                certificate_fingerprint,
                valid: false,
            });
        }
    }

    Ok(results)
}

/// Extract embedded X.509 certificates from PDF signature dictionaries.
pub fn extract_certificates_from_pdf_bytes(
    data: &[u8],
) -> Result<Vec<crate::security::SigningCertificate>> {
    let text = String::from_utf8_lossy(data);
    let obj_re = regex::Regex::new(r"(?s)(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj").unwrap();
    let mut certs = Vec::new();
    let mut index = 0usize;

    for caps in obj_re.captures_iter(&text) {
        let dict_content = &caps[2];
        if (dict_content.contains("/Type /Sig") || dict_content.contains("/Type/Sig"))
            && let Some(hex) = super::extract_pdf_dict_value(dict_content, "/Cert")
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
        return Err(anyhow!("Invalid certificate hex length"));
    }
    let der: Vec<u8> = cleaned
        .as_bytes()
        .chunks(2)
        .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16))
        .collect::<Result<Vec<_>, _>>()?;
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

fn parse_cert_hex_metadata(hex: &str) -> Option<(String, String)> {
    let cert = der_hex_to_certificate(hex, 0).ok()?;
    Some((cert.subject, cert.fingerprint_sha256))
}
