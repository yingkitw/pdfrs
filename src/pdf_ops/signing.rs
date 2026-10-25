//! Real PKCS#7/CMS detached signing and verification (RFC 5652).
//!
//! This module upgrades [`sign_pdf_bytes`](super::security::sign_pdf_bytes)-
//! style structural signing to genuine cryptographic signatures:
//!
//! - **Signing**: a detached CMS `SignedData` (SHA-256, embedded signer
//!   certificate) is built over the `/ByteRange`-covered bytes and spliced
//!   into the `/Contents` placeholder of an `/adbe.pkcs7.detached` signature
//!   dictionary. RSA (PKCS#1 v1.5) and ECDSA P-256 private keys in PKCS#8
//!   PEM form are supported.
//! - **Verification**: the CMS container is parsed, the digest is recomputed
//!   over the byte ranges recorded in `/ByteRange`, and the signature is
//!   verified against the public key of a certificate embedded in the
//!   container.
//!
//! DER encoding/decoding for the fixed `SignedData` shape used here is
//! implemented locally (TLV reader/writer) rather than pulled in as a
//! dependency; X.509 parsing uses the `x509-cert` crate.

use crate::error::{PdfError, Result};
use der::{Decode, DecodePem, Encode};
use sha2::{Digest, Sha256};
use x509_cert::Certificate;

// Object identifiers, DER content-octet encoding (no tag/length).
const OID_RSA_ENCRYPTION: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01]; // 1.2.840.113549.1.1.1
const OID_EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01]; // 1.2.840.10045.2.1 (id-ecPublicKey)
const OID_ECDSA_WITH_SHA256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02]; // 1.2.840.10045.4.3.2
const OID_SHA256: &[u8] = &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01]; // 2.16.840.1.101.3.4.2.1
const OID_ID_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x01]; // 1.2.840.113549.1.7.1
const OID_ID_SIGNED_DATA: &[u8] = &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02]; // 1.2.840.113549.1.7.2

/// PEM-encoded signing material: X.509 certificate + PKCS#8 private key.
#[derive(Debug, Clone)]
pub struct SigningKeyMaterial {
    /// PEM certificate (`-----BEGIN CERTIFICATE-----`).
    pub cert_pem: String,
    /// PEM PKCS#8 private key (`-----BEGIN PRIVATE KEY-----`), RSA or ECDSA
    /// P-256. Encrypted keys are not supported.
    pub key_pem: String,
    /// PEM certificates of the issuing CAs, leaf-most first.
    ///
    /// They are embedded alongside the signer certificate so that a verifier
    /// can build the chain up to a trusted root. Leaving this empty produces a
    /// signature that only carries the signer certificate.
    pub chain_pem: Vec<String>,
}

impl SigningKeyMaterial {
    /// Signing material with just a certificate and a private key.
    pub fn new(cert_pem: String, key_pem: String) -> Self {
        SigningKeyMaterial {
            cert_pem,
            key_pem,
            chain_pem: Vec::new(),
        }
    }

    /// Add the issuing CA certificates, leaf-most first.
    pub fn with_chain(mut self, chain_pem: Vec<String>) -> Self {
        self.chain_pem = chain_pem;
        self
    }
}

// ----- minimal DER writer -----------------------------------------------------

fn der_len(len: usize) -> Vec<u8> {
    if len < 0x80 {
        vec![len as u8]
    } else if len <= 0xFF {
        vec![0x81, len as u8]
    } else {
        vec![0x82, (len >> 8) as u8, (len & 0xFF) as u8]
    }
}

fn tlv(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend_from_slice(&der_len(content.len()));
    out.extend_from_slice(content);
    out
}

fn der_oid(content: &[u8]) -> Vec<u8> {
    tlv(0x06, content)
}

fn der_octet_string(bytes: &[u8]) -> Vec<u8> {
    tlv(0x04, bytes)
}

/// Minimal two's-complement positive INTEGER (adds a leading 0x00 when the
/// high bit is set, per DER).
fn der_positive_integer(magnitude: &[u8]) -> Vec<u8> {
    let mut content: Vec<u8> = magnitude.to_vec();
    if content.is_empty() {
        content.push(0);
    } else if content[0] & 0x80 != 0 {
        content.insert(0, 0);
    }
    tlv(0x02, &content)
}

fn concat_all(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for p in parts {
        out.extend_from_slice(p);
    }
    out
}

fn der_sequence(parts: &[&[u8]]) -> Vec<u8> {
    tlv(0x30, &concat_all(parts))
}

fn der_set(parts: &[&[u8]]) -> Vec<u8> {
    tlv(0x31, &concat_all(parts))
}

// ----- minimal DER reader ------------------------------------------------------

/// One parsed DER TLV; `full` includes tag and length for raw byte copies.
#[derive(Debug)]
struct Tlv<'a> {
    tag: u8,
    content: &'a [u8],
    full: &'a [u8],
}

fn read_tlv<'a>(buf: &'a [u8], pos: &mut usize) -> Result<Tlv<'a>> {
    let start = *pos;
    let tag = *buf
        .get(*pos)
        .ok_or_else(|| PdfError::Crypto("DER truncated (tag)".into()))?;
    if tag & 0x1F == 0x1F {
        return Err(PdfError::Crypto(
            "DER multi-byte tags are not supported here".into(),
        ));
    }
    *pos += 1;
    let first = *buf
        .get(*pos)
        .ok_or_else(|| PdfError::Crypto("DER truncated (length)".into()))?;
    *pos += 1;
    let len = if first < 0x80 {
        first as usize
    } else {
        let n = (first & 0x7F) as usize;
        if n == 0 || n > 2 {
            return Err(PdfError::Crypto("unsupported DER length form".into()));
        }
        let mut len = 0usize;
        for _ in 0..n {
            let byte = *buf
                .get(*pos)
                .ok_or_else(|| PdfError::Crypto("DER truncated (long length)".into()))?;
            len = (len << 8) | byte as usize;
            *pos += 1;
        }
        len
    };
    let content_end = pos
        .checked_add(len)
        .ok_or_else(|| PdfError::Crypto("DER length overflow".into()))?;
    let content = buf
        .get(*pos..content_end)
        .ok_or_else(|| PdfError::Crypto("DER truncated (content)".into()))?;
    *pos = content_end;
    Ok(Tlv {
        tag,
        content,
        full: &buf[start..content_end],
    })
}

fn expect_tlv<'a>(buf: &'a [u8], pos: &mut usize, tag: u8, what: &str) -> Result<Tlv<'a>> {
    let t = read_tlv(buf, pos)?;
    if t.tag != tag {
        return Err(PdfError::Crypto(format!(
            "CMS structure error: expected tag {tag:#04x} for {what}, found {:#04x}",
            t.tag
        )));
    }
    Ok(t)
}

// ----- signing ------------------------------------------------------------------

enum SigningKey {
    Rsa(Box<rsa::pkcs1v15::SigningKey<Sha256>>),
    EcdsaP256(p256::ecdsa::SigningKey),
}

fn load_private_key(key_pem: &str) -> Result<SigningKey> {
    use rsa::pkcs8::DecodePrivateKey;
    if let Ok(private) = rsa::RsaPrivateKey::from_pkcs8_pem(key_pem) {
        return Ok(SigningKey::Rsa(Box::new(
            rsa::pkcs1v15::SigningKey::<Sha256>::new(private),
        )));
    }
    if let Ok(key) = p256::ecdsa::SigningKey::from_pkcs8_pem(key_pem) {
        return Ok(SigningKey::EcdsaP256(key));
    }
    Err(PdfError::Unsupported(
        "unsupported private key: expected unencrypted PKCS#8 PEM with an RSA or ECDSA P-256 key"
            .into(),
    ))
}

/// Parsed signer certificate data needed to build `SignedData`.
struct SignerCert {
    /// Full DER of the certificate (embedded into the CMS).
    der: Vec<u8>,
    /// DER of the issuer `Name` (copied byte-for-byte into `IssuerAndSerialNumber`).
    issuer_der: Vec<u8>,
    /// Serial number magnitude bytes.
    serial: Vec<u8>,
}

fn parse_signer_cert(cert_pem: &str) -> Result<SignerCert> {
    let cert = Certificate::from_pem(cert_pem.as_bytes())
        .map_err(|e| PdfError::Crypto(format!("invalid signing certificate PEM: {e}")))?;
    let der = cert
        .to_der()
        .map_err(|e| PdfError::Crypto(format!("certificate DER re-encode failed: {e}")))?;
    let issuer_der = cert
        .tbs_certificate
        .issuer
        .to_der()
        .map_err(|e| PdfError::Crypto(format!("issuer name encode failed: {e}")))?;
    Ok(SignerCert {
        der,
        issuer_der,
        serial: cert.tbs_certificate.serial_number.as_bytes().to_vec(),
    })
}

fn signature_algorithm(key: &SigningKey) -> Vec<u8> {
    match key {
        // rsaEncryption with NULL parameters (maximum interoperability).
        SigningKey::Rsa(_) => der_sequence(&[&der_oid(OID_RSA_ENCRYPTION), &tlv(0x05, &[])]),
        // ecdsa-with-SHA256, parameters absent.
        SigningKey::EcdsaP256(_) => der_sequence(&[&der_oid(OID_ECDSA_WITH_SHA256)]),
    }
}

fn sign_digest(key: &SigningKey, digest: &[u8; 32]) -> Result<Vec<u8>> {
    use p256::ecdsa::signature::hazmat::PrehashSigner;
    use rsa::signature::SignatureEncoding;
    match key {
        SigningKey::Rsa(sk) => {
            let sig = sk
                .sign_prehash(digest)
                .map_err(|e| PdfError::Crypto(format!("RSA signing failed: {e}")))?;
            Ok(sig.to_vec())
        }
        SigningKey::EcdsaP256(sk) => {
            let sig: p256::ecdsa::DerSignature = sk
                .sign_prehash(digest)
                .map_err(|e| PdfError::Crypto(format!("ECDSA signing failed: {e}")))?;
            let der = sig
                .to_der()
                .map_err(|e| PdfError::Crypto(format!("ECDSA signature encode failed: {e}")))?;
            Ok(der)
        }
    }
}

/// Build a detached CMS `ContentInfo` wrapping a `SignedData` with one
/// signer, the embedded certificate chain, and no signed attributes.
fn build_cms_signed_data(
    digest: &[u8; 32],
    key: &SigningKey,
    cert: &SignerCert,
    chain: &[Vec<u8>],
) -> Result<Vec<u8>> {
    let version = der_positive_integer(&[1]);
    let sha256_alg = der_sequence(&[&der_oid(OID_SHA256)]);
    let digest_algorithms = der_set(&[&sha256_alg]);
    let encap_content = der_sequence(&[&der_oid(OID_ID_DATA)]);
    // [0] IMPLICIT SET OF Certificate — the SET tag is replaced by the
    // context tag, and the set contents are the certificate TLVs. The signer
    // certificate comes first, followed by the issuing CAs, so that a verifier
    // can build the chain up to a trusted root.
    let mut all_certs = vec![cert.der.clone()];
    all_certs.extend_from_slice(chain);
    let cert_refs: Vec<&[u8]> = all_certs.iter().map(|der| der.as_slice()).collect();
    let certificates = tlv(0xA0, &der_set(&cert_refs));
    let issuer_and_serial = der_sequence(&[&cert.issuer_der, &der_positive_integer(&cert.serial)]);
    let signature = sign_digest(key, digest)?;
    let signature_algorithm = signature_algorithm(key);
    let signer_info = der_sequence(&[
        &version,
        &issuer_and_serial,
        &sha256_alg,
        &signature_algorithm,
        &der_octet_string(&signature),
    ]);
    let signed_data = der_sequence(&[
        &version,
        &digest_algorithms,
        &encap_content,
        &certificates,
        &der_set(&[&signer_info]),
    ]);
    Ok(der_sequence(&[
        &der_oid(OID_ID_SIGNED_DATA),
        &tlv(0xA0, &signed_data),
    ]))
}

/// Assemble a cryptographically signed PDF via the shared incremental-update
/// scaffolding in [`super::security`].
pub(crate) fn sign_with_cms(
    pdf_bytes: &[u8],
    signature: &crate::security::DigitalSignature,
    material: &SigningKeyMaterial,
) -> Result<Vec<u8>> {
    let key = load_private_key(&material.key_pem)?;
    let cert = parse_signer_cert(&material.cert_pem)?;
    // The issuing CAs travel with the signature so a verifier can build the
    // chain up to a trusted root.
    let mut chain = Vec::new();
    for pem in &material.chain_pem {
        chain.push(parse_signer_cert(pem)?.der);
    }
    super::security::assemble_signature_update(pdf_bytes, signature, None, &|digest| {
        let d: &[u8; 32] = digest
            .try_into()
            .map_err(|_| PdfError::Crypto("unexpected digest length".into()))?;
        build_cms_signed_data(d, &key, &cert, &chain)
    })
}

// ----- verification ---------------------------------------------------------------

/// A parsed CMS `SignedData` restricted to the fields verification needs.
pub(crate) struct ParsedCms {
    /// DER of the first embedded certificate.
    pub certificates: Vec<Vec<u8>>,
    /// Raw signature bits (contents of the `signature OCTET STRING`).
    pub signature: Vec<u8>,
    /// DER content octets of the `signatureAlgorithm` OID.
    pub signature_alg_oid: Vec<u8>,
}

fn parse_algorithm_oid(content: &[u8]) -> Result<Vec<u8>> {
    let mut pos = 0usize;
    let oid = expect_tlv(content, &mut pos, 0x06, "algorithm OID")?;
    if pos != content.len() {
        // Parameters may follow (e.g. RSA NULL); they are ignored.
        let _ = read_tlv(content, &mut pos)?;
    }
    Ok(oid.content.to_vec())
}

/// Parse a CMS `ContentInfo`/`SignedData` for verification.
pub(crate) fn parse_cms_signed_data(der: &[u8]) -> Result<ParsedCms> {
    let mut pos = 0usize;
    let content_info = expect_tlv(der, &mut pos, 0x30, "ContentInfo SEQUENCE")?;
    let mut pos = 0usize;
    let oid = expect_tlv(content_info.content, &mut pos, 0x06, "contentType OID")?;
    if oid.content != OID_ID_SIGNED_DATA {
        return Err(PdfError::Crypto(
            "CMS content is not SignedData (unexpected contentType OID)".into(),
        ));
    }
    let explicit = expect_tlv(content_info.content, &mut pos, 0xA0, "SignedData [0]")?;
    let mut pos = 0usize;
    let signed_data = expect_tlv(explicit.content, &mut pos, 0x30, "SignedData SEQUENCE")?;

    let mut pos = 0usize;
    expect_tlv(signed_data.content, &mut pos, 0x02, "version INTEGER")?;
    expect_tlv(signed_data.content, &mut pos, 0x31, "digestAlgorithms SET")?;
    expect_tlv(
        signed_data.content,
        &mut pos,
        0x30,
        "encapsulatedContentInfo",
    )?;

    // Optional [0] certificates, [1] crls, [2] aggregate context — walk until
    // the signerInfos SET.
    let mut certificates = Vec::new();
    let signer_infos = loop {
        let tlv = read_tlv(signed_data.content, &mut pos)?;
        match tlv.tag {
            0xA0 => {
                let mut set_pos = 0usize;
                let set = expect_tlv(tlv.content, &mut set_pos, 0x31, "CertificateSet")?;
                let mut cert_pos = 0usize;
                while cert_pos < set.content.len() {
                    let cert = read_tlv(set.content, &mut cert_pos)?;
                    certificates.push(cert.full.to_vec());
                }
            }
            0x31 => break tlv,
            other => {
                return Err(PdfError::Crypto(format!(
                    "unexpected tag {other:#04x} in SignedData"
                )));
            }
        }
    };

    let mut si_pos = 0usize;
    let signer_info = expect_tlv(signer_infos.content, &mut si_pos, 0x30, "SignerInfo")?;
    let mut pos = 0usize;
    expect_tlv(signer_info.content, &mut pos, 0x02, "SignerInfo version")?;
    // sid: IssuerAndSerialNumber (SEQUENCE) or SubjectKeyIdentifier ([0]).
    let _sid = read_tlv(signer_info.content, &mut pos)?;
    let digest_alg = expect_tlv(signer_info.content, &mut pos, 0x30, "digestAlgorithm")?;
    let digest_oid = parse_algorithm_oid(digest_alg.content)?;
    if digest_oid != OID_SHA256 {
        return Err(PdfError::Crypto(
            "only SHA-256 CMS signatures are supported".into(),
        ));
    }
    let sig_alg = expect_tlv(signer_info.content, &mut pos, 0x30, "signatureAlgorithm")?;
    let signature_alg_oid = parse_algorithm_oid(sig_alg.content)?;
    let signature = expect_tlv(
        signer_info.content,
        &mut pos,
        0x04,
        "signature OCTET STRING",
    )?;

    Ok(ParsedCms {
        certificates,
        signature: signature.content.to_vec(),
        signature_alg_oid,
    })
}

/// The outcome of validating one signature's CMS container.
pub(crate) struct CmsValidation {
    /// Whether the signature verified against the embedded certificate.
    pub valid: bool,
    /// DER of the certificate whose key verified the signature.
    pub signer_certificate: Option<Vec<u8>>,
    /// Every certificate carried by the CMS container.
    pub certificates: Vec<Vec<u8>>,
}

/// Attempt cryptographic validation of one signature dictionary.
///
/// Returns `Some(true/false)` when `/Contents` holds a parseable CMS
/// `SignedData` with a supported algorithm and an embedded certificate,
/// and `None` when the contents are not CMS (e.g. the legacy structural
/// placeholder), leaving `valid` unset.
pub(crate) fn try_cryptographic_validation(
    pdf_bytes: &[u8],
    byte_range: Option<&str>,
    contents_hex: &str,
) -> Result<Option<bool>> {
    Ok(validate_cms(pdf_bytes, byte_range, contents_hex)?.map(|validation| validation.valid))
}

/// Validate the CMS container of one signature, reporting which certificate
/// verified it so the chain can be built.
pub(crate) fn validate_cms(
    pdf_bytes: &[u8],
    byte_range: Option<&str>,
    contents_hex: &str,
) -> Result<Option<CmsValidation>> {
    let br = byte_range.and_then(parse_byte_range);
    let br = match br {
        Some(br) => br,
        None => return Ok(None),
    };
    let cms_der = match decode_hex_strict(contents_hex) {
        Some(der) => der,
        None => return Ok(None),
    };
    let parsed = match parse_cms_signed_data(&cms_der) {
        Ok(parsed) => parsed,
        // Not CMS-shaped (legacy hash placeholder etc.): structural only.
        Err(_) => return Ok(None),
    };

    // Recompute the digest over the ranges the signature dict declares.
    let (r1_off, r1_len, r2_off, r2_len) = br;
    let end1 = r1_off.checked_add(r1_len);
    let end2 = r2_off.checked_add(r2_len);
    let (Some(end1), Some(end2)) = (end1, end2) else {
        return Err(PdfError::Crypto("ByteRange overflow".into()));
    };
    if end1 > pdf_bytes.len() || end2 > pdf_bytes.len() {
        return Err(PdfError::Crypto(
            "ByteRange extends beyond the end of the document".into(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(&pdf_bytes[r1_off..end1]);
    hasher.update(&pdf_bytes[r2_off..end2]);
    let digest = hasher.finalize();

    for cert_der in &parsed.certificates {
        if verify_with_certificate(
            cert_der,
            digest.as_slice().try_into().expect("sha256 length"),
            &parsed.signature,
            &parsed.signature_alg_oid,
        )
        .unwrap_or(false)
        {
            return Ok(Some(CmsValidation {
                valid: true,
                signer_certificate: Some(cert_der.clone()),
                certificates: parsed.certificates.clone(),
            }));
        }
    }
    Ok(Some(CmsValidation {
        valid: false,
        signer_certificate: None,
        certificates: parsed.certificates.clone(),
    }))
}

fn verify_with_certificate(
    cert_der: &[u8],
    digest: &[u8; 32],
    signature: &[u8],
    alg_oid: &[u8],
) -> Result<bool> {
    use p256::ecdsa::signature::hazmat::PrehashVerifier;
    use rsa::pkcs8::DecodePublicKey;
    let cert = Certificate::from_der(cert_der)
        .map_err(|e| PdfError::Crypto(format!("embedded certificate parse failed: {e}")))?;
    let spki = &cert.tbs_certificate.subject_public_key_info;
    let key_alg_oid = spki.algorithm.oid.as_bytes();

    // The certificate's key algorithm (SPKI) differs from the signature
    // algorithm: EC keys carry id-ecPublicKey, while the SignerInfo
    // signatureAlgorithm is ecdsa-with-SHA256.
    let ec_key = key_alg_oid == OID_EC_PUBLIC_KEY;
    let rsa_key = key_alg_oid == OID_RSA_ENCRYPTION;

    if rsa_key && alg_oid == OID_RSA_ENCRYPTION {
        let spki_der = spki
            .to_der()
            .map_err(|e| PdfError::Crypto(format!("SPKI encode failed: {e}")))?;
        let public = rsa::RsaPublicKey::from_public_key_der(&spki_der)
            .map_err(|e| PdfError::Crypto(format!("RSA public key parse failed: {e}")))?;
        let verifying = rsa::pkcs1v15::VerifyingKey::<Sha256>::new(public);
        let sig = rsa::pkcs1v15::Signature::try_from(signature)
            .map_err(|e| PdfError::Crypto(format!("RSA signature parse failed: {e}")))?;
        Ok(verifying.verify_prehash(digest, &sig).is_ok())
    } else if ec_key && alg_oid == OID_ECDSA_WITH_SHA256 {
        let point = spki.subject_public_key.raw_bytes();
        let verifying = p256::ecdsa::VerifyingKey::from_sec1_bytes(point)
            .map_err(|e| PdfError::Crypto(format!("ECDSA public key parse failed: {e}")))?;
        let sig = p256::ecdsa::DerSignature::try_from(signature)
            .map_err(|e| PdfError::Crypto(format!("ECDSA signature parse failed: {e}")))?;
        Ok(verifying.verify_prehash(digest, &sig).is_ok())
    } else {
        // Key/certificate algorithm not supported here.
        Ok(false)
    }
}

/// Parse a PDF `/ByteRange [a b c d]` value.
fn parse_byte_range(value: &str) -> Option<(usize, usize, usize, usize)> {
    let inner = value.trim().trim_start_matches('[').trim_end_matches(']');
    let nums: Vec<Option<usize>> = inner
        .split_whitespace()
        .map(|t| t.parse::<usize>().ok())
        .collect();
    match nums.as_slice() {
        [Some(a), Some(b), Some(c), Some(d)] => Some((*a, *b, *c, *d)),
        _ => None,
    }
}

/// Decode padded `/Contents` hex, rejecting odd length or non-hex input.
///
/// ponytail: thin strict wrapper over the crate-wide
/// `compression::decode_hex_string`; strictness (odd length / bad digits →
/// `None`) matters because this feeds signature verification.
fn decode_hex_strict(hex: &str) -> Option<Vec<u8>> {
    let cleaned: String = hex.chars().filter(|c| !c.is_whitespace()).collect();
    if !cleaned.len().is_multiple_of(2) {
        return None;
    }
    crate::compression::decode_hex_string(&cleaned).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn der_length_forms() {
        assert_eq!(der_len(0), vec![0x00]);
        assert_eq!(der_len(0x7F), vec![0x7F]);
        assert_eq!(der_len(0x80), vec![0x81, 0x80]);
        assert_eq!(der_len(0x1234), vec![0x82, 0x12, 0x34]);
    }

    #[test]
    fn positive_integer_encoding() {
        assert_eq!(der_positive_integer(&[0]), vec![0x02, 0x01, 0x00]);
        assert_eq!(der_positive_integer(&[0x7F]), vec![0x02, 0x01, 0x7F]);
        assert_eq!(der_positive_integer(&[0x80]), vec![0x02, 0x02, 0x00, 0x80]);
        assert_eq!(
            der_positive_integer(&[0x01, 0x00]),
            vec![0x02, 0x02, 0x01, 0x00]
        );
    }

    #[test]
    fn reader_round_trips_writer() {
        let inner = der_sequence(&[&der_oid(OID_ID_DATA), &der_octet_string(b"payload")]);
        let outer = der_sequence(&[&inner, &tlv(0xA0, &der_positive_integer(&[0x80, 0x7F]))]);
        let mut pos = 0usize;
        let seq = expect_tlv(&outer, &mut pos, 0x30, "outer").unwrap();
        assert_eq!(pos, outer.len());
        let mut pos = 0usize;
        let inner_tlv = expect_tlv(seq.content, &mut pos, 0x30, "inner").unwrap();
        let ctx = read_tlv(seq.content, &mut pos).unwrap();
        assert_eq!(ctx.tag, 0xA0);
        assert_eq!(ctx.content, &[0x02, 0x03, 0x00, 0x80, 0x7F]);
        let mut pos = 0usize;
        let oid = expect_tlv(inner_tlv.content, &mut pos, 0x06, "oid").unwrap();
        assert_eq!(oid.content, OID_ID_DATA);
        let octet = expect_tlv(inner_tlv.content, &mut pos, 0x04, "octet").unwrap();
        assert_eq!(octet.content, b"payload");
    }

    #[test]
    fn reader_rejects_truncated_input() {
        let full = der_sequence(&[&der_octet_string(b"abc")]);
        for cut in 0..full.len() {
            let mut pos = 0usize;
            assert!(read_tlv(&full[..cut], &mut pos).is_err());
        }
    }

    #[test]
    fn cms_round_trip_parse() {
        // Deterministic synthetic SignedData: fixture certificate, synthetic
        // signature. Exercises the writer/parser round trip without keys.
        let cert = parse_signer_cert_fixture();
        let version = der_positive_integer(&[1]);
        let digest_algs = der_set(&[&der_sequence(&[&der_oid(OID_SHA256)])]);
        let encap = der_sequence(&[&der_oid(OID_ID_DATA)]);
        let issuer_and_serial =
            der_sequence(&[&cert.issuer_der, &der_positive_integer(&cert.serial)]);
        let signer_info = der_sequence(&[
            &version,
            &issuer_and_serial,
            &der_sequence(&[&der_oid(OID_SHA256)]),
            &der_sequence(&[&der_oid(OID_RSA_ENCRYPTION), &tlv(0x05, &[])]),
            &der_octet_string(&[0xAA; 16]),
        ]);
        let certificates = tlv(0xA0, &der_set(&[&cert.der]));
        let signed_data = der_sequence(&[
            &version,
            &digest_algs,
            &encap,
            &certificates,
            &der_set(&[&signer_info]),
        ]);
        let content_info = der_sequence(&[&der_oid(OID_ID_SIGNED_DATA), &tlv(0xA0, &signed_data)]);

        let parsed = parse_cms_signed_data(&content_info).expect("parse CMS");
        assert_eq!(parsed.certificates, vec![cert.der.clone()]);
        assert_eq!(parsed.signature, vec![0xAA; 16]);
        assert_eq!(parsed.signature_alg_oid, OID_RSA_ENCRYPTION);
    }

    fn parse_signer_cert_fixture() -> SignerCert {
        let pem = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/test_cert.pem"
        ))
        .expect("fixture certificate");
        parse_signer_cert(&pem).expect("parse signer cert")
    }

    #[test]
    fn parse_byte_range_values() {
        assert_eq!(
            parse_byte_range("[0 1234 1254 678]"),
            Some((0, 1234, 1254, 678))
        );
        assert_eq!(parse_byte_range("[]"), None);
        assert_eq!(parse_byte_range("[1 2 3]"), None);
    }

    #[test]
    fn hex_decode() {
        assert_eq!(decode_hex_strict("00FF10"), Some(vec![0x00, 0xFF, 0x10]));
        assert_eq!(decode_hex_strict("00 ff"), Some(vec![0x00, 0xFF]));
        assert_eq!(decode_hex_strict("00F"), None);
        assert_eq!(decode_hex_strict("ZZ"), None);
    }
}
