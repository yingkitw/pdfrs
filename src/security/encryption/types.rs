//! Encryption algorithm selection and password-protection settings.

use super::permissions::PdfPermissions;
use crate::error::{PdfError, Result};

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
        if let Some(ref pw) = self.user_password
            && pw.is_empty()
        {
            return Err(PdfError::InvalidInput(
                "User password cannot be empty".into(),
            ));
        }
        if let Some(ref pw) = self.owner_password
            && pw.is_empty()
        {
            return Err(PdfError::InvalidInput(
                "Owner password cannot be empty".into(),
            ));
        }
        Ok(())
    }
}
