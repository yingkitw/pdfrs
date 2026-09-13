//! `security` subcommand handlers.

use pdfrs::{pdf, pdf_ops, security};

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_sign(
    input: String,
    output: String,
    signer: String,
    reason: Option<String>,
    location: Option<String>,
    contact: Option<String>,
    certificate: Option<String>,
    cert_id: Option<String>,
    cert_store: String,
) {
    {
        let cert = match (&certificate, &cert_id) {
            (Some(path), _) => Some(security::load_certificate_pem("signing-cert", path)),
            (_, Some(id)) => {
                Some(security::CertificateStore::open(&cert_store).and_then(|store| store.get(id)))
            }
            _ => None,
        };

        let cert = match cert {
            Some(Ok(c)) => Some(c),
            Some(Err(e)) => {
                eprintln!("Error loading certificate: {e}");
                return;
            }
            None => None,
        };

        let signer_name = if signer.is_empty() {
            cert.as_ref()
                .map(|c| c.subject.clone())
                .unwrap_or_else(|| "Unknown Signer".to_string())
        } else {
            signer
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let date = format!("{}0000+0000", now);
        let sig = security::DigitalSignature::new(&signer_name).with_date(date);
        let sig = if let Some(r) = reason {
            sig.with_reason(r)
        } else {
            sig
        };
        let sig = if let Some(l) = location {
            sig.with_location(l)
        } else {
            sig
        };
        let sig = if let Some(c) = contact {
            sig.with_contact_info(c)
        } else {
            sig
        };
        match pdf_ops::sign_pdf_with_certificate(&input, &output, &sig, cert.as_ref()) {
            Ok(_) => {
                println!("Successfully signed {} -> {}", input, output);
                if let Some(c) = &cert {
                    println!("  Certificate: {} ({})", c.id, c.fingerprint_sha256);
                }
            }
            Err(e) => eprintln!("Error signing PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_import_certificate(
    id: String,
    file: String,
    subject: Option<String>,
    store: String,
) {
    match security::CertificateStore::open(&store) {
        Ok(cert_store) => match cert_store.import(&id, &file, subject.as_deref()) {
            Ok(cert) => {
                println!("Imported certificate '{}' into {}", id, store);
                println!("  Subject: {}", cert.subject);
                println!("  Fingerprint (SHA-256): {}", cert.fingerprint_sha256);
            }
            Err(e) => eprintln!("Error importing certificate: {}", e),
        },
        Err(e) => eprintln!("Error opening certificate store: {}", e),
    }
}

pub(crate) fn cmd_list_certificates(store: String) {
    match security::CertificateStore::open(&store) {
        Ok(cert_store) => match cert_store.list() {
            Ok(certs) => {
                if certs.is_empty() {
                    println!("No certificates in {}", store);
                } else {
                    println!("Certificates in {}:", store);
                    for cert in certs {
                        println!(
                            "  {} — {} [{}]",
                            cert.id, cert.subject, cert.fingerprint_sha256
                        );
                    }
                }
            }
            Err(e) => eprintln!("Error listing certificates: {}", e),
        },
        Err(e) => eprintln!("Error opening certificate store: {}", e),
    }
}

pub(crate) fn cmd_verify_signature(input: String) {
    match pdf_ops::verify_pdf_signature(&input) {
        Ok(sigs) => {
            if sigs.is_empty() {
                println!("No digital signatures found in {}", input);
            } else {
                println!("Found {} signature(s) in {}:", sigs.len(), input);
                for (i, sig) in sigs.iter().enumerate() {
                    println!("  Signature #{}:", i + 1);
                    println!("    Signer: {}", sig.signer_name);
                    if let Some(ref reason) = sig.reason {
                        println!("    Reason: {}", reason);
                    }
                    if let Some(ref location) = sig.location {
                        println!("    Location: {}", location);
                    }
                    if let Some(ref date) = sig.date {
                        println!("    Date: {}", date);
                    }
                    if let Some(ref subject) = sig.certificate_subject {
                        println!("    Certificate subject: {}", subject);
                    }
                    if let Some(ref fp) = sig.certificate_fingerprint {
                        println!("    Certificate fingerprint: {}", fp);
                    }
                }
            }
        }
        Err(e) => eprintln!("Error verifying signatures: {}", e),
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn cmd_protect(
    input: String,
    output: String,
    user_password: Option<String>,
    owner_password: Option<String>,
    algorithm: String,
    allow_print: bool,
    allow_copy: bool,
    allow_modify: bool,
    allow_annotate: bool,
    allow_fill_forms: bool,
    allow_extract: bool,
    allow_assemble: bool,
    allow_print_high_quality: bool,
    read_only: bool,
) {
    {
        // Check if at least one password is provided
        if user_password.is_none() && owner_password.is_none() {
            eprintln!(
                "Error: At least one of --user-password or --owner-password must be specified"
            );
            return;
        }

        // Parse encryption algorithm
        let encryption_algo = match algorithm.to_lowercase().as_str() {
            "rc4-40" => security::EncryptionAlgorithm::Rc4_40,
            "rc4-128" => security::EncryptionAlgorithm::Rc4_128,
            "aes-128" => security::EncryptionAlgorithm::Aes128,
            "aes-256" => security::EncryptionAlgorithm::Aes256,
            _ => {
                eprintln!(
                    "Error: Invalid algorithm '{}'. Valid options: rc4-40, rc4-128, aes-128, aes-256",
                    algorithm
                );
                return;
            }
        };

        // Create permissions
        let permissions = if read_only {
            security::PdfPermissions::read_only()
        } else {
            security::PdfPermissions {
                print: allow_print,
                copy: allow_copy,
                modify: allow_modify,
                annotate: allow_annotate,
                fill_forms: allow_fill_forms,
                extract: allow_extract,
                assemble: allow_assemble,
                print_high_quality: allow_print_high_quality,
            }
        };

        // Create security settings
        let mut sec = security::PdfSecurity::new()
            .with_encryption(encryption_algo)
            .with_permissions(permissions);

        if let Some(user_pwd) = user_password {
            sec = sec.with_user_password(user_pwd);
        }
        if let Some(owner_pwd) = owner_password {
            sec = sec.with_owner_password(owner_pwd);
        }

        // Validate security settings
        if let Err(e) = sec.validate() {
            eprintln!("Error: {}", e);
            return;
        }

        match pdf_ops::protect_pdf(&input, &output, &sec) {
            Ok(_) => println!("Successfully applied protection to {}", output),
            Err(e) => eprintln!("Error protecting PDF: {}", e),
        }
    }
}

pub(crate) fn cmd_sanitize_pdf(input: String, output: String) {
    match pdf::PdfDocument::load_from_file(&input) {
        Ok(mut doc) => {
            doc.sanitize();
            match std::fs::write(&output, doc.to_bytes()) {
                Ok(_) => println!("Sanitized PDF written to {}", output),
                Err(e) => eprintln!("Error writing sanitized PDF: {}", e),
            }
        }
        Err(e) => eprintln!("Error loading PDF: {}", e),
    }
}

pub(crate) fn cmd_sandbox_pdf(input: String, output: String) {
    match std::fs::read(&input) {
        Ok(bytes) => match pdf::sandbox_pdf_bytes(&bytes) {
            Ok((output_bytes, report)) => match std::fs::write(&output, output_bytes) {
                Ok(_) => {
                    println!("Sandboxed PDF written to {}", output);
                    println!("  Actions found: {}", report.actions_found.len());
                    println!("  Actions removed: {}", report.actions_removed);
                    println!("  Clean: {}", report.clean);
                    for action in &report.actions_found {
                        let id = action
                            .object_id
                            .map(|n| n.to_string())
                            .unwrap_or_else(|| "?".to_string());
                        println!(
                            "    [{}] object {} — {}",
                            action.kind, id, action.description
                        );
                    }
                }
                Err(e) => eprintln!("Error writing sandboxed PDF: {}", e),
            },
            Err(e) => eprintln!("Error sandboxing PDF: {}", e),
        },
        Err(e) => eprintln!("Error reading PDF: {}", e),
    }
}
