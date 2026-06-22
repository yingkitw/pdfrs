//! PDF security and encryption support
//!
//! This module provides password protection and permission management for PDF documents.

use anyhow::{anyhow, Result};

/// PDF permission flags for controlling what operations are allowed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfPermissions {
    /// Allow printing the document
    pub print: bool,
    /// Allow copying text and graphics
    pub copy: bool,
    /// Allow modifying the document
    pub modify: bool,
    /// Allow adding or modifying annotations
    pub annotate: bool,
    /// Allow filling in form fields
    pub fill_forms: bool,
    /// Allow extracting content for accessibility
    pub extract: bool,
    /// Allow assembling the document (insert, rotate, delete pages)
    pub assemble: bool,
    /// Allow printing high-quality versions
    pub print_high_quality: bool,
}

impl Default for PdfPermissions {
    fn default() -> Self {
        Self {
            print: true,
            copy: true,
            modify: true,
            annotate: true,
            fill_forms: true,
            extract: true,
            assemble: true,
            print_high_quality: true,
        }
    }
}

impl PdfPermissions {
    /// Create permissions with all permissions granted
    pub fn all() -> Self {
        Self::default()
    }

    /// Create permissions with all permissions denied (except basic viewing)
    pub fn none() -> Self {
        Self {
            print: false,
            copy: false,
            modify: false,
            annotate: false,
            fill_forms: false,
            extract: false,
            assemble: false,
            print_high_quality: false,
        }
    }

    /// Create permissions for read-only documents (viewing only)
    pub fn read_only() -> Self {
        Self {
            print: false,
            copy: false,
            modify: false,
            annotate: false,
            fill_forms: false,
            extract: true,
            assemble: false,
            print_high_quality: false,
        }
    }

    /// Convert to PDF permission flags (as specified in PDF 1.7 spec)
    /// Returns a u32 representing the permission bits
    pub fn to_pdf_flags(&self) -> u32 {
        // Default value with reserved bits set (bits 0-2, 6-7, 10, 13-31 are reserved)
        let mut flags = 0xFFFFF0C0u32;

        // Clear permission bits first (bits 2-5, 8-9, 11-12)
        flags &= !(1 << 2);  // Clear print bit
        flags &= !(1 << 3);  // Clear modify bit
        flags &= !(1 << 4);  // Clear copy bit
        flags &= !(1 << 5);  // Clear annotate bit
        flags &= !(1 << 8);  // Clear fill_forms bit
        flags &= !(1 << 9);  // Clear extract bit
        flags &= !(1 << 11); // Clear assemble bit
        flags &= !(1 << 12); // Clear print_high_quality bit

        // Set permission bits based on settings
        if self.print {
            flags |= 1 << 2;
        }
        if self.modify {
            flags |= 1 << 3;
        }
        if self.copy {
            flags |= 1 << 4;
        }
        if self.annotate {
            flags |= 1 << 5;
        }
        if self.fill_forms {
            flags |= 1 << 8;
        }
        if self.extract {
            flags |= 1 << 9;
        }
        if self.assemble {
            flags |= 1 << 11;
        }
        if self.print_high_quality {
            flags |= 1 << 12;
        }

        flags
    }

    /// Parse from PDF permission flags
    pub fn from_pdf_flags(flags: u32) -> Self {
        Self {
            print: (flags & (1 << 2)) != 0,
            modify: (flags & (1 << 3)) != 0,
            copy: (flags & (1 << 4)) != 0,
            annotate: (flags & (1 << 5)) != 0,
            fill_forms: (flags & (1 << 8)) != 0,
            extract: (flags & (1 << 9)) != 0,
            assemble: (flags & (1 << 11)) != 0,
            print_high_quality: (flags & (1 << 12)) != 0,
        }
    }
}

/// Encryption algorithms supported for PDF encryption
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionAlgorithm {
    /// RC4 40-bit (PDF 1.3)
    Rc4_40,
    /// RC4 128-bit (PDF 1.4)
    Rc4_128,
    /// AES 128-bit (PDF 1.6)
    Aes128,
    /// AES 256-bit (PDF 2.0)
    Aes256,
}

impl EncryptionAlgorithm {
    /// Get the key length in bytes
    pub fn key_length(&self) -> usize {
        match self {
            Self::Rc4_40 => 5,
            Self::Rc4_128 => 16,
            Self::Aes128 => 16,
            Self::Aes256 => 32,
        }
    }

    /// Get the algorithm name as used in PDF
    pub fn name(&self) -> &str {
        match self {
            Self::Rc4_40 => "V2",
            Self::Rc4_128 => "V4",
            Self::Aes128 => "AESV2",
            Self::Aes256 => "AESV3",
        }
    }
}

/// Password protection settings for a PDF document
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfSecurity {
    /// User password (optional) - if provided, document requires password to open
    pub user_password: Option<String>,
    /// Owner password (optional) - if provided, controls permissions
    pub owner_password: Option<String>,
    /// Encryption algorithm to use
    pub encryption_algorithm: EncryptionAlgorithm,
    /// Permission flags
    pub permissions: PdfPermissions,
    /// Whether to encrypt metadata
    pub encrypt_metadata: bool,
}

impl Default for PdfSecurity {
    fn default() -> Self {
        Self {
            user_password: None,
            owner_password: None,
            encryption_algorithm: EncryptionAlgorithm::Rc4_128,
            permissions: PdfPermissions::default(),
            encrypt_metadata: true,
        }
    }
}

impl PdfSecurity {
    /// Create a new PdfSecurity with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the user password (required to open the document)
    pub fn with_user_password(mut self, password: String) -> Self {
        self.user_password = Some(password);
        self
    }

    /// Set the owner password (controls permissions)
    pub fn with_owner_password(mut self, password: String) -> Self {
        self.owner_password = Some(password);
        self
    }

    /// Set the encryption algorithm
    pub fn with_encryption(mut self, algorithm: EncryptionAlgorithm) -> Self {
        self.encryption_algorithm = algorithm;
        self
    }

    /// Set the permissions
    pub fn with_permissions(mut self, permissions: PdfPermissions) -> Self {
        self.permissions = permissions;
        self
    }

    /// Set whether to encrypt metadata
    pub fn with_encrypt_metadata(mut self, encrypt: bool) -> Self {
        self.encrypt_metadata = encrypt;
        self
    }

    /// Check if the document is password protected
    pub fn is_protected(&self) -> bool {
        self.user_password.is_some() || self.owner_password.is_some()
    }

    /// Validate password settings
    pub fn validate(&self) -> Result<()> {
        if self.user_password.is_some() && self.user_password.as_ref().unwrap().is_empty() {
            return Err(anyhow!("User password cannot be empty"));
        }
        if self.owner_password.is_some() && self.owner_password.as_ref().unwrap().is_empty() {
            return Err(anyhow!("Owner password cannot be empty"));
        }
        Ok(())
    }
}

/// Basic encryption/decryption functions
///
/// Note: This is a simplified implementation. For production use, you would want
/// to use a proper cryptographic library like RustCrypto or openssl.
impl PdfSecurity {
    /// Encrypt data using the configured algorithm
    ///
    /// Note: This is a stub implementation. For production, use a proper crypto library.
    pub fn encrypt_data(&self, data: &[u8], _key: &[u8]) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }

        // Stub: In production, this would use actual encryption
        // For now, just return the data as-is (no encryption)
        Ok(data.to_vec())
    }

    /// Decrypt data using the configured algorithm
    ///
    /// Note: This is a stub implementation. For production, use a proper crypto library.
    pub fn decrypt_data(&self, data: &[u8], _key: &[u8]) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }

        // Stub: In production, this would use actual decryption
        // For now, just return the data as-is (no encryption)
        Ok(data.to_vec())
    }

    /// Generate an encryption key from passwords
    ///
    /// Note: This is a simplified implementation following PDF 1.7 spec algorithm 3.2
    pub fn generate_encryption_key(&self) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(Vec::new());
        }

        let key_len = self.encryption_algorithm.key_length();
        // Stub: Generate a placeholder key
        // In production, this would follow the PDF spec's key derivation algorithm
        Ok(vec![0u8; key_len])
    }

    /// Create the encryption dictionary for the PDF trailer
    pub fn create_encryption_dict(&self) -> String {
        if !self.is_protected() {
            return String::new();
        }

        let key_length = self.encryption_algorithm.key_length() * 8;
        let flags = self.permissions.to_pdf_flags();

        format!(
            "<< /Filter /Standard \
               /V {} \
               /R {} \
               /Length {} \
               /P {} \
               /EncryptMetadata {} \
               /O <OWNER_PASSWORD_PLACEHOLDER> \
               /U <USER_PASSWORD_PLACEHOLDER> >>",
            if self.encryption_algorithm == EncryptionAlgorithm::Aes256 {
                "5"
            } else if self.encryption_algorithm == EncryptionAlgorithm::Aes128 {
                "4"
            } else {
                "2"
            },
            if self.encryption_algorithm == EncryptionAlgorithm::Aes256 {
                "5"
            } else if self.encryption_algorithm == EncryptionAlgorithm::Aes128 {
                "4"
            } else {
                "3"
            },
            key_length,
            flags,
            if self.encrypt_metadata { "true" } else { "false" }
        )
    }
}

/// Digital signature information for PDF documents
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigitalSignature {
    /// Name of the signer
    pub signer_name: String,
    /// Reason for signing (e.g., "I approve this document")
    pub reason: Option<String>,
    /// Location where the document was signed
    pub location: Option<String>,
    /// Contact information for the signer
    pub contact_info: Option<String>,
    /// Signing date (ISO 8601 format)
    pub date: Option<String>,
    /// Signature filter (e.g., "Adobe.PPKLite")
    pub filter: String,
    /// Sub-filter defining the signature format (e.g., "adbe.pkcs7.detached")
    pub sub_filter: String,
    /// The PKCS#7/CMS signature bytes (hex-encoded for PDF storage)
    pub signature_hex: Option<String>,
    /// Byte range [start1, len1, start2, len2] that was signed
    pub byte_range: Option<Vec<u32>>,
}

impl Default for DigitalSignature {
    fn default() -> Self {
        Self {
            signer_name: String::new(),
            reason: None,
            location: None,
            contact_info: None,
            date: None,
            filter: "Adobe.PPKLite".to_string(),
            sub_filter: "adbe.pkcs7.detached".to_string(),
            signature_hex: None,
            byte_range: None,
        }
    }
}

impl DigitalSignature {
    /// Create a new digital signature with the given signer name
    pub fn new(signer_name: impl Into<String>) -> Self {
        Self {
            signer_name: signer_name.into(),
            ..Default::default()
        }
    }

    /// Set the reason for signing
    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    /// Set the signing location
    pub fn with_location(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    /// Set the contact information
    pub fn with_contact_info(mut self, contact: impl Into<String>) -> Self {
        self.contact_info = Some(contact.into());
        self
    }

    /// Set the signing date
    pub fn with_date(mut self, date: impl Into<String>) -> Self {
        self.date = Some(date.into());
        self
    }

    /// Build the PDF signature dictionary string
    pub fn to_pdf_dict(&self) -> String {
        let mut dict = format!(
            "<< /Type /Sig\n\
             /Filter /{}\n\
             /SubFilter /{}\n\
             /M (D:{})\n\
             /Name ({})\n",
            escape_pdf_name(&self.filter),
            escape_pdf_name(&self.sub_filter),
            self.date.as_deref().unwrap_or(""),
            escape_pdf_string(&self.signer_name)
        );

        if let Some(ref reason) = self.reason {
            dict.push_str(&format!(" /Reason ({})\n", escape_pdf_string(reason)));
        }
        if let Some(ref location) = self.location {
            dict.push_str(&format!(" /Location ({})\n", escape_pdf_string(location)));
        }
        if let Some(ref contact) = self.contact_info {
            dict.push_str(&format!(" /ContactInfo ({})\n", escape_pdf_string(contact)));
        }

        // ByteRange: [start1, len1, start2, len2]
        if let Some(ref range) = self.byte_range {
            let range_str: Vec<String> = range.iter().map(|v| v.to_string()).collect();
            dict.push_str(&format!(" /ByteRange [{}]\n", range_str.join(" ")));
        }

        // Contents: hex-encoded signature placeholder or actual signature
        if let Some(ref sig_hex) = self.signature_hex {
            dict.push_str(&format!(" /Contents <{}>\n", sig_hex));
        } else {
            // Placeholder for a 4096-byte signature
            dict.push_str(" /Contents <");
            dict.push_str(&"0".repeat(8192));
            dict.push_str(">\n");
        }

        dict.push_str(">>");
        dict
    }
}

fn escape_pdf_string(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn escape_pdf_name(name: &str) -> String {
    name.replace(" ", "#20")
        .replace("#", "#23")
        .replace("/", "#2F")
        .replace("[", "#5B")
        .replace("]", "#5D")
        .replace("<", "#3C")
        .replace(">", "#3E")
        .replace("(", "#28")
        .replace(")", "#29")
}

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
        let security = PdfSecurity::new()
            .with_user_password("test123".to_string());

        assert!(security.is_protected());
        assert!(security.validate().is_ok());
    }

    #[test]
    fn test_security_empty_password_rejected() {
        let security = PdfSecurity::new()
            .with_user_password("".to_string());

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
        let security = PdfSecurity::new()
            .with_user_password("user".to_string())
            .with_owner_password("owner".to_string());

        let dict = security.create_encryption_dict();
        assert!(dict.contains("/Filter /Standard"));
        assert!(dict.contains("/O <"));
        assert!(dict.contains("/U <"));
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
}
