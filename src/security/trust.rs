//! Certificate trust chains: a trust store of trusted roots and the chain
//! builder that links a signer certificate back to one of them.
//!
//! Cryptographic validation of a PDF signature proves that the bytes covered
//! by `/ByteRange` were signed by the key in the embedded certificate. It says
//! nothing about *who* that key belongs to. This module closes that gap: given
//! a set of trusted root certificates, the signer's certificate is chained
//! through its issuers (each link verified with the issuer's public key) until
//! a trusted root is reached.
//!
//! Revocation is **not** evaluated: CRL and OCSP checking need network access,
//! which is out of scope for a library. A chain that validates here may still
//! have been revoked by its issuer.

use std::time::{SystemTime, UNIX_EPOCH};

use der::{Decode, Encode};
use sha2::{Digest, Sha256};
use x509_cert::certificate::Certificate;
use x509_cert::time::Time;

use crate::error::{PdfError, Result};

/// Maximum number of certificates in a chain (leaf plus issuers).
const MAX_CHAIN_DEPTH: usize = 10;

/// Object identifiers used when verifying certificate signatures.
mod oid {
    /// `rsaEncryption`
    pub(super) const RSA_ENCRYPTION: &[u8] =
        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01];
    /// `id-ecPublicKey`
    pub(super) const EC_PUBLIC_KEY: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x02, 0x01];
    /// `sha256WithRSAEncryption`
    pub(super) const SHA256_WITH_RSA: &[u8] =
        &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B];
    /// `ecdsa-with-SHA256`
    pub(super) const ECDSA_WITH_SHA256: &[u8] = &[0x2A, 0x86, 0x48, 0xCE, 0x3D, 0x04, 0x03, 0x02];
}

/// One certificate in a validated chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustedCertificate {
    /// Hex-encoded DER of the certificate, for identification.
    pub der_hex: String,
    /// Subject distinguished name (best effort, human readable).
    pub subject: String,
    /// Issuer distinguished name (best effort, human readable).
    pub issuer: String,
    /// True when this certificate is the trust anchor the chain ended at.
    pub is_trust_anchor: bool,
}

/// The outcome of evaluating a certificate chain against a trust store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustEvaluation {
    /// True when the chain reaches a trusted root certificate.
    pub trusted: bool,
    /// The chain, leaf certificate first.
    pub chain: Vec<TrustedCertificate>,
    /// Why the chain was rejected, when it was.
    pub reason: Option<String>,
}

impl TrustEvaluation {
    fn rejected(chain: Vec<TrustedCertificate>, reason: impl Into<String>) -> Self {
        TrustEvaluation {
            trusted: false,
            chain,
            reason: Some(reason.into()),
        }
    }
}

/// A set of trusted root certificates.
///
/// Roots are held as DER; a certificate is recognised as a trust anchor when
/// its DER encoding matches one of them exactly.
#[derive(Debug, Clone, Default)]
pub struct TrustStore {
    roots: Vec<Vec<u8>>,
}

impl TrustStore {
    /// An empty trust store (every chain will be untrusted).
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a trust store from a PEM document that may hold several
    /// certificates.
    pub fn from_pem(pem: &str) -> Result<Self> {
        let mut store = Self::new();
        for der in split_pem_certificates(pem)? {
            store.add_der(&der)?;
        }
        Ok(store)
    }

    /// Load every `*.pem` file in `directory` as a trusted root.
    pub fn from_directory(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let mut store = Self::new();
        let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(directory)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("pem"))
            .collect();
        entries.sort();
        for path in entries {
            let pem = std::fs::read_to_string(&path)?;
            store.add_pem(&pem)?;
        }
        Ok(store)
    }

    /// Add a PEM certificate (possibly holding several) as a trusted root.
    pub fn add_pem(&mut self, pem: &str) -> Result<()> {
        for der in split_pem_certificates(pem)? {
            self.add_der(&der)?;
        }
        Ok(())
    }

    /// Add a DER certificate as a trusted root.
    pub fn add_der(&mut self, der: &[u8]) -> Result<()> {
        // Reject certificates that do not parse, so a typo in a trust store is
        // reported instead of silently never matching.
        Certificate::from_der(der)
            .map_err(|e| PdfError::Crypto(format!("trusted certificate parse failed: {e}")))?;
        if !self.roots.iter().any(|root| root == der) {
            self.roots.push(der.to_vec());
        }
        Ok(())
    }

    /// Number of trusted roots.
    pub fn len(&self) -> usize {
        self.roots.len()
    }

    /// True when no trusted roots are configured.
    pub fn is_empty(&self) -> bool {
        self.roots.is_empty()
    }

    /// Whether `der` is one of the trusted roots.
    pub fn contains_der(&self, der: &[u8]) -> bool {
        self.roots.iter().any(|root| root == der)
    }

    /// The DER encodings of the trusted roots.
    pub fn roots(&self) -> &[Vec<u8>] {
        &self.roots
    }

    /// Add every root of `other` to this store.
    pub fn extend(&mut self, other: &TrustStore) {
        for root in &other.roots {
            if !self.roots.iter().any(|existing| existing == root) {
                self.roots.push(root.clone());
            }
        }
    }
}

/// Evaluate a certificate chain.
///
/// `leaf_der` is the signer's certificate; `pool` holds the other
/// certificates carried by the signature (typically the intermediate CAs).
/// The chain is built by repeatedly finding an issuer whose subject matches
/// the current certificate's issuer name and whose public key verifies the
/// current certificate's signature.
pub fn evaluate_chain(leaf_der: &[u8], pool: &[Vec<u8>], trust: &TrustStore) -> TrustEvaluation {
    let mut chain: Vec<TrustedCertificate> = Vec::new();
    let mut current = leaf_der.to_vec();
    let mut visited: Vec<Vec<u8>> = Vec::new();

    for _ in 0..MAX_CHAIN_DEPTH {
        if visited.contains(&current) {
            return TrustEvaluation::rejected(chain, "certificate chain contains a loop");
        }
        visited.push(current.clone());

        let Ok(cert) = Certificate::from_der(&current) else {
            return TrustEvaluation::rejected(
                chain,
                "certificate in the chain could not be parsed",
            );
        };

        let is_anchor = trust.contains_der(&current);
        if !is_anchor && let Some(reason) = validity_problem(&cert) {
            return TrustEvaluation::rejected(chain, reason);
        }

        chain.push(TrustedCertificate {
            der_hex: to_hex(&current),
            subject: subject_of(&cert),
            issuer: issuer_of(&cert),
            is_trust_anchor: is_anchor,
        });

        if is_anchor {
            return TrustEvaluation {
                trusted: true,
                chain,
                reason: None,
            };
        }

        let Some(issuer_der) = find_issuer(&cert, &current, pool, trust) else {
            return TrustEvaluation::rejected(
                chain,
                format!("no trusted issuer found for '{}'", subject_of(&cert)),
            );
        };
        current = issuer_der;
    }

    TrustEvaluation::rejected(chain, "certificate chain is too long")
}

fn find_issuer(
    cert: &Certificate,
    cert_der: &[u8],
    pool: &[Vec<u8>],
    trust: &TrustStore,
) -> Option<Vec<u8>> {
    let issuer_name = cert.tbs_certificate.issuer.to_der().ok()?;

    for candidate in pool.iter().chain(trust.roots.iter()) {
        // Prefer the original DER encoding over a re-encoded parse of `cert`.
        if candidate.as_slice() == cert_der {
            continue;
        }
        let Ok(issuer) = Certificate::from_der(candidate) else {
            continue;
        };
        // The issuer's subject must be the certificate's issuer name.
        let Ok(subject) = issuer.tbs_certificate.subject.to_der() else {
            continue;
        };
        if subject != issuer_name {
            continue;
        }
        // And the issuer's key must verify the certificate's signature.
        if verify_certificate_signature(cert, cert_der, &issuer) {
            return Some(candidate.clone());
        }
    }
    None
}

/// Verify `child`'s signature with `issuer`'s public key.
///
/// The signature covers the certificate's `tbsCertificate` **exactly as it was
/// encoded**, so the original bytes are taken from the DER rather than
/// re-encoded from the parsed structure: a re-encoding can differ (the
/// `version` field, for instance) and would then hash to a different value.
fn verify_certificate_signature(
    child: &Certificate,
    child_der: &[u8],
    issuer: &Certificate,
) -> bool {
    let Some(tbs) = tbs_bytes(child_der) else {
        return false;
    };
    let Some(signature) = child.signature.as_bytes() else {
        return false;
    };
    let alg_oid = child.signature_algorithm.oid.as_bytes();
    let spki = &issuer.tbs_certificate.subject_public_key_info;
    let key_oid = spki.algorithm.oid.as_bytes();

    if key_oid == oid::RSA_ENCRYPTION && alg_oid == oid::SHA256_WITH_RSA {
        use rsa::pkcs8::DecodePublicKey;
        use rsa::signature::hazmat::PrehashVerifier;
        let Ok(spki_der) = spki.to_der() else {
            return false;
        };
        let Ok(public) = rsa::RsaPublicKey::from_public_key_der(&spki_der) else {
            return false;
        };
        let verifying = rsa::pkcs1v15::VerifyingKey::<Sha256>::new(public);
        let Ok(sig) = rsa::pkcs1v15::Signature::try_from(signature) else {
            return false;
        };
        verifying
            .verify_prehash(&Sha256::digest(&tbs), &sig)
            .is_ok()
    } else if key_oid == oid::EC_PUBLIC_KEY && alg_oid == oid::ECDSA_WITH_SHA256 {
        use p256::ecdsa::signature::hazmat::PrehashVerifier;
        let Ok(point) =
            p256::ecdsa::VerifyingKey::from_sec1_bytes(spki.subject_public_key.raw_bytes())
        else {
            return false;
        };
        let digest = Sha256::digest(&tbs);
        // RFC 5758 requires the DER `ECDSA-Sig-Value`; some producers write
        // the fixed-length r||s pair instead, so both are accepted.
        if let Ok(signature) = p256::ecdsa::DerSignature::try_from(signature) {
            return point.verify_prehash(&digest, &signature).is_ok();
        }
        match p256::ecdsa::Signature::from_slice(signature) {
            Ok(signature) => point.verify_prehash(&digest, &signature).is_ok(),
            Err(_) => false,
        }
    } else {
        false
    }
}

/// The original DER encoding of a certificate's `tbsCertificate` (tag + length +
/// value), taken as a slice of `cert_der` without re-encoding.
///
/// X.509 signatures cover those exact bytes. Re-encoding from a parsed
/// structure (or via `Any::to_der`) can change length-form or field order and
/// then fail verification even when the certificate is valid.
fn tbs_bytes(cert_der: &[u8]) -> Option<Vec<u8>> {
    if cert_der.first().copied()? != 0x30 {
        return None;
    }
    // Skip the outer Certificate SEQUENCE header.
    let mut pos = 1usize;
    let content_start = der_skip_length(cert_der, &mut pos)?;
    // First TLV inside is tbsCertificate.
    let start = content_start;
    let mut p = start;
    let _tag = *cert_der.get(p)?;
    p += 1;
    let (len, value_start) = der_read_length(cert_der, p)?;
    let end = value_start.checked_add(len)?;
    if end > cert_der.len() {
        return None;
    }
    Some(cert_der[start..end].to_vec())
}

/// Advance `pos` past a DER length field; return the index of the value.
fn der_skip_length(bytes: &[u8], pos: &mut usize) -> Option<usize> {
    let b = *bytes.get(*pos)?;
    if b < 0x80 {
        *pos += 1;
        Some(*pos)
    } else {
        let n = (b & 0x7f) as usize;
        *pos += 1 + n;
        if *pos > bytes.len() {
            return None;
        }
        Some(*pos)
    }
}

/// Read a DER length at `pos`; return `(value_len, index_of_value)`.
fn der_read_length(bytes: &[u8], pos: usize) -> Option<(usize, usize)> {
    let b = *bytes.get(pos)?;
    if b < 0x80 {
        Some((b as usize, pos + 1))
    } else {
        let n = (b & 0x7f) as usize;
        if n == 0 || n > 4 || pos + 1 + n > bytes.len() {
            return None;
        }
        let mut v = 0usize;
        for i in 0..n {
            v = (v << 8) | bytes[pos + 1 + i] as usize;
        }
        Some((v, pos + 1 + n))
    }
}

/// Report why the certificate is outside its validity window, if it is.
fn validity_problem(cert: &Certificate) -> Option<String> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    let validity = &cert.tbs_certificate.validity;
    let not_before = time_to_unix(&validity.not_before)?;
    let not_after = time_to_unix(&validity.not_after)?;
    if now < not_before {
        return Some(format!(
            "certificate '{}' is not valid yet (valid from {not_before})",
            subject_of(cert)
        ));
    }
    if now > not_after {
        return Some(format!(
            "certificate '{}' has expired (expired at {not_after})",
            subject_of(cert)
        ));
    }
    None
}

/// Convert an X.509 time to seconds since the Unix epoch.
fn time_to_unix(time: &Time) -> Option<u64> {
    match time {
        Time::UtcTime(t) => Some(t.to_unix_duration().as_secs()),
        Time::GeneralTime(t) => Some(t.to_unix_duration().as_secs()),
    }
}

/// A best-effort human-readable subject for a certificate.
fn subject_of(cert: &Certificate) -> String {
    display_name(&cert.tbs_certificate.subject.to_der().unwrap_or_default())
}

/// A best-effort human-readable issuer for a certificate.
fn issuer_of(cert: &Certificate) -> String {
    display_name(&cert.tbs_certificate.issuer.to_der().unwrap_or_default())
}

/// Extract a readable distinguished name from a DER-encoded `Name`.
fn display_name(der: &[u8]) -> String {
    let lossy = String::from_utf8_lossy(der);
    for marker in ["CN=", "O=", "OU="] {
        if let Some(idx) = lossy.find(marker) {
            let slice = &lossy[idx..];
            let end = slice
                .find(['\0', '\x01', '\x02'])
                .unwrap_or(slice.len().min(64));
            let candidate = slice[..end].trim_matches(|c: char| !c.is_ascii_graphic() && c != '=');
            if !candidate.is_empty() {
                return candidate.to_string();
            }
        }
    }
    String::new()
}

/// Split a PEM document into the DER of each certificate it contains.
fn split_pem_certificates(pem: &str) -> Result<Vec<Vec<u8>>> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let mut out = Vec::new();
    let mut rest = pem;
    while let Some(start) = rest.find(BEGIN) {
        let after = &rest[start + BEGIN.len()..];
        let Some(end) = after.find(END) else {
            break;
        };
        let body: String = after[..end]
            .chars()
            .filter(|c| !c.is_ascii_whitespace())
            .collect();
        out.push(crate::security::certificates::pem_body_to_der(&body)?);
        rest = &after[end + END.len()..];
    }
    if out.is_empty() {
        return Err(PdfError::Crypto(
            "no PEM certificate block found in the trust store input".into(),
        ));
    }
    Ok(out)
}

/// Hex-encode bytes.
fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_signed_certificate_verifies_against_itself() {
        use rcgen::{CertificateParams, KeyPair};
        let key = KeyPair::generate().unwrap();
        let params = CertificateParams::new(Vec::new()).unwrap();
        let cert = params.self_signed(&key).unwrap();
        let pem = cert.pem();
        let der = crate::security::certificates::pem_body_to_der(
            &pem.lines()
                .filter(|l| !l.starts_with("-----"))
                .collect::<String>(),
        )
        .unwrap();
        let parsed = Certificate::from_der(&der).unwrap();
        assert!(
            verify_certificate_signature(&parsed, &der, &parsed),
            "a self-signed certificate must verify with its own key"
        );
        let mut store = TrustStore::new();
        store.add_der(&der).unwrap();
        let eval = evaluate_chain(&der, &[], &store);
        assert!(eval.trusted);
        assert_eq!(eval.chain.len(), 1);
        assert!(eval.chain[0].is_trust_anchor);
    }

    #[test]
    fn empty_trust_store_rejects_everything() {
        let store = TrustStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);
        assert!(!store.contains_der(&[1, 2, 3]));
    }

    #[test]
    fn pem_without_certificate_block_is_rejected() {
        let err = TrustStore::from_pem("not a certificate").expect_err("should fail");
        assert!(err.to_string().contains("no PEM certificate block"));
    }

    #[test]
    fn hex_encoding_is_lowercase() {
        assert_eq!(to_hex(&[0x00, 0xAB, 0xFF]), "00abff");
    }
}
