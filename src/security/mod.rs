//! PDF security and encryption support
//!
//! This module provides password protection and permission management for PDF documents.
//! Implements the PDF Standard Security Handler (RC4 40/128-bit and AES-128/256-bit).

mod certificates;
mod encryption;
mod signature;
#[cfg(feature = "sign")]
mod trust;

pub use certificates::*;
pub use encryption::*;
pub use signature::*;
#[cfg(feature = "sign")]
pub use trust::{TrustEvaluation, TrustStore, TrustedCertificate, evaluate_chain};

#[cfg(test)]
pub(crate) use encryption::{
    PADDING, aes_cbc_decrypt, aes_cbc_encrypt, derive_object_key_rc4, hash_r6, pad_password,
    random_vec,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permissions_default() {
        let perms = PdfPermissions::default();
        assert!(perms.print);
        assert!(perms.copy);
        assert!(perms.modify);
    }

    #[test]
    fn test_permissions_none() {
        let perms = PdfPermissions::none();
        assert!(!perms.print);
        assert!(!perms.copy);
        assert!(!perms.modify);
    }

    #[test]
    fn test_permissions_read_only() {
        let perms = PdfPermissions::read_only();
        assert!(!perms.print);
        assert!(!perms.copy);
        assert!(perms.extract);
    }

    #[test]
    fn test_permissions_flags_roundtrip() {
        let perms = PdfPermissions {
            print: true,
            copy: false,
            modify: true,
            annotate: false,
            fill_forms: true,
            extract: false,
            assemble: true,
            print_high_quality: false,
        };

        let flags = perms.to_pdf_flags();
        let restored = PdfPermissions::from_pdf_flags(flags);

        assert_eq!(restored.print, perms.print);
        assert_eq!(restored.copy, perms.copy);
        assert_eq!(restored.modify, perms.modify);
        assert_eq!(restored.annotate, perms.annotate);
        assert_eq!(restored.fill_forms, perms.fill_forms);
        assert_eq!(restored.extract, perms.extract);
        assert_eq!(restored.assemble, perms.assemble);
        assert_eq!(restored.print_high_quality, perms.print_high_quality);
    }

    #[test]
    fn test_encryption_algorithm_key_length() {
        assert_eq!(EncryptionAlgorithm::Rc4_40.key_length(), 5);
        assert_eq!(EncryptionAlgorithm::Rc4_128.key_length(), 16);
        assert_eq!(EncryptionAlgorithm::Aes128.key_length(), 16);
        assert_eq!(EncryptionAlgorithm::Aes256.key_length(), 32);
    }

    #[test]
    fn test_security_default() {
        let security = PdfSecurity::new();
        assert!(!security.is_protected());
        assert!(security.validate().is_ok());
    }

    #[test]
    fn test_security_with_user_password() {
        let security = PdfSecurity::new().with_user_password("test123".to_string());

        assert!(security.is_protected());
        assert!(security.validate().is_ok());
    }

    #[test]
    fn test_security_empty_password_rejected() {
        let security = PdfSecurity::new().with_user_password("".to_string());

        assert!(security.validate().is_err());
    }

    #[test]
    fn test_security_read_only() {
        let perms = PdfPermissions::read_only();
        let security = PdfSecurity::new()
            .with_user_password("secret".to_string())
            .with_permissions(perms);

        assert!(security.is_protected());
        assert!(!security.permissions.copy);
        assert!(!security.permissions.modify);
    }

    #[test]
    fn test_create_encryption_dict() {
        let unprotected = PdfSecurity::new();
        assert_eq!(
            unprotected
                .generate_encryption_materials(&[7u8; 16])
                .unwrap()
                .encrypt_dict,
            ""
        );

        let security = PdfSecurity::new()
            .with_user_password("user".to_string())
            .with_owner_password("owner".to_string());
        let materials = security.generate_encryption_materials(&[7u8; 16]).unwrap();
        let dict = materials.encrypt_dict;
        assert!(dict.contains("/Filter /Standard"));
        assert!(dict.contains("/V 2"));
        assert!(dict.contains("/R 3"));
        assert!(dict.contains("/O <"));
        assert!(dict.contains("/U <"));
        assert_eq!(materials.file_key.len(), 16); // RC4-128 key length
    }

    #[test]
    fn test_rc4_empty_key_rejected() {
        assert!(rc4_encrypt(b"", b"data").is_err());
    }

    #[test]
    fn test_random_bytes_distinct() {
        let a = random_vec(32).unwrap();
        let b = random_vec(32).unwrap();
        assert_ne!(a, b);
        assert!(a.iter().any(|&x| x != 0));
    }

    #[test]
    fn test_encryption_non_deterministic_across_runs() {
        let sec = || {
            PdfSecurity::new()
                .with_user_password("pw".to_string())
                .with_encryption(EncryptionAlgorithm::Aes256)
        };
        let m1 = sec().generate_encryption_materials(&[1u8; 16]).unwrap();
        let m2 = sec().generate_encryption_materials(&[1u8; 16]).unwrap();
        assert_ne!(m1.file_key, m2.file_key, "file key must be random");
        assert_ne!(m1.encrypt_dict, m2.encrypt_dict, "salts must be random");
    }

    #[test]
    fn test_aes_random_iv() {
        let key = [0x11u8; 16];
        let pt = b"same plaintext twice";
        let c1 = aes_cbc_encrypt(&key, pt).unwrap();
        let c2 = aes_cbc_encrypt(&key, pt).unwrap();
        assert_ne!(
            c1, c2,
            "identical plaintext must produce different ciphertext"
        );
    }

    #[test]
    fn test_rc4_roundtrip() {
        let key = b"secret";
        let plaintext = b"Hello, World!";
        let ciphertext = rc4_encrypt(key, plaintext).unwrap();
        assert_ne!(&ciphertext[..], plaintext);
        let decrypted = rc4_encrypt(key, &ciphertext).unwrap();
        assert_eq!(&decrypted[..], plaintext);
    }

    #[test]
    fn test_rc4_empty_and_long() {
        let key = b"k";
        assert_eq!(rc4_encrypt(key, b"").unwrap(), Vec::<u8>::new());
        let long = vec![0x42u8; 1000];
        let ct = rc4_encrypt(key, &long).unwrap();
        let pt = rc4_encrypt(key, &ct).unwrap();
        assert_eq!(pt, long);
    }

    #[test]
    fn test_aes128_roundtrip() {
        let key = [0x42u8; 16];
        let plaintext = b"Sensitive PDF content";
        let ct = aes_cbc_encrypt(&key, plaintext).unwrap();
        assert_ne!(&ct[..], plaintext);
        let pt = aes_cbc_decrypt(&key, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_aes256_roundtrip() {
        let key = [0xABu8; 32];
        let plaintext = b"Top secret document content";
        let ct = aes_cbc_encrypt(&key, plaintext).unwrap();
        let pt = aes_cbc_decrypt(&key, &ct).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_aes_wrong_key_fails() {
        let key1 = [0x01u8; 16];
        let key2 = [0x02u8; 16];
        let ct = aes_cbc_encrypt(&key1, b"secret").unwrap();
        assert!(aes_cbc_decrypt(&key2, &ct).is_err());
    }

    #[test]
    fn test_encrypt_data_rc4_40() {
        let sec = PdfSecurity::new()
            .with_user_password("pass".to_string())
            .with_encryption(EncryptionAlgorithm::Rc4_40);
        let key = sec.generate_encryption_key(&[3u8; 16]).unwrap();
        assert_eq!(key.len(), 5);
        let plaintext = b"stream content";
        let ct = sec.encrypt_data(plaintext, &key, 1, 0).unwrap();
        assert_ne!(&ct[..], plaintext);
        let pt = sec.decrypt_data(&ct, &key, 1, 0).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_encrypt_data_rc4_128() {
        let sec = PdfSecurity::new()
            .with_user_password("pass".to_string())
            .with_owner_password("owner".to_string())
            .with_encryption(EncryptionAlgorithm::Rc4_128);
        let key = sec.generate_encryption_key(&[3u8; 16]).unwrap();
        assert_eq!(key.len(), 16);
        let plaintext = b"stream content here";
        let ct = sec.encrypt_data(plaintext, &key, 5, 0).unwrap();
        assert_ne!(&ct[..], plaintext);
        let pt = sec.decrypt_data(&ct, &key, 5, 0).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_encrypt_data_aes128() {
        let sec = PdfSecurity::new()
            .with_user_password("pass".to_string())
            .with_encryption(EncryptionAlgorithm::Aes128);
        let key = sec.generate_encryption_key(&[3u8; 16]).unwrap();
        assert_eq!(key.len(), 16);
        let plaintext = b"AES encrypted stream";
        let ct = sec.encrypt_data(plaintext, &key, 3, 0).unwrap();
        assert_ne!(&ct[..], plaintext);
        let pt = sec.decrypt_data(&ct, &key, 3, 0).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_encrypt_data_aes256() {
        let sec = PdfSecurity::new()
            .with_user_password("pass".to_string())
            .with_encryption(EncryptionAlgorithm::Aes256);
        let key = sec.generate_encryption_key(&[3u8; 16]).unwrap();
        assert_eq!(key.len(), 32);
        let plaintext = b"AES-256 encrypted stream";
        let ct = sec.encrypt_data(plaintext, &key, 7, 0).unwrap();
        assert_ne!(&ct[..], plaintext);
        let pt = sec.decrypt_data(&ct, &key, 7, 0).unwrap();
        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_encryption_dict_aes128() {
        let sec = PdfSecurity::new()
            .with_user_password("pass".to_string())
            .with_encryption(EncryptionAlgorithm::Aes128);
        let dict = sec
            .generate_encryption_materials(&[3u8; 16])
            .unwrap()
            .encrypt_dict;
        assert!(dict.contains("/V 4"));
        assert!(dict.contains("/R 4"));
        assert!(dict.contains("/CFM /AESV2"));
        assert!(dict.contains("/StmF /StdCF"));
    }

    #[test]
    fn test_encryption_dict_aes256() {
        let sec = PdfSecurity::new()
            .with_user_password("pass".to_string())
            .with_encryption(EncryptionAlgorithm::Aes256);
        let m = sec.generate_encryption_materials(&[3u8; 16]).unwrap();
        let dict = m.encrypt_dict;
        assert!(dict.contains("/V 5"));
        assert!(dict.contains("/R 6"));
        assert!(dict.contains("/CFM /AESV3"));
        // R6 entries: U/O are 48 bytes, UE/OE are 32 bytes.
        let hex_len = |key: &str| {
            dict.split_once(&format!("/{key} <"))
                .and_then(|(_, rest)| rest.split_once('>').map(|(v, _)| v.len()))
                .unwrap()
        };
        assert_eq!(hex_len("U"), 96);
        assert_eq!(hex_len("O"), 96);
        assert_eq!(hex_len("UE"), 64);
        assert_eq!(hex_len("OE"), 64);
    }

    #[test]
    fn test_hash_r6_self_consistency() {
        // Salt changes must change the hash; user_bytes participate for O.
        let h1 = hash_r6(b"pw", b"12345678", b"").unwrap();
        let h2 = hash_r6(b"pw", b"87654321", b"").unwrap();
        assert_ne!(h1, h2);
        let h3 = hash_r6(b"pw", b"12345678", &[7u8; 48]).unwrap();
        assert_ne!(h1, h3);
        assert_eq!(h1.len(), 32);
    }

    #[test]
    fn test_pad_password() {
        let padded = pad_password("abc");
        assert_eq!(padded.len(), 32);
        assert_eq!(&padded[..3], b"abc");
        assert_eq!(padded[3], 0x28); // First padding byte

        let padded_empty = pad_password("");
        assert_eq!(padded_empty.len(), 32);
        assert_eq!(&padded_empty[..], &PADDING[..]);
    }

    #[test]
    fn test_per_object_key_differs() {
        let file_key = [0x01u8; 16];
        let k1 = derive_object_key_rc4(&file_key, 1, 0);
        let k2 = derive_object_key_rc4(&file_key, 2, 0);
        assert_ne!(k1, k2);
    }

    #[test]
    fn test_digital_signature_defaults() {
        let sig = DigitalSignature::new("Alice");
        assert_eq!(sig.signer_name, "Alice");
        assert_eq!(sig.filter, "Adobe.PPKLite");
        assert_eq!(sig.sub_filter, "adbe.pkcs7.detached");
    }

    #[test]
    fn test_digital_signature_builder() {
        let sig = DigitalSignature::new("Bob")
            .with_reason("I approve")
            .with_location("NYC")
            .with_contact_info("bob@example.com")
            .with_date("20240101");

        assert_eq!(sig.signer_name, "Bob");
        assert_eq!(sig.reason, Some("I approve".to_string()));
        assert_eq!(sig.location, Some("NYC".to_string()));
        assert_eq!(sig.contact_info, Some("bob@example.com".to_string()));
        assert_eq!(sig.date, Some("20240101".to_string()));
    }

    #[test]
    fn test_digital_signature_to_pdf_dict() {
        let sig = DigitalSignature::new("Charlie")
            .with_reason("Test reason")
            .with_location("Test location");

        let dict = sig.to_pdf_dict();
        assert!(dict.contains("/Type /Sig"));
        assert!(dict.contains("/Filter /Adobe.PPKLite"));
        assert!(dict.contains("/SubFilter /adbe.pkcs7.detached"));
        assert!(dict.contains("/Name (Charlie)"));
        assert!(dict.contains("/Reason (Test reason)"));
        assert!(dict.contains("/Location (Test location)"));
    }

    #[test]
    fn test_load_certificate_pem_fixture() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_cert.pem");
        let cert = load_certificate_pem("test", path).unwrap();
        assert_eq!(cert.id, "test");
        assert!(cert.subject.contains("Test Signer"));
        assert_eq!(cert.fingerprint_sha256.len(), 64);
    }

    #[test]
    fn test_certificate_store_import_list() {
        let dir = std::env::temp_dir().join(format!("pdfrs-certs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = CertificateStore::open(&dir).unwrap();
        let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_cert.pem");
        let cert = store.import("signer1", fixture, None).unwrap();
        assert_eq!(cert.id, "signer1");

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "signer1");

        let loaded = store.get("signer1").unwrap();
        assert_eq!(loaded.fingerprint_sha256, cert.fingerprint_sha256);

        assert!(store.remove("signer1").unwrap());
        assert!(store.list().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_certificate_pem_to_der_hex_roundtrip() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_cert.pem");
        let pem = std::fs::read_to_string(path).unwrap();
        let hex = certificate_pem_to_der_hex(&pem).unwrap();
        assert!(hex.len() > 100);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_cert_id_path_traversal_rejected() {
        let dir = std::env::temp_dir().join(format!("pdfrs-trav-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = CertificateStore::open(&dir).unwrap();
        let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/test_cert.pem");

        assert!(store.import("../escape", fixture, None).is_err());
        assert!(store.import("../../etc/evil", fixture, None).is_err());
        assert!(store.import("foo/bar", fixture, None).is_err());
        assert!(store.import("foo\\bar", fixture, None).is_err());
        assert!(store.import("", fixture, None).is_err());

        assert!(store.get("../escape").is_err());
        assert!(store.remove("../escape").is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
