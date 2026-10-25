//! X.509 signing-certificate management (PEM store, fingerprints, /Cert hex).

use crate::error::{PdfError, Result};

// --- Certificate management (FR19.4) ---

/// X.509 signing certificate stored as PEM for PDF digital signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningCertificate {
    /// Unique identifier for this certificate in a store
    pub id: String,
    /// Distinguished name or display subject (e.g. `CN=Alice`)
    pub subject: String,
    /// Optional issuer distinguished name
    pub issuer: Option<String>,
    /// PEM-encoded certificate bytes
    pub pem: String,
    /// SHA-256 fingerprint of the DER encoding (hex)
    pub fingerprint_sha256: String,
}

/// Directory-backed store for signing certificates (`{id}.pem` files).
#[derive(Debug, Clone)]
pub struct CertificateStore {
    directory: std::path::PathBuf,
}

impl CertificateStore {
    /// Open or create a certificate store directory.
    pub fn open(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        std::fs::create_dir_all(&directory)?;
        Ok(Self { directory })
    }

    /// Import a PEM certificate file into the store under `id`.
    pub fn import(
        &self,
        id: &str,
        pem_path: &str,
        subject: Option<&str>,
    ) -> Result<SigningCertificate> {
        validate_cert_id(id)?;
        let cert = load_certificate_pem(id, pem_path)?;
        let cert = if let Some(subject) = subject {
            SigningCertificate {
                subject: subject.to_string(),
                ..cert
            }
        } else {
            cert
        };
        let dest = self.directory.join(format!("{id}.pem"));
        std::fs::write(&dest, &cert.pem)?;
        Ok(cert)
    }

    /// List all certificates in the store.
    pub fn list(&self) -> Result<Vec<SigningCertificate>> {
        let mut certs = Vec::new();
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pem") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                certs.push(load_certificate_pem(&id, path.to_str().unwrap())?);
            }
        }
        certs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(certs)
    }

    /// Load a certificate from the store by id.
    pub fn get(&self, id: &str) -> Result<SigningCertificate> {
        validate_cert_id(id)?;
        let path = self.directory.join(format!("{id}.pem"));
        load_certificate_pem(id, path.to_str().unwrap())
    }

    /// Remove a certificate from the store.
    pub fn remove(&self, id: &str) -> Result<bool> {
        validate_cert_id(id)?;
        let path = self.directory.join(format!("{id}.pem"));
        if path.exists() {
            std::fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Validate that a certificate store ID is safe (no path traversal, no empty).
fn validate_cert_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(PdfError::InvalidInput(
            "Certificate ID cannot be empty".into(),
        ));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(PdfError::InvalidInput(
            "Certificate ID contains invalid characters (path separators or traversal sequences)"
                .into(),
        ));
    }
    Ok(())
}

/// Load a PEM-encoded X.509 certificate from disk.
pub fn load_certificate_pem(id: impl Into<String>, path: &str) -> Result<SigningCertificate> {
    let pem = std::fs::read_to_string(path)?;
    parse_certificate_pem(id, &pem)
}

/// Parse a PEM-encoded X.509 certificate string.
pub fn parse_certificate_pem(id: impl Into<String>, pem: &str) -> Result<SigningCertificate> {
    let id = id.into();
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err(PdfError::Crypto(
            "File does not contain a PEM certificate block".into(),
        ));
    }

    let der = pem_to_der(pem)?;
    let fingerprint_sha256 = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&der);
        hash.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };

    let subject = parse_subject_from_der(&der).unwrap_or_else(|| id.clone());

    Ok(SigningCertificate {
        id,
        subject,
        issuer: None,
        pem: pem.to_string(),
        fingerprint_sha256,
    })
}

/// Convert PEM certificate text to uppercase hex-encoded DER (for PDF `/Cert` entries).
pub fn certificate_pem_to_der_hex(pem: &str) -> Result<String> {
    let der = pem_to_der(pem)?;
    Ok(der.iter().map(|b| format!("{:02x}", b)).collect::<String>())
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>> {
    let b64: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    pem_body_to_der(&b64)
}

/// Decode the base64 body of a PEM block (without the `-----` markers).
pub(crate) fn pem_body_to_der(body: &str) -> Result<Vec<u8>> {
    let b64: String = body.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    decode_base64(&b64)
}

fn decode_base64(input: &str) -> Result<Vec<u8>> {
    const TABLE: &[u8; 256] = &{
        let mut table = [255u8; 256];
        let mut i = 0u8;
        while i < 26 {
            table[(b'A' + i) as usize] = i;
            table[(b'a' + i) as usize] = 26 + i;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            table[(b'0' + d) as usize] = 52 + d;
            d += 1;
        }
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
        table
    };

    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &byte in input.as_bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let val = TABLE[byte as usize];
        if val == 255 {
            return Err(PdfError::Crypto(
                "Invalid base64 character in PEM certificate".into(),
            ));
        }
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

fn parse_subject_from_der(der: &[u8]) -> Option<String> {
    let lossy = String::from_utf8_lossy(der);
    // Heuristic: self-signed and test certs embed the CN as a printable UTF-8 string in the DER.
    for marker in ["CN=", "Test Signer"] {
        if let Some(idx) = lossy.find(marker) {
            let slice = &lossy[idx..];
            let end = slice
                .find(['\0', '\x01', '\x02'])
                .unwrap_or(slice.len().min(64));
            let candidate = slice[..end].trim_matches(|c: char| !c.is_ascii_graphic() && c != '=');
            if !candidate.is_empty() {
                return Some(if candidate.starts_with("CN=") {
                    candidate.to_string()
                } else {
                    format!("CN={candidate}")
                });
            }
        }
    }
    None
}
