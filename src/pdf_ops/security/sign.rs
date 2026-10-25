//! Legacy structural signing.
//!
//! These functions insert a signature dictionary and a placeholder digest.
//! They do **not** perform a private-key operation — use
//! [`sign_pdf_cms`](crate::pdf_ops::sign_pdf_cms) for real cryptographic
//! signatures.

use std::fs;

use sha2::{Digest, Sha256};

use super::objects::{find_from, scan_pdf_objects};
use super::regexes::{re_acroform, re_catalog_obj};
use crate::error::{PdfError, Result};

/// Add a digital signature to a PDF document.
///
/// This creates the PDF signature field structure and computes a SHA-256
/// content digest over the signed byte ranges. The actual PKCS#7/CMS
/// container is stored as a placeholder; external tools can replace it.
///
/// # Warning — structural signature, not cryptographic
///
/// The output is **not** a cryptographically verifiable signature. The
/// `/Contents` holds a placeholder PKCS#7/CMS container (digest only, no
/// private-key signing), and [`verify_pdf_signature`](crate::pdf_ops::verify_pdf_signature) reports
/// [`SignatureInfo::valid`](crate::pdf_ops::SignatureInfo::valid) as `false` for it. Do not use this for legal,
/// compliance, or authentication purposes — use [`sign_pdf_cms`](crate::pdf_ops::sign_pdf_cms) with a
/// real private key instead.
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
///
/// Builds a proper incremental update: new objects are numbered past the
/// document's maximum, the original catalog is re-emitted with `/AcroForm`
/// added, and the new xref/trailer chain back via `/Prev`. The digest and
/// `/ByteRange` use fixed-width values so splicing never shifts offsets.
pub fn sign_pdf_with_certificate(
    input_file: &str,
    output_file: &str,
    signature: &crate::security::DigitalSignature,
    certificate: Option<&crate::security::SigningCertificate>,
) -> Result<()> {
    let pdf_bytes = fs::read(input_file)?;
    let signed = sign_pdf_bytes(&pdf_bytes, signature, certificate)?;
    fs::write(output_file, signed)?;
    println!(
        "[sign] Signed {input_file} -> {output_file} (signer: {})",
        signature.signer_name
    );
    Ok(())
}

/// In-memory variant of [`sign_pdf_with_certificate`].
///
/// # Warning — structural signature, not cryptographic
///
/// See [`sign_pdf`]: the emitted `/Contents` is a placeholder PKCS#7/CMS
/// container (SHA-256 digest only, no private-key operation). The result is
/// not a verifiable digital signature. For a genuine signature use
/// [`sign_pdf_cms_bytes`].
pub fn sign_pdf_bytes(
    pdf_bytes: &[u8],
    signature: &crate::security::DigitalSignature,
    certificate: Option<&crate::security::SigningCertificate>,
) -> Result<Vec<u8>> {
    assemble_signature_update(pdf_bytes, signature, certificate, &|digest| {
        Ok(digest.to_vec())
    })
}

/// Sign a PDF with a real cryptographic PKCS#7/CMS signature.
///
/// Builds a detached CMS `SignedData` (SHA-256, embedded signer certificate)
/// over the `/ByteRange`-covered bytes using the provided private key, and
/// splices it into the `/Contents` placeholder of an incremental update. RSA
/// and ECDSA P-256 PKCS#8 PEM keys are supported; the output is verifiable by
/// [`verify_pdf_signature`](crate::pdf_ops::verify_pdf_signature) and external tools.
///
/// # Arguments
/// * `input_file` - Path to the original PDF
/// * `output_file` - Path for the signed PDF output
/// * `signature` - Digital signature metadata (signer, reason, location, etc.)
/// * `material` - PEM certificate and PKCS#8 private key
///
/// # Example
/// ```no_run
/// use pdfrs::{security::DigitalSignature, pdf_ops::signing::SigningKeyMaterial};
/// use pdfrs::pdf_ops::sign_pdf_cms;
///
/// let material = SigningKeyMaterial::new(
///     std::fs::read_to_string("signer.pem").unwrap(),
///     std::fs::read_to_string("signer.key.pem").unwrap(),
/// );
/// let sig = DigitalSignature::new("Alice");
/// sign_pdf_cms("input.pdf", "signed.pdf", &sig, &material).unwrap();
/// ```
#[cfg(feature = "sign")]
pub fn sign_pdf_cms(
    input_file: &str,
    output_file: &str,
    signature: &crate::security::DigitalSignature,
    material: &crate::pdf_ops::signing::SigningKeyMaterial,
) -> Result<()> {
    let pdf_bytes = fs::read(input_file)?;
    let signed = sign_pdf_cms_bytes(&pdf_bytes, signature, material)?;
    fs::write(output_file, signed)?;
    println!(
        "[sign] Signed {input_file} -> {output_file} (cryptographic CMS signature, signer: {})",
        signature.signer_name
    );
    Ok(())
}

/// In-memory variant of [`sign_pdf_cms`].
#[cfg(feature = "sign")]
pub fn sign_pdf_cms_bytes(
    pdf_bytes: &[u8],
    signature: &crate::security::DigitalSignature,
    material: &crate::pdf_ops::signing::SigningKeyMaterial,
) -> Result<Vec<u8>> {
    crate::pdf_ops::signing::sign_with_cms(pdf_bytes, signature, material)
}

/// Shared incremental-update assembly used by both the structural placeholder
/// signer and the CMS signer.
///
/// Builds the signature dictionary, widget field, and catalog objects as an
/// incremental update, computes the SHA-256 digest over the
/// `/ByteRange`-covered bytes, and splices `contents_filler(digest)` into the
/// fixed-width `/Contents` placeholder as zero-padded hex.
pub(crate) fn assemble_signature_update(
    pdf_bytes: &[u8],
    signature: &crate::security::DigitalSignature,
    certificate: Option<&crate::security::SigningCertificate>,
    contents_filler: &dyn Fn(&[u8]) -> Result<Vec<u8>>,
) -> Result<Vec<u8>> {
    let objects = scan_pdf_objects(pdf_bytes)?;
    let mut max_obj = 0u32;
    for obj in &objects {
        max_obj = max_obj.max(obj.num);
    }

    // Locate the original catalog to re-emit it with /AcroForm.
    let lossy = String::from_utf8_lossy(pdf_bytes);
    let catalog_re = re_catalog_obj();
    let (catalog_num, catalog_body) = catalog_re
        .captures(&lossy)
        .map(|c| {
            let body = c[3].trim().to_string();
            (c[1].parse::<u32>().unwrap_or(1), body)
        })
        .ok_or_else(|| PdfError::Parse("no /Catalog object found in document".into()))?;

    // Original xref offset (for /Prev).
    let last_eof = find_from(pdf_bytes, b"%%EOF", 0)
        .ok_or_else(|| PdfError::InvalidPdf("document has no %%EOF marker".into()))?;
    let sx = find_from(pdf_bytes, b"startxref", 0)
        .filter(|&p| p < last_eof)
        .ok_or_else(|| PdfError::InvalidPdf("document has no startxref marker".into()))?;
    let after_sx = &pdf_bytes[sx + 9..last_eof];
    let num_end = after_sx
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .map(|ws| {
            after_sx[ws..]
                .iter()
                .position(|b| !b.is_ascii_digit())
                .map(|d| ws + d)
                .unwrap_or(after_sx.len())
        })
        .unwrap_or(0);
    let prev_xref: usize = std::str::from_utf8(&after_sx[..num_end])
        .ok()
        .and_then(|t| t.trim().parse().ok())
        .ok_or_else(|| PdfError::InvalidPdf("malformed startxref offset".into()))?;

    // Fixed-width ByteRange so later in-place splicing keeps offsets stable.
    const BR_WIDTH: usize = 10;
    let byte_range = format!(
        "[0 {:0width$} {:0width$} {:0width$}]",
        0,
        0,
        0,
        width = BR_WIDTH
    );

    let contents_placeholder = "0".repeat(8192);

    let mut sig_dict = format!(
        "<< /Type /Sig\n\
         /Filter /Adobe.PPKLite\n\
         /SubFilter /adbe.pkcs7.detached\n\
         /Contents <{contents_placeholder}>\n\
         /ByteRange {byte_range}\n"
    );
    if let Some(ref date) = signature.date {
        sig_dict.push_str(&format!(
            " /M (D:{})\n",
            crate::pdf_ops::escape_pdf_meta(date)
        ));
    }
    sig_dict.push_str(&format!(
        " /Name ({})\n",
        crate::pdf_ops::escape_pdf_meta(&signature.signer_name)
    ));
    if let Some(ref reason) = signature.reason {
        sig_dict.push_str(&format!(
            " /Reason ({})\n",
            crate::pdf_ops::escape_pdf_meta(reason)
        ));
    }
    if let Some(ref location) = signature.location {
        sig_dict.push_str(&format!(
            " /Location ({})\n",
            crate::pdf_ops::escape_pdf_meta(location)
        ));
    }
    if let Some(ref contact) = signature.contact_info {
        sig_dict.push_str(&format!(
            " /ContactInfo ({})\n",
            crate::pdf_ops::escape_pdf_meta(contact)
        ));
    }
    if let Some(cert) = certificate {
        let der_hex = crate::security::certificate_pem_to_der_hex(&cert.pem)?;
        sig_dict.push_str(&format!(" /Cert <{}>\n", der_hex));
    }
    sig_dict.push_str(">>");

    let sig_obj_num = max_obj + 1;
    let field_obj_num = max_obj + 2;
    let new_catalog_num = max_obj + 3;
    let size = new_catalog_num + 1;

    // Strip any existing /AcroForm from the copied catalog body, then add ours.
    let acro_form = format!("<< /Fields [{field_obj_num} 0 R] /SigFlags 3 >>");
    let catalog_inner = {
        let body = catalog_body
            .trim_start_matches("<<")
            .trim_end_matches(">>")
            .to_string();
        let cleaned = re_acroform().replace_all(&body, "").to_string();
        format!("<<{cleaned} /AcroForm {acro_form}>>")
    };

    let update_start = pdf_bytes.len();
    let mut update: Vec<u8> = Vec::with_capacity(16384);

    let sig_obj = format!("{sig_obj_num} 0 obj\n{sig_dict}\nendobj\n");
    let field_obj = format!(
        "{field_obj_num} 0 obj\n<< /Type /Annot\n\
         /Subtype /Widget\n\
         /FT /Sig\n\
         /T (Signature1)\n\
         /V {sig_obj_num} 0 R\n\
         /P {catalog_num} 0 R\n\
         /Rect [0 0 0 0]\n\
         /F 132\n\
         >>\nendobj\n"
    );
    let catalog_obj = format!("{new_catalog_num} 0 obj\n{catalog_inner}\nendobj\n");

    let obj_offsets = [(sig_obj_num, update_start + update.len())];
    update.extend_from_slice(sig_obj.as_bytes());
    let obj_offsets = [obj_offsets[0], (field_obj_num, update_start + update.len())];
    update.extend_from_slice(field_obj.as_bytes());
    let obj_offsets = [
        obj_offsets[0],
        obj_offsets[1],
        (new_catalog_num, update_start + update.len()),
    ];
    update.extend_from_slice(catalog_obj.as_bytes());

    // xref for the three new objects.
    update.extend_from_slice(
        format!(
            "xref\n0 1\n0000000000 65535 f \n{sig_obj_num} 3\n{:010} 00000 n \n{:010} 00000 n \n{:010} 00000 n \n",
            obj_offsets[0].1,
            obj_offsets[1].1,
            obj_offsets[2].1
        )
        .as_bytes(),
    );
    update.extend_from_slice(
        format!(
            "trailer\n<< /Size {size} /Root {new_catalog_num} 0 R /Prev {prev_xref} >>\nstartxref\n{}\n%%EOF\n",
            update_start
        )
        .as_bytes(),
    );

    // Compute the real ByteRange around the /Contents value, then splice both
    // in place — neither changes total length, so offsets stay valid.
    let mut output = pdf_bytes.to_vec();
    output.extend_from_slice(&update);

    let contents_marker = format!("/Contents <{contents_placeholder}>");
    let lt = find_from(&output, contents_marker.as_bytes(), 0)
        .map(|p| p + "/Contents ".len())
        .ok_or_else(|| PdfError::Crypto("signature contents placeholder not found".into()))?;
    let gt = lt + contents_placeholder.len() + 1;
    if output.get(gt) != Some(&b'>') {
        return Err(PdfError::Crypto(
            "signature contents placeholder not found".into(),
        ));
    }
    let total = output.len();

    // Splice the real ByteRange values FIRST: they live inside the first
    // covered region, so the digest must be computed over the final bytes.
    let br_marker = format!("/ByteRange {byte_range}");
    let br_pos = find_from(&output, br_marker.as_bytes(), 0)
        .ok_or_else(|| PdfError::Crypto("ByteRange placeholder not found".into()))?;
    let nums_start = br_pos + "/ByteRange [0 ".len();
    let real_br = format!(
        "{lt:0width$} {mid:0width$} {tail:0width$}",
        mid = gt + 1,
        tail = total - gt - 1,
        width = BR_WIDTH
    );
    output[nums_start..nums_start + real_br.len()].copy_from_slice(real_br.as_bytes());

    // Digest over the covered bytes (the /Contents region between the angle
    // brackets is excluded, so filling it afterwards does not change it).
    let mut hasher = Sha256::new();
    hasher.update(&output[..lt]);
    hasher.update(&output[gt + 1..]);
    let hash = hasher.finalize();

    // Splice the filler's contents into the placeholder (same width).
    let contents = contents_filler(&hash)?;
    if contents.len() > contents_placeholder.len() / 2 {
        return Err(PdfError::Crypto(
            "signature contents exceed the /Contents placeholder".into(),
        ));
    }
    let contents_hex: String = contents.iter().map(|b| format!("{b:02x}")).collect();
    let padded_contents = format!("{contents_hex:0<8192}");
    output[lt + 1..gt].copy_from_slice(padded_contents.as_bytes());

    Ok(output)
}
