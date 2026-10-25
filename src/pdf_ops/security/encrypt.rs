//! Password protection: encrypting an existing PDF with the standard
//! security handler.

use std::fs;

use super::objects::{
    encrypt_strings_in_dict, expand_compressed_objects, is_xref_stream_dict, parse_trailer_refs,
    rewrite_stream_length, scan_pdf_objects,
};
use crate::error::{PdfError, Result};

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

    if !security.is_protected() {
        let content = fs::read(input_file)?;
        fs::write(output_file, content)?;
        return Ok(());
    }

    let pdf_bytes = fs::read(input_file)?;
    let encrypted = encrypt_pdf_bytes(&pdf_bytes, security)?;
    fs::write(output_file, encrypted)?;
    Ok(())
}

/// Encrypt a PDF in memory: encrypts all stream and string objects, appends
/// the `/Encrypt` dictionary, and rebuilds the xref table and trailer.
///
/// Operates on raw bytes end-to-end (no UTF-8 lossy conversion), so binary
/// streams survive intact. Object streams are expanded and cross-reference
/// streams are replaced by a fresh classic xref table before encryption.
pub fn encrypt_pdf_bytes(
    pdf_bytes: &[u8],
    security: &crate::security::PdfSecurity,
) -> Result<Vec<u8>> {
    let mut doc_id = [0u8; 16];
    crate::security::random_bytes(&mut doc_id)?;
    Ok(encrypt_pdf_bytes_with_id(pdf_bytes, security, doc_id)?.0)
}

/// Like [`encrypt_pdf_bytes`], but returns the generated file key alongside
/// the encrypted document and accepts an explicit document `/ID`.
pub fn encrypt_pdf_bytes_with_id(
    pdf_bytes: &[u8],
    security: &crate::security::PdfSecurity,
    doc_id: [u8; 16],
) -> Result<(Vec<u8>, crate::security::EncryptionMaterials)> {
    security.validate()?;
    if !security.is_protected() {
        return Ok((
            pdf_bytes.to_vec(),
            crate::security::EncryptionMaterials {
                file_key: Vec::new(),
                encrypt_dict: String::new(),
            },
        ));
    }

    let mut objects = scan_pdf_objects(pdf_bytes)?;
    expand_compressed_objects(&mut objects, pdf_bytes)?;
    let (root_ref, info_ref) = parse_trailer_refs(pdf_bytes, &objects)?;

    let materials = security.generate_encryption_materials(&doc_id)?;
    let id_hex: String = doc_id.iter().map(|b| format!("{b:02x}")).collect();

    // Rebuild the file: header, all objects (encrypted), /Encrypt object,
    // fresh xref table, and a new trailer.
    let mut max_obj = 0u32;
    for obj in &objects {
        max_obj = max_obj.max(obj.num);
    }
    let encrypt_obj_num = max_obj + 1;

    let header_end = objects.first().map(|o| o.header_start).unwrap_or(0);
    let mut out = Vec::with_capacity(pdf_bytes.len() + 1024);
    out.extend_from_slice(&pdf_bytes[..header_end]);
    if !out.is_empty() && out.last() != Some(&b'\n') {
        out.push(b'\n');
    }

    let mut offsets: std::collections::BTreeMap<u32, u64> = std::collections::BTreeMap::new();
    for obj in &objects {
        if obj.num == 0
            || obj.skip
            || obj.is_xref_stream
            || is_xref_stream_dict(obj.dict_slice(pdf_bytes))
        {
            continue;
        }
        offsets.insert(obj.num, out.len() as u64);
        out.extend_from_slice(format!("{} {} obj", obj.num, obj.generation).as_bytes());
        match obj.stream {
            None => {
                let body = if let Some(body) = obj.body_override.as_deref() {
                    body
                } else {
                    let end = obj.body_end.checked_sub(6).ok_or_else(|| {
                        PdfError::Parse(format!("object {} has an invalid body extent", obj.num))
                    })?;
                    pdf_bytes.get(obj.header_end..end).ok_or_else(|| {
                        PdfError::Parse(format!("object {} has an invalid body range", obj.num))
                    })?
                };
                let encrypted = encrypt_strings_in_dict(
                    body,
                    security,
                    &materials.file_key,
                    obj.num,
                    obj.generation,
                )?;
                out.extend_from_slice(&encrypted);
                out.extend_from_slice(b"endobj\n");
            }
            Some(ref stream) => {
                let dict_bytes = &pdf_bytes[obj.header_end..stream.keyword_start];
                let raw_stream = &pdf_bytes[stream.data_start..stream.data_end];
                let encrypted = security.encrypt_data(
                    raw_stream,
                    &materials.file_key,
                    obj.num,
                    obj.generation,
                )?;
                let mut dict = rewrite_stream_length(dict_bytes, encrypted.len());
                if dict.last() != Some(&b'\n') && dict.last() != Some(&b' ') {
                    dict.push(b'\n');
                }
                out.extend_from_slice(&dict);
                out.extend_from_slice(b"stream\n");
                out.extend_from_slice(&encrypted);
                out.extend_from_slice(b"\nendstream\nendobj\n");
            }
        }
    }

    offsets.insert(encrypt_obj_num, out.len() as u64);
    out.extend_from_slice(
        format!(
            "{} 0 obj\n{}\nendobj\n",
            encrypt_obj_num, materials.encrypt_dict
        )
        .as_bytes(),
    );

    // xref table with contiguous subsections; holes become free entries.
    let xref_start = out.len() as u64;
    let mut xref = String::from("xref\n0 1\n0000000000 65535 f \n");
    let mut nums: Vec<u32> = offsets.keys().copied().collect();
    nums.sort_unstable();
    let mut i = 0;
    while i < nums.len() {
        let run_start = nums[i];
        let mut j = i;
        while j + 1 < nums.len() && nums[j + 1] == nums[j] + 1 {
            j += 1;
        }
        let count = nums[j] - run_start + 1;
        xref.push_str(&format!("{run_start} {count}\n"));
        for n in run_start..=nums[j] {
            match offsets.get(&n) {
                Some(&off) => xref.push_str(&format!("{off:010} 00000 n \n")),
                None => xref.push_str("0000000000 65535 f \n"),
            }
        }
        i = j + 1;
    }
    out.extend_from_slice(xref.as_bytes());

    let size = encrypt_obj_num + 1;
    let mut trailer =
        format!("trailer\n<< /Size {size}\n/Encrypt {encrypt_obj_num} 0 R\n/Root {root_ref}");
    if let Some(ref info) = info_ref {
        trailer.push_str(&format!("\n/Info {info}"));
    }
    trailer.push_str(&format!(
        "\n/ID <{id_hex}> <{id_hex}>\n>>\nstartxref\n{xref_start}\n%%EOF\n"
    ));
    out.extend_from_slice(trailer.as_bytes());

    Ok((out, materials))
}
