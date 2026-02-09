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
    Aes_128,
    /// AES 256-bit (PDF 2.0)
    Aes_256,
}

impl EncryptionAlgorithm {
    /// Get the key length in bytes
    pub fn key_length(&self) -> usize {
        match self {
            Self::Rc4_40 => 5,
            Self::Rc4_128 => 16,
            Self::Aes_128 => 16,
            Self::Aes_256 => 32,
        }
    }

    /// Get the algorithm name as used in PDF
    pub fn name(&self) -> &str {
        match self {
            Self::Rc4_40 => "V2",
            Self::Rc4_128 => "V4",
            Self::Aes_128 => "AESV2",
            Self::Aes_256 => "AESV3",
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

        let algorithm = self.encryption_algorithm.name();
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
            if self.encryption_algorithm == EncryptionAlgorithm::Aes_256 {
                "5"
            } else if self.encryption_algorithm == EncryptionAlgorithm::Aes_128 {
                "4"
            } else {
                "2"
            },
            if self.encryption_algorithm == EncryptionAlgorithm::Aes_256 {
                "5"
            } else if self.encryption_algorithm == EncryptionAlgorithm::Aes_128 {
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
        assert_eq!(EncryptionAlgorithm::Aes_128.key_length(), 16);
        assert_eq!(EncryptionAlgorithm::Aes_256.key_length(), 32);
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
}
