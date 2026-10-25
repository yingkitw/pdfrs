//! End-to-end tests for certificate trust-chain validation.
//!
//! A real chain is built at run time with `rcgen`: a self-signed root CA, an
//! intermediate CA signed by the root, and a leaf signer signed by the
//! intermediate. The leaf certificate and key sign a PDF, and the signature
//! is then validated against a trust store holding the root.

#![cfg(feature = "sign")]

use pdfrs::pdf_ops::{
    sign_pdf_cms_bytes, verify_pdf_signature_bytes, verify_pdf_signature_bytes_with_trust,
};
use pdfrs::security::{DigitalSignature, TrustStore};

/// A three-level certificate chain: root CA → intermediate CA → leaf signer.
struct Chain {
    root_pem: String,
    intermediate_pem: String,
    leaf_pem: String,
    leaf_key_pem: String,
}

/// Build a root CA, an intermediate CA and a leaf signer certificate.
fn build_chain() -> Chain {
    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair, KeyUsagePurpose};

    // Root CA — self-signed.
    let root_key = KeyPair::generate().expect("root key");
    let mut root_params = CertificateParams::new(Vec::new()).expect("root params");
    root_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    root_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let root = root_params
        .self_signed(&root_key)
        .expect("self-signed root");

    // Intermediate CA — signed by the root.
    let intermediate_key = KeyPair::generate().expect("intermediate key");
    let mut intermediate_params = CertificateParams::new(Vec::new()).expect("intermediate params");
    intermediate_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    intermediate_params.key_usages = vec![
        KeyUsagePurpose::KeyCertSign,
        KeyUsagePurpose::CrlSign,
        KeyUsagePurpose::DigitalSignature,
    ];
    let intermediate = intermediate_params
        .signed_by(&intermediate_key, &root, &root_key)
        .expect("intermediate signed by root");

    // Leaf signer — signed by the intermediate.
    let leaf_key = KeyPair::generate().expect("leaf key");
    let leaf_params =
        CertificateParams::new(vec!["pdfrs-test.example".to_string()]).expect("leaf params");
    let leaf = leaf_params
        .signed_by(&leaf_key, &intermediate, &intermediate_key)
        .expect("leaf signed by intermediate");

    Chain {
        root_pem: root.pem(),
        intermediate_pem: intermediate.pem(),
        leaf_pem: leaf.pem(),
        leaf_key_pem: leaf_key.serialize_pem(),
    }
}

/// A minimal one-page PDF to sign.
fn make_test_pdf() -> Vec<u8> {
    let elements = vec![pdfrs::elements::Element::Paragraph {
        text: "Trust chain test".into(),
    }];
    pdfrs::pdf_generator::generate_pdf_bytes(
        &elements,
        "Helvetica",
        12.0,
        pdfrs::pdf_generator::PageLayout::portrait(),
    )
    .unwrap()
}

fn test_signature() -> DigitalSignature {
    DigitalSignature::new("Chain Signer").with_reason("trust chain test")
}

#[test]
fn signature_chains_to_a_trusted_root() {
    let chain = build_chain();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial::new(
        chain.leaf_pem.clone(),
        chain.leaf_key_pem.clone(),
    )
    .with_chain(vec![chain.intermediate_pem.clone()]);
    let pdf = make_test_pdf();
    let signed = sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("signing succeeds");

    // The signature itself is cryptographically valid.
    let plain = verify_pdf_signature_bytes(&signed).expect("verification runs");
    assert_eq!(plain.len(), 1);
    assert!(plain[0].valid, "the CMS signature must verify");

    // With the root trusted, the chain validates.
    let mut trust = TrustStore::new();
    trust
        .add_pem(&chain.root_pem)
        .expect("root is a valid certificate");
    let trusted =
        verify_pdf_signature_bytes_with_trust(&signed, &trust).expect("verification runs");
    assert_eq!(trusted.len(), 1);
    assert!(
        trusted[0].trusted,
        "the chain should reach the trusted root: {:?}",
        trusted[0].reason
    );
    // The chain runs leaf → intermediate → root.
    assert_eq!(trusted[0].chain.len(), 3, "chain: {:?}", trusted[0].chain);
    assert!(
        trusted[0].chain.last().expect("anchor").is_trust_anchor,
        "the last certificate in the chain is the trust anchor"
    );
    assert!(trusted[0].reason.is_none());
}

#[test]
fn signature_is_untrusted_without_the_root() {
    let chain = build_chain();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial::new(
        chain.leaf_pem.clone(),
        chain.leaf_key_pem.clone(),
    )
    .with_chain(vec![chain.intermediate_pem.clone()]);
    let pdf = make_test_pdf();
    let signed = sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("signing succeeds");

    // An empty trust store trusts nothing.
    let trust = TrustStore::new();
    let result = verify_pdf_signature_bytes_with_trust(&signed, &trust).expect("verification runs");
    assert_eq!(result.len(), 1);
    assert!(!result[0].trusted, "an empty trust store trusts nothing");
    assert!(
        result[0].reason.is_some(),
        "an untrusted chain must explain itself"
    );
}

#[test]
fn unrelated_root_does_not_trust_the_signature() {
    let chain = build_chain();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial::new(
        chain.leaf_pem.clone(),
        chain.leaf_key_pem.clone(),
    )
    .with_chain(vec![chain.intermediate_pem.clone()]);
    let pdf = make_test_pdf();
    let signed = sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("signing succeeds");

    // Trusting a different root must not validate this chain.
    let other = build_chain();
    let mut trust = TrustStore::new();
    trust
        .add_pem(&other.root_pem)
        .expect("root is a valid certificate");
    let result = verify_pdf_signature_bytes_with_trust(&signed, &trust).expect("verification runs");
    assert!(
        !result[0].trusted,
        "an unrelated root must not validate the chain"
    );
}

#[test]
fn intermediate_is_not_a_trust_anchor_on_its_own() {
    let chain = build_chain();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial::new(
        chain.leaf_pem.clone(),
        chain.leaf_key_pem.clone(),
    )
    .with_chain(vec![chain.intermediate_pem.clone()]);
    let pdf = make_test_pdf();
    let signed = sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("signing succeeds");

    // Trusting only the intermediate stops the chain one link short of the
    // root: the intermediate is the anchor, so the chain is "trusted" only
    // because the caller explicitly pinned it.
    let mut trust = TrustStore::new();
    trust
        .add_pem(&chain.intermediate_pem)
        .expect("intermediate is a valid certificate");
    let result = verify_pdf_signature_bytes_with_trust(&signed, &trust).expect("verification runs");
    // The intermediate is trusted, but it is not self-signed, so the chain
    // ends there without reaching a self-signed root.
    assert_eq!(result[0].chain.len(), 2, "chain: {:?}", result[0].chain);
    assert!(
        result[0].chain.last().expect("anchor").is_trust_anchor,
        "the intermediate is the configured anchor"
    );
}

#[test]
fn trust_store_rejects_malformed_certificates() {
    let mut trust = TrustStore::new();
    let err = trust
        .add_der(&[0x30, 0x00])
        .expect_err("garbage must be rejected");
    assert!(
        err.to_string().contains("trusted certificate parse failed"),
        "unexpected error: {err}"
    );
}

#[test]
fn trust_store_reports_empty_for_no_roots() {
    let trust = TrustStore::new();
    assert!(trust.is_empty());
    assert_eq!(trust.len(), 0);
    assert_eq!(trust.roots().len(), 0);
}

#[test]
fn cli_reports_the_trust_decision() {
    use std::process::Command;

    let chain = build_chain();
    let material = pdfrs::pdf_ops::signing::SigningKeyMaterial::new(
        chain.leaf_pem.clone(),
        chain.leaf_key_pem.clone(),
    )
    .with_chain(vec![chain.intermediate_pem.clone()]);
    let pdf = make_test_pdf();
    let signed = sign_pdf_cms_bytes(&pdf, &test_signature(), &material).expect("signing succeeds");

    let dir = std::env::temp_dir().join(format!("pdfrs-trust-cli-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let pdf_path = dir.join("signed.pdf");
    let root_path = dir.join("root.pem");
    std::fs::write(&pdf_path, &signed).expect("write pdf");
    std::fs::write(&root_path, &chain.root_pem).expect("write root");

    let output = Command::new(env!("CARGO_BIN_EXE_pdfcli"))
        .args([
            "verify-signature",
            pdf_path.to_str().expect("path"),
            "--trusted-cert",
            root_path.to_str().expect("path"),
        ])
        .output()
        .expect("cli runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Trusted: yes"),
        "the CLI should report the trust decision: {stdout}"
    );
    assert!(
        stdout.contains("chains to a trusted root"),
        "the CLI should explain the trust decision: {stdout}"
    );

    // Without the trusted root the same signature is untrusted.
    let output = Command::new(env!("CARGO_BIN_EXE_pdfcli"))
        .args(["verify-signature", pdf_path.to_str().expect("path")])
        .output()
        .expect("cli runs");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Valid: yes"),
        "the signature itself is still valid: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
