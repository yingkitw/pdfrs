//! End-to-end tests for real PKCS#7/CMS cryptographic signing.
//!
//! Uses `rcgen` (dev-dependency) to generate a self-signed ECDSA P-256
//! certificate/key pair, signs a generated PDF with
//! [`pdfrs::pdf_ops::sign_pdf_cms_bytes`], and verifies the signature
//! cryptographically — including tamper detection.

#![cfg(feature = "sign")]

use pdfrs::pdf_ops::{sign_pdf_bytes, sign_pdf_cms_bytes, verify_pdf_signature_bytes};
use pdfrs::security::DigitalSignature;
use rcgen::generate_simple_self_signed;

/// Generate a minimal one-page PDF for signing tests.
fn make_test_pdf() -> Vec<u8> {
    let elements = vec![
        pdfrs::elements::Element::Heading {
            text: "Signing Test".into(),
            level: 1,
        },
        pdfrs::elements::Element::Paragraph {
            text: "This document will be signed with a real CMS signature.".into(),
        },
    ];
    pdfrs::pdf_generator::generate_pdf_bytes(
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap()
}

/// Generate a self-signed ECDSA P-256 certificate + PKCS#8 key (PEM).
fn make_signing_material() -> (String, String) {
    let certified = generate_simple_self_signed(vec!["pdfrs-test.example".to_string()])
        .expect("generate test certificate");
    (certified.cert.pem(), certified.key_pair.serialize_pem())
}

/// Generate a self-signed RSA certificate + PKCS#8 key (PEM).
///
/// The key pair is produced at run time rather than checked in as a fixture,
/// so no private key material is ever stored in the repository.
fn make_rsa_signing_material() -> (String, String) {
    use rsa::pkcs8::EncodePrivateKey;

    let mut rng = rand::rngs::OsRng;
    let private = rsa::RsaPrivateKey::new(&mut rng, 2048).expect("generate RSA key");
    let key_pem = private
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .expect("serialize PKCS#8 PEM");

    // `rcgen` cannot generate RSA keys, but it can sign a certificate with an
    // existing one.
    let key_pair = rcgen::KeyPair::from_pkcs8_pem_and_sign_algo(&key_pem, &rcgen::PKCS_RSA_SHA256)
        .expect("load RSA key for certificate signing");
    let certificate = rcgen::CertificateParams::new(vec!["pdfrs-test.example".to_string()])
        .expect("certificate parameters")
        .self_signed(&key_pair)
        .expect("self-signed certificate");
    (certificate.pem(), key_pem.to_string())
}

fn test_signature() -> DigitalSignature {
    DigitalSignature::new("Test Signer")
        .with_reason("integration test")
        .with_location("CI")
}

#[test]
fn cms_signature_round_trips_and_verifies() {
    let (cert_pem, key_pem) = make_signing_material();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial { cert_pem, key_pem };
    let pdf = make_test_pdf();

    let signed =
        sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("CMS signing should succeed");

    // The signed document must still parse and keep its content.
    let doc = pdfrs::pdf::PdfDocument::load_from_bytes(&signed).expect("signed PDF parses");
    assert!(!doc.objects.is_empty());

    let sigs = verify_pdf_signature_bytes(&signed).expect("verification runs");
    assert_eq!(sigs.len(), 1, "exactly one signature expected");
    let sig = &sigs[0];
    assert_eq!(sig.signer_name, "Test Signer");
    assert!(
        sig.valid,
        "CMS signature must verify cryptographically: {sig:?}"
    );
}

#[test]
fn tampered_cms_signature_fails_verification() {
    let (cert_pem, key_pem) = make_signing_material();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial { cert_pem, key_pem };
    let pdf = make_test_pdf();

    let mut signed =
        sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("CMS signing should succeed");

    // Tamper with a byte inside the first /ByteRange region (the document
    // body, well before the signature update). Flip a byte in the middle of
    // the original content.
    let mid = pdf.len() / 2;
    let before = signed[mid];
    signed[mid] = before.wrapping_add(1);

    let sigs = verify_pdf_signature_bytes(&signed).expect("verification runs");
    assert_eq!(sigs.len(), 1);
    assert!(
        !sigs[0].valid,
        "signature must not verify after content tampering"
    );
}

#[test]
fn rsa_cms_signature_round_trips_and_verifies() {
    // RSA path: a self-signed certificate and key generated at run time.
    let (cert_pem, key_pem) = make_rsa_signing_material();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial { cert_pem, key_pem };
    let pdf = make_test_pdf();

    let signed =
        sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("CMS signing should succeed");

    let sigs = verify_pdf_signature_bytes(&signed).expect("verification runs");
    assert_eq!(sigs.len(), 1);
    assert!(
        sigs[0].valid,
        "RSA CMS signature must verify cryptographically: {:?}",
        sigs[0]
    );

    // Tamper detection must hold for RSA as well.
    let mut tampered = signed.clone();
    let mid = pdf.len() / 2;
    tampered[mid] = tampered[mid].wrapping_add(1);
    let sigs = verify_pdf_signature_bytes(&tampered).expect("verification runs");
    assert!(!sigs[0].valid, "tampered RSA signature must fail");
}

#[test]
fn placeholder_signature_is_reported_invalid() {
    // The legacy structural signer emits a raw hash, not a CMS container;
    // verification must honestly report it as not cryptographically valid.
    let pdf = make_test_pdf();
    let sig = test_signature();
    let signed = sign_pdf_bytes(&pdf, &sig, None).expect("placeholder signing succeeds");

    let sigs = verify_pdf_signature_bytes(&signed).expect("verification runs");
    assert_eq!(sigs.len(), 1);
    assert!(
        !sigs[0].valid,
        "placeholder (non-CMS) signature must not be reported valid"
    );
}

#[test]
fn unsigned_pdf_has_no_signatures() {
    let pdf = make_test_pdf();
    let sigs = verify_pdf_signature_bytes(&pdf).expect("verification runs");
    assert!(sigs.is_empty(), "unsigned PDF has no signatures");
}

#[test]
fn unsupported_key_type_is_rejected_with_clear_error() {
    // An Ed25519 PKCS#8 key is valid PKCS#8 but unsupported by the CMS signer.
    let key_pair = rcgen::KeyPair::generate_for(&rcgen::PKCS_ED25519).expect("ed25519 key");
    let key_pem = key_pair.serialize_pem();
    let cert_pem = String::from("-----BEGIN CERTIFICATE-----\nMA==\n-----END CERTIFICATE-----\n");
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial { cert_pem, key_pem };
    let pdf = make_test_pdf();

    let err = sign_pdf_cms_bytes(&pdf, &test_signature(), &material)
        .expect_err("unsupported key must fail");
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported private key") || msg.contains("certificate PEM"),
        "error should explain the problem: {msg}"
    );
}
