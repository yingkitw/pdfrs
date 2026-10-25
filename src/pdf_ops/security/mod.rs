//! PDF security: password protection, digital signatures, and certificate
//! extraction.
//!
//! The module is split into focused submodules:
//!
//! - [`encrypt`] — password protection and permission enforcement
//! - [`objects`] — raw PDF object scanning used by the encryption path
//! - [`sign`] — legacy structural (non-cryptographic) signing
//! - [`verify`] — signature inspection and verification
//! - [`certificates`] — extracting embedded certificates
//!
//! Cryptographic PKCS#7/CMS signing lives in [`signing`](crate::pdf_ops::signing).

mod certificates;
mod encrypt;
mod objects;
mod regexes;
mod sign;
mod verify;

pub use certificates::{extract_certificates_from_pdf, extract_certificates_from_pdf_bytes};
pub use encrypt::{encrypt_pdf_bytes, encrypt_pdf_bytes_with_id, protect_pdf};
#[cfg(feature = "sign")]
pub(crate) use sign::assemble_signature_update;
pub use sign::{sign_pdf, sign_pdf_bytes, sign_pdf_with_certificate};
#[cfg(feature = "sign")]
pub use sign::{sign_pdf_cms, sign_pdf_cms_bytes};
pub use verify::{SignatureInfo, verify_pdf_signature, verify_pdf_signature_bytes};
#[cfg(feature = "sign")]
pub use verify::{TrustedSignatureInfo, verify_pdf_signature_bytes_with_trust};
