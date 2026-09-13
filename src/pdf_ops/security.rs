//! PDF security: password protection, digital signatures, and certificate extraction.

use crate::error::{PdfError, Result};
use std::fs;

use sha2::{Digest, Sha256};

macro_rules! security_regex {
    ($name:ident, $pat:literal) => {
        fn $name() -> &'static regex::Regex {
            static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
            RE.get_or_init(|| regex::Regex::new($pat).unwrap())
        }
    };
}

security_regex!(re_sig_obj, r"(?s)(\d+)\s+0\s+obj\s+<<(.+?)>>\s+endobj");
security_regex!(re_root_ref, r"/Root\s+(\d+\s+\d+\s+R)");
security_regex!(re_info_ref, r"/Info\s+(\d+\s+\d+\s+R)");
security_regex!(
    re_catalog_obj,
    r"(?s)(\d+)\s+(\d+)\s+obj\s*(<<.*?/Type\s*/Catalog.*?>>)"
);
security_regex!(re_acroform, r"/AcroForm\s+<<[^>]*>>");

/// Apply password protection and permissions to a PDF.
///
/// This function adds security settings to a PDF document, including password protection
/// and permission restrictions. It encrypts string and stream objects using the configured
/// algorithm (RC4 40/128-bit or AES-128/256-bit) and writes the `/Encrypt` dictionary.
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
/// streams survive intact. Documents using cross-reference streams or object
/// streams are rejected with an error rather than silently corrupted.
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

    let objects = scan_pdf_objects(pdf_bytes)?;
    for obj in &objects {
        let dict = obj.dict_slice(pdf_bytes);
        if dict.windows(b"/ObjStm".len()).any(|w| w == b"/ObjStm")
            || dict
                .windows(b"/Type /XRef".len())
                .any(|w| w == b"/Type /XRef")
            || dict
                .windows(b"/Type/XRef".len())
                .any(|w| w == b"/Type/XRef")
        {
            return Err(PdfError::Unsupported(
                "encryption of documents using object streams or cross-reference streams is not supported yet"
                    .into(),
            ));
        }
    }

    let (root_ref, info_ref) = parse_trailer_refs(pdf_bytes)?;

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
        if obj.num == 0 {
            continue;
        }
        offsets.insert(obj.num, out.len() as u64);
        out.extend_from_slice(format!("{} {} obj", obj.num, obj.generation).as_bytes());
        match obj.stream {
            None => {
                let dict_bytes = &pdf_bytes[obj.header_end..obj.body_end - 6];
                let encrypted = encrypt_strings_in_dict(
                    dict_bytes,
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

/// A scanned `N G obj ... endobj` object with byte-precise extents.
struct RawObject {
    num: u32,
    generation: u16,
    header_start: usize,
    /// Just after the `obj` keyword.
    header_end: usize,
    /// Just after `endobj`.
    body_end: usize,
    stream: Option<StreamSpan>,
}

struct StreamSpan {
    /// Position of the `stream` keyword.
    keyword_start: usize,
    data_start: usize,
    data_end: usize,
}

impl RawObject {
    fn dict_slice<'a>(&self, bytes: &'a [u8]) -> &'a [u8] {
        match self.stream {
            Some(ref s) => &bytes[self.header_end..s.keyword_start],
            None => &bytes[self.header_end..self.body_end],
        }
    }
}

fn find_from(bytes: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from >= bytes.len() {
        return None;
    }
    bytes[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Scan `N G obj ... endobj` objects sequentially, using direct `/Length`
/// values to locate stream boundaries in binary data.
fn scan_pdf_objects(bytes: &[u8]) -> Result<Vec<RawObject>> {
    let header_re = regex::bytes::Regex::new(r"(\d+)[\x20\t\r\n]+(\d+)[\x20\t\r\n]+obj").unwrap();
    let mut objects = Vec::new();
    let mut pos = 0usize;
    while pos < bytes.len() {
        let Some(m) = header_re.find_at(bytes, pos) else {
            break;
        };
        let whole = &bytes[m.start()..m.end()];
        let fields: Vec<&[u8]> = whole
            .split(|&b| b == b' ' || b == b'\t' || b == b'\r' || b == b'\n')
            .filter(|f| !f.is_empty())
            .collect();
        if fields.len() < 3 {
            pos = m.end();
            continue;
        }
        let num: u32 = std::str::from_utf8(fields[0])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        let generation: u16 = std::str::from_utf8(fields[1])
            .unwrap_or("0")
            .parse()
            .unwrap_or(0);
        if num == 0 {
            pos = m.end();
            continue;
        }

        let endobj = find_from(bytes, b"endobj", m.end());
        let stream_kw = find_standalone_stream_kw(bytes, m.end());

        let (stream, body_end) = match (endobj, stream_kw) {
            (Some(eo), Some(sk)) if sk < eo => {
                let dict = &bytes[m.end()..sk];
                let mut data_start = sk + 6;
                if bytes.get(data_start) == Some(&b'\r') {
                    data_start += 1;
                }
                if bytes.get(data_start) == Some(&b'\n') {
                    data_start += 1;
                }
                let data_end = match stream_data_end_via_length(bytes, dict, data_start) {
                    Some(end) => end,
                    None => {
                        let es = find_from(bytes, b"endstream", data_start).ok_or_else(|| {
                            PdfError::Parse(format!("unterminated stream in object {num}"))
                        })?;
                        strip_trailing_eol(bytes, es)
                    }
                };
                let after_data = find_from(bytes, b"endstream", data_end)
                    .ok_or_else(|| PdfError::Parse(format!("missing endstream in object {num}")))?;
                let end = find_from(bytes, b"endobj", after_data)
                    .ok_or_else(|| PdfError::Parse(format!("missing endobj for object {num}")))?
                    + 6;
                (
                    Some(StreamSpan {
                        keyword_start: sk,
                        data_start,
                        data_end,
                    }),
                    end,
                )
            }
            (Some(eo), _) => (None, eo + 6),
            (None, _) => {
                return Err(PdfError::Parse(format!(
                    "unterminated object near offset {}",
                    m.start()
                )));
            }
        };

        objects.push(RawObject {
            num,
            generation,
            header_start: m.start(),
            header_end: m.end(),
            body_end,
            stream,
        });
        pos = body_end;
    }
    Ok(objects)
}

/// Find the next `stream` keyword that stands alone (whitespace before,
/// end-of-line after) — avoids matching the word inside stream data.
fn find_standalone_stream_kw(bytes: &[u8], from: usize) -> Option<usize> {
    let mut search = from;
    while let Some(p) = find_from(bytes, b"stream", search) {
        let before_ok = p > 0 && bytes[p - 1].is_ascii_whitespace();
        let after = p + 6;
        let after_ok = matches!(bytes.get(after), Some(b'\r') | Some(b'\n'));
        if before_ok && after_ok {
            return Some(p);
        }
        search = p + 6;
    }
    None
}

/// Resolve the stream data end using a direct `/Length` integer, verifying
/// that `endstream` actually follows. Returns `None` to fall back to a scan.
fn stream_data_end_via_length(bytes: &[u8], dict: &[u8], data_start: usize) -> Option<usize> {
    let re = regex::bytes::Regex::new(r"/Length\s+(\d+)").ok()?;
    let caps = re.captures(dict)?;
    let len: usize = std::str::from_utf8(&caps[1]).ok()?.parse().ok()?;
    let end = data_start.checked_add(len)?;
    let after = &bytes[end.min(bytes.len())..];
    let ws = after
        .iter()
        .take_while(|&&b| b == b'\r' || b == b'\n')
        .count();
    if after[ws..].starts_with(b"endstream") {
        Some(end)
    } else {
        None
    }
}

fn strip_trailing_eol(bytes: &[u8], endstream_pos: usize) -> usize {
    let mut end = endstream_pos;
    if end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
    }
    if end > 0 && bytes[end - 1] == b'\r' {
        end -= 1;
    }
    end
}

/// Extract `/Root` and `/Info` references from a classic `trailer` dict.
fn parse_trailer_refs(bytes: &[u8]) -> Result<(String, Option<String>)> {
    let sx = find_from(bytes, b"startxref", 0)
        .ok_or_else(|| PdfError::InvalidPdf("no startxref found in document".into()))?;
    let trailer_kw = find_from(bytes, b"trailer", 0)
        .filter(|&t| t < sx)
        .ok_or_else(|| {
            PdfError::Unsupported(
                "no trailer dictionary found (cross-reference stream document?)".into(),
            )
        })?;
    let dict_re = regex::bytes::Regex::new(r"(?s)<<(.+?)>>").unwrap();
    let region = &bytes[trailer_kw..sx];
    let dict = dict_re
        .captures(region)
        .map(|c| c[1].to_vec())
        .ok_or_else(|| PdfError::Parse("malformed trailer dictionary".into()))?;
    let dict_text = String::from_utf8_lossy(&dict).to_string();
    let root_re = re_root_ref();
    let root = root_re
        .captures(&dict_text)
        .map(|c| c[1].to_string())
        .ok_or_else(|| PdfError::Parse("trailer has no /Root reference".into()))?;
    let info_re = re_info_ref();
    let info = info_re.captures(&dict_text).map(|c| c[1].to_string());
    Ok((root, info))
}

/// Replace a stream dictionary `/Length` entry with a new direct value.
fn rewrite_stream_length(dict: &[u8], new_len: usize) -> Vec<u8> {
    let re = regex::bytes::Regex::new(r"(/Length\s+)\d+(?:\s+\d+\s+R)?").unwrap();
    re.replace_all(dict, format!("${{1}}{new_len}").as_bytes())
        .into_owned()
}

/// Encrypt PDF literal string `(...)` values within an object dictionary,
/// re-emitting them as hex strings. Handles escape sequences and nesting.
fn encrypt_strings_in_dict(
    dict: &[u8],
    security: &crate::security::PdfSecurity,
    file_key: &[u8],
    obj_num: u32,
    gen_num: u16,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(dict.len());
    let mut i = 0usize;
    while i < dict.len() {
        if dict[i] != b'(' {
            out.push(dict[i]);
            i += 1;
            continue;
        }
        // Scan the literal string, decoding escapes into raw bytes.
        let mut depth = 1usize;
        let mut raw = Vec::new();
        i += 1;
        while i < dict.len() && depth > 0 {
            let c = dict[i];
            if c == b'\\' && i + 1 < dict.len() {
                let n = dict[i + 1];
                match n {
                    b'n' => raw.push(b'\n'),
                    b'r' => raw.push(b'\r'),
                    b't' => raw.push(b'\t'),
                    b'b' => raw.push(0x08),
                    b'f' => raw.push(0x0C),
                    b'(' => raw.push(b'('),
                    b')' => raw.push(b')'),
                    b'\\' => raw.push(b'\\'),
                    b'0'..=b'7' => {
                        let mut val = 0u32;
                        let mut used = 0;
                        let mut j = i + 1;
                        while used < 3 && j < dict.len() && (b'0'..=b'7').contains(&dict[j]) {
                            val = val * 8 + u32::from(dict[j] - b'0');
                            j += 1;
                            used += 1;
                        }
                        raw.push((val & 0xFF) as u8);
                        i = j - 2;
                    }
                    b'\r' => {
                        if dict.get(i + 2) == Some(&b'\n') {
                            i += 1;
                        }
                    }
                    b'\n' => {}
                    other => raw.push(other),
                }
                i += 2;
            } else if c == b'(' {
                depth += 1;
                raw.push(c);
                i += 1;
            } else if c == b')' {
                depth -= 1;
                if depth > 0 {
                    raw.push(c);
                }
                i += 1;
            } else {
                raw.push(c);
                i += 1;
            }
        }
        if depth != 0 {
            return Err(PdfError::Parse(format!(
                "unbalanced string literal in object {obj_num}"
            )));
        }
        let encrypted = security.encrypt_data(&raw, file_key, obj_num, gen_num)?;
        out.push(b'<');
        for b in &encrypted {
            out.extend_from_slice(format!("{b:02x}").as_bytes());
        }
        out.push(b'>');
    }
    Ok(out)
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
pub fn sign_pdf_bytes(
    pdf_bytes: &[u8],
    signature: &crate::security::DigitalSignature,
    certificate: Option<&crate::security::SigningCertificate>,
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
        sig_dict.push_str(&format!(" /M (D:{})\n", super::escape_pdf_meta(date)));
    }
    sig_dict.push_str(&format!(
        " /Name ({})\n",
        super::escape_pdf_meta(&signature.signer_name)
    ));
    if let Some(ref reason) = signature.reason {
        sig_dict.push_str(&format!(" /Reason ({})\n", super::escape_pdf_meta(reason)));
    }
    if let Some(ref location) = signature.location {
        sig_dict.push_str(&format!(
            " /Location ({})\n",
            super::escape_pdf_meta(location)
        ));
    }
    if let Some(ref contact) = signature.contact_info {
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

    let mut hasher = Sha256::new();
    hasher.update(&output[..lt]);
    hasher.update(&output[gt + 1..]);
    let hash = hasher.finalize();
    let hash_hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();

    // Splice hash into the placeholder (same width).
    let padded_hash = format!("{hash_hex:0<8192}");
    output[lt + 1..gt].copy_from_slice(padded_hash.as_bytes());

    // Splice real ByteRange values over the fixed-width zeros.
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

    Ok(output)
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
    let obj_re = re_sig_obj();
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
    let obj_re = re_sig_obj();
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

fn parse_cert_hex_metadata(hex: &str) -> Option<(String, String)> {
    let cert = der_hex_to_certificate(hex, 0).ok()?;
    Some((cert.subject, cert.fingerprint_sha256))
}
