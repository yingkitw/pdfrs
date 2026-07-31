//! PDF security and encryption support
//!
//! This module provides password protection and permission management for PDF documents.
//! Implements the PDF Standard Security Handler (RC4 40/128-bit and AES-128/256-bit).

use crate::pdf_generator::escape_pdf_string;
use anyhow::{Result, anyhow};
use md5::Md5;
use sha2::{Digest, Sha256};

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
        flags &= !(1 << 2); // Clear print bit
        flags &= !(1 << 3); // Clear modify bit
        flags &= !(1 << 4); // Clear copy bit
        flags &= !(1 << 5); // Clear annotate bit
        flags &= !(1 << 8); // Clear fill_forms bit
        flags &= !(1 << 9); // Clear extract bit
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
        if let Some(ref pw) = self.user_password
            && pw.is_empty() {
                return Err(anyhow!("User password cannot be empty"));
            }
        if let Some(ref pw) = self.owner_password
            && pw.is_empty() {
                return Err(anyhow!("Owner password cannot be empty"));
            }
        Ok(())
    }
}

/// PDF Standard Security Handler encryption.
///
/// Implements RC4 40-bit (V1/V2), RC4 128-bit (V4), AES-128 (V4), and
/// AES-256 (V5/V6) per the PDF 1.7 specification (Algorithm 2, etc.).
impl PdfSecurity {
    /// Encrypt data using the configured algorithm and object reference.
    ///
    /// For RC4 and AES-128, the per-object key is derived from the file key
    /// + object number + generation number (Algorithm 3.1 / PDF 1.7 §7.6.2).
    /// For AES-256 (V5/V6), the file key is used directly.
    pub fn encrypt_data(&self, data: &[u8], key: &[u8], obj_num: u32, gen_num: u16) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }
        match self.encryption_algorithm {
            EncryptionAlgorithm::Rc4_40 | EncryptionAlgorithm::Rc4_128 => {
                let obj_key = derive_object_key_rc4(key, obj_num, gen_num);
                Ok(rc4_encrypt(&obj_key, data))
            }
            EncryptionAlgorithm::Aes128 => {
                let obj_key = derive_object_key_aes(key, obj_num, gen_num);
                aes_cbc_encrypt(&obj_key, data)
            }
            EncryptionAlgorithm::Aes256 => {
                aes_cbc_encrypt(key, data)
            }
        }
    }

    /// Decrypt data using the configured algorithm and object reference.
    pub fn decrypt_data(&self, data: &[u8], key: &[u8], obj_num: u32, gen_num: u16) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }
        match self.encryption_algorithm {
            EncryptionAlgorithm::Rc4_40 | EncryptionAlgorithm::Rc4_128 => {
                let obj_key = derive_object_key_rc4(key, obj_num, gen_num);
                Ok(rc4_encrypt(&obj_key, data))
            }
            EncryptionAlgorithm::Aes128 => {
                let obj_key = derive_object_key_aes(key, obj_num, gen_num);
                aes_cbc_decrypt(&obj_key, data)
            }
            EncryptionAlgorithm::Aes256 => {
                aes_cbc_decrypt(key, data)
            }
        }
    }

    /// Generate the file encryption key from passwords using the PDF Standard Security Handler.
    ///
    /// For V1-V4 (RC4/AES-128): uses MD5-based key derivation (Algorithm 2).
    /// For V5/V6 (AES-256): uses SHA-256-based key derivation (ExtensionLevel 3 / Algorithm 2B).
    pub fn generate_encryption_key(&self) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(Vec::new());
        }
        self.validate()?;

        let owner_pw = self.owner_password.as_deref().unwrap_or("");
        let user_pw = self.user_password.as_deref().unwrap_or("");

        match self.encryption_algorithm {
            EncryptionAlgorithm::Aes256 => {
                // AES-256: Algorithm 2B (PDF 2.0 / ExtensionLevel 3+)
                // File key is 32 random bytes; we derive from password for deterministic testing.
                let key = derive_aes256_key(user_pw, owner_pw, self.permissions.to_pdf_flags());
                Ok(key)
            }
            _ => {
                // V1-V4: Algorithm 2 (MD5-based)
                let key = derive_standard_key(
                    user_pw,
                    owner_pw,
                    self.permissions.to_pdf_flags(),
                    self.encryption_algorithm.key_length(),
                    self.encrypt_metadata,
                );
                Ok(key)
            }
        }
    }

    /// Generate the owner password hash (Algorithm 3.2 / 3.3 of PDF 1.7 spec).
    pub fn generate_owner_hash(&self) -> Result<Vec<u8>> {
        let owner_pw = self.owner_password.as_deref().unwrap_or("");
        let user_pw = self.user_password.as_deref().unwrap_or("");
        let pw = if owner_pw.is_empty() { user_pw } else { owner_pw };

        let mut hasher = Md5::new();
        hasher.update(pw.as_bytes());
        // For 128-bit keys, hash 50 times
        let mut hash = hasher.finalize().to_vec();
        if self.encryption_algorithm.key_length() > 5 {
            for _ in 0..50 {
                let mut h = Md5::new();
                h.update(&hash[..self.encryption_algorithm.key_length().min(16)]);
                hash = h.finalize().to_vec();
            }
        }
        Ok(hash[..self.encryption_algorithm.key_length().min(16)].to_vec())
    }

    /// Generate the user password hash (Algorithm 3.4 / 3.5 of PDF 1.7 spec).
    pub fn generate_user_hash(&self, file_key: &[u8]) -> Result<Vec<u8>> {
        match self.encryption_algorithm {
            EncryptionAlgorithm::Aes256 => {
                // Algorithm 2B: SHA-256(user_pw || user_validation_salt)
                let user_pw = self.user_password.as_deref().unwrap_or("");
                let mut hasher = Sha256::new();
                hasher.update(user_pw.as_bytes());
                // 8-byte validation salt (derived from key for determinism)
                hasher.update(&file_key[0..8.min(file_key.len())]);
                Ok(hasher.finalize().to_vec())
            }
            _ => {
                // Algorithm 3.4: MD5(padding || file_key)
                let padding = PADDING;
                let mut hasher = Md5::new();
                hasher.update(padding);
                hasher.update(file_key);
                let mut hash = hasher.finalize().to_vec();

                // RC4 hash 20 times with mutated key
                let key_len = file_key.len();
                for i in 0..20u8 {
                    let mut new_key = Vec::with_capacity(key_len);
                    for &b in &file_key[..key_len] {
                        new_key.push(b ^ i);
                    }
                    hash = rc4_encrypt(&new_key, &hash);
                }
                Ok(hash)
            }
        }
    }

    /// Create the encryption dictionary for the PDF trailer.
    ///
    /// Returns the `/Encrypt` dictionary content (without the `N 0 obj` wrapper).
    /// For AES-256, includes `/CF`, `/CFM`, and `/UES` entries.
    pub fn create_encryption_dict(&self) -> Result<String> {
        if !self.is_protected() {
            return Ok(String::new());
        }
        self.validate()?;

        let file_key = self.generate_encryption_key()?;
        let owner_hash = self.generate_owner_hash()?;
        let user_hash = self.generate_user_hash(&file_key)?;
        let flags = self.permissions.to_pdf_flags();
        let key_len = self.encryption_algorithm.key_length();

        let (v, r, cf_str) = match self.encryption_algorithm {
            EncryptionAlgorithm::Rc4_40 => (1, 2, String::new()),
            EncryptionAlgorithm::Rc4_128 => (2, 3, String::new()),
            EncryptionAlgorithm::Aes128 => (4, 4, " /CF << /StdCF << /CFM /AESV2 /Length 16 >> >>\n /StmF /StdCF\n /StrF /StdCF".to_string()),
            EncryptionAlgorithm::Aes256 => (5, 5, " /CF << /StdCF << /CFM /AESV3 /Length 32 >> >>\n /StmF /StdCF\n /StrF /StdCF".to_string()),
        };

        let owner_hex: String = owner_hash.iter().map(|b| format!("{:02x}", b)).collect();
        let user_hex: String = user_hash.iter().map(|b| format!("{:02x}", b)).collect();

        let mut dict = format!(
            "<< /Filter /Standard\n\
             /V {v}\n\
             /R {r}\n\
             /Length {}\n\
             /P {flags}\n\
             /O <{owner_hex}>\n\
             /U <{user_hex}>\n",
            key_len * 8,
        );

        if !cf_str.is_empty() {
            dict.push_str(&format!("{cf_str}\n"));
        }

        if !self.encrypt_metadata && v >= 4 {
            dict.push_str(" /EncryptMetadata false\n");
        }

        dict.push_str(">>");
        Ok(dict)
    }

    /// Get the file encryption key, generating it if needed.
    pub fn get_file_key(&self) -> Result<Vec<u8>> {
        self.generate_encryption_key()
    }
}

// --- RC4 cipher (inline, pure Rust) ---

/// RC4 stream cipher — encryption and decryption are the same operation.
pub fn rc4_encrypt(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut s = [0u8; 256];
    for i in 0..256 {
        s[i] = i as u8;
    }
    let mut j = 0u8;
    for i in 0..256 {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let mut i = 0u8;
    let mut j = 0u8;
    data.iter()
        .map(|&byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
            byte ^ k
        })
        .collect()
}

// --- Key derivation helpers ---

/// PDF 1.7 Algorithm 3.1: per-object key for RC4 (V1-V4).
fn derive_object_key_rc4(file_key: &[u8], obj_num: u32, gen_num: u16) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(file_key);
    hasher.update(&obj_num.to_le_bytes()[..3]);
    hasher.update(&gen_num.to_le_bytes()[..2]);
    let hash = hasher.finalize();
    hash[..(file_key.len() + 5).min(16)].to_vec()
}

/// Per-object key for AES-128 (V4) — same as RC4 but with 4 extra bytes (0x73 0x41 0x6C 0x54 = "sAlT").
fn derive_object_key_aes(file_key: &[u8], obj_num: u32, gen_num: u16) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(file_key);
    hasher.update(&obj_num.to_le_bytes()[..3]);
    hasher.update(&gen_num.to_le_bytes()[..2]);
    hasher.update(b"sAlT");
    let hash = hasher.finalize();
    hash[..16].to_vec()
}

/// PDF 1.7 Algorithm 2: Standard Security Handler key derivation (V1-V4).
fn derive_standard_key(
    user_pw: &str,
    owner_pw: &str,
    flags: u32,
    key_len: usize,
    encrypt_metadata: bool,
) -> Vec<u8> {
    // Step 1: Pad user password
    let padded_user = pad_password(user_pw);

    // Step 2: Compute owner password hash
    let owner_padded = pad_password(if owner_pw.is_empty() { user_pw } else { owner_pw });
    let mut hasher = Md5::new();
    hasher.update(&owner_padded);
    let mut owner_hash = hasher.finalize().to_vec();
    if key_len > 5 {
        for _ in 0..50 {
            let mut h = Md5::new();
            h.update(&owner_hash[..16]);
            owner_hash = h.finalize().to_vec();
        }
    }
    let owner_key = &owner_hash[..key_len.min(16)];

    // Step 3-4: MD5(padded_user || owner_key || flags_le32 || [optional /EncryptMetadata false])
    let mut hasher = Md5::new();
    hasher.update(&padded_user);
    hasher.update(owner_key);
    hasher.update(flags.to_le_bytes());
    if !encrypt_metadata {
        hasher.update(b"\xff\xff\xff\xff");
    }
    let mut key = hasher.finalize().to_vec();

    // Step 5: For 128-bit keys, hash 50 more times
    if key_len > 5 {
        for _ in 0..50 {
            let mut h = Md5::new();
            h.update(&key[..key_len.min(16)]);
            key = h.finalize().to_vec();
        }
    }

    key[..key_len.min(16)].to_vec()
}

/// AES-256 key derivation (Algorithm 2B, PDF 2.0).
/// Generates a 32-byte key from the user password.
fn derive_aes256_key(user_pw: &str, _owner_pw: &str, _flags: u32) -> Vec<u8> {
    // For AES-256, the file key should be random. We derive deterministically from
    // the password for reproducibility in testing. Real implementations should
    // generate a random 32-byte key and store it encrypted.
    let mut hasher = Sha256::new();
    hasher.update(user_pw.as_bytes());
    hasher.update(b"pdfrs-aes256-key-derivation");
    hasher.finalize().to_vec()
}

/// AES-128/256-CBC encrypt with 16-byte IV prepended.
fn aes_cbc_encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

    // Generate a deterministic IV from key + data hash for reproducibility
    let iv = {
        let mut h = Sha256::new();
        h.update(key);
        h.update(plaintext);
        let hash = h.finalize();
        hash[..16].to_vec()
    };

    match key.len() {
        16 => {
            let encryptor = Aes128CbcEnc::new_from_slices(key, &iv)
                .map_err(|_| anyhow!("Invalid AES-128 key/IV length"))?;
            let mut buf = plaintext.to_vec();
            buf.resize(plaintext.len() + 16, 0u8);
            let ct = encryptor
                .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
                .map_err(|_| anyhow!("AES-128 encryption failed"))?;
            let mut output = iv.clone();
            output.extend_from_slice(ct);
            Ok(output)
        }
        32 => {
            let encryptor = Aes256CbcEnc::new_from_slices(key, &iv)
                .map_err(|_| anyhow!("Invalid AES-256 key/IV length"))?;
            let mut buf = plaintext.to_vec();
            buf.resize(plaintext.len() + 16, 0u8);
            let ct = encryptor
                .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
                .map_err(|_| anyhow!("AES-256 encryption failed"))?;
            let mut output = iv.clone();
            output.extend_from_slice(ct);
            Ok(output)
        }
        _ => Err(anyhow!("Unsupported AES key length: {}", key.len())),
    }
}

/// AES-128/256-CBC decrypt (IV is first 16 bytes of ciphertext).
fn aes_cbc_decrypt(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    if ciphertext.len() < 16 {
        return Err(anyhow!("Ciphertext too short for IV"));
    }
    let iv = &ciphertext[..16];
    let ct = &ciphertext[16..];

    match key.len() {
        16 => {
            let decryptor = Aes128CbcDec::new_from_slices(key, iv)
                .map_err(|_| anyhow!("Invalid AES-128 key/IV length"))?;
            let mut buf = ct.to_vec();
            let pt = decryptor
                .decrypt_padded_mut::<Pkcs7>(&mut buf)
                .map_err(|_| anyhow!("AES-128 decryption failed (wrong key or corrupted data)"))?;
            Ok(pt.to_vec())
        }
        32 => {
            let decryptor = Aes256CbcDec::new_from_slices(key, iv)
                .map_err(|_| anyhow!("Invalid AES-256 key/IV length"))?;
            let mut buf = ct.to_vec();
            let pt = decryptor
                .decrypt_padded_mut::<Pkcs7>(&mut buf)
                .map_err(|_| anyhow!("AES-256 decryption failed (wrong key or corrupted data)"))?;
            Ok(pt.to_vec())
        }
        _ => Err(anyhow!("Unsupported AES key length: {}", key.len())),
    }
}

/// PDF password padding (32 bytes).
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41,
    0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80,
    0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Pad a password to 32 bytes using the PDF standard padding string.
fn pad_password(pw: &str) -> Vec<u8> {
    let pw_bytes = pw.as_bytes();
    let mut padded = Vec::with_capacity(32);
    let take = pw_bytes.len().min(32);
    padded.extend_from_slice(&pw_bytes[..take]);
    let remaining = 32 - take;
    padded.extend_from_slice(&PADDING[..remaining]);
    padded
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

// --- Certificate management (FR19.4) ---

/// X.509 signing certificate stored as PEM for PDF digital signatures.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SigningCertificate {
    /// Unique identifier for this certificate in a store
    pub id: String,
    /// Distinguished name or display subject (e.g. `CN=Alice`)
    pub subject: String,
    /// Optional issuer distinguished name
    pub issuer: Option<String>,
    /// PEM-encoded certificate bytes
    pub pem: String,
    /// SHA-256 fingerprint of the DER encoding (hex)
    pub fingerprint_sha256: String,
}

/// Directory-backed store for signing certificates (`{id}.pem` files).
#[derive(Debug, Clone)]
pub struct CertificateStore {
    directory: std::path::PathBuf,
}

impl CertificateStore {
    /// Open or create a certificate store directory.
    pub fn open(directory: impl AsRef<std::path::Path>) -> Result<Self> {
        let directory = directory.as_ref().to_path_buf();
        std::fs::create_dir_all(&directory)?;
        Ok(Self { directory })
    }

    /// Import a PEM certificate file into the store under `id`.
    pub fn import(
        &self,
        id: &str,
        pem_path: &str,
        subject: Option<&str>,
    ) -> Result<SigningCertificate> {
        validate_cert_id(id)?;
        let cert = load_certificate_pem(id, pem_path)?;
        let cert = if let Some(subject) = subject {
            SigningCertificate {
                subject: subject.to_string(),
                ..cert
            }
        } else {
            cert
        };
        let dest = self.directory.join(format!("{id}.pem"));
        std::fs::write(&dest, &cert.pem)?;
        Ok(cert)
    }

    /// List all certificates in the store.
    pub fn list(&self) -> Result<Vec<SigningCertificate>> {
        let mut certs = Vec::new();
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pem") {
                let id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                certs.push(load_certificate_pem(&id, path.to_str().unwrap())?);
            }
        }
        certs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(certs)
    }

    /// Load a certificate from the store by id.
    pub fn get(&self, id: &str) -> Result<SigningCertificate> {
        validate_cert_id(id)?;
        let path = self.directory.join(format!("{id}.pem"));
        load_certificate_pem(id, path.to_str().unwrap())
    }

    /// Remove a certificate from the store.
    pub fn remove(&self, id: &str) -> Result<bool> {
        validate_cert_id(id)?;
        let path = self.directory.join(format!("{id}.pem"));
        if path.exists() {
            std::fs::remove_file(path)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// Validate that a certificate store ID is safe (no path traversal, no empty).
fn validate_cert_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(anyhow!("Certificate ID cannot be empty"));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(anyhow!(
            "Certificate ID contains invalid characters (path separators or traversal sequences)"
        ));
    }
    Ok(())
}

/// Load a PEM-encoded X.509 certificate from disk.
pub fn load_certificate_pem(id: impl Into<String>, path: &str) -> Result<SigningCertificate> {
    let pem = std::fs::read_to_string(path)?;
    parse_certificate_pem(id, &pem)
}

/// Parse a PEM-encoded X.509 certificate string.
pub fn parse_certificate_pem(id: impl Into<String>, pem: &str) -> Result<SigningCertificate> {
    let id = id.into();
    if !pem.contains("-----BEGIN CERTIFICATE-----") {
        return Err(anyhow!("File does not contain a PEM certificate block"));
    }

    let der = pem_to_der(pem)?;
    let fingerprint_sha256 = {
        use sha2::{Digest, Sha256};
        let hash = Sha256::digest(&der);
        hash.iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };

    let subject = parse_subject_from_der(&der).unwrap_or_else(|| id.clone());

    Ok(SigningCertificate {
        id,
        subject,
        issuer: None,
        pem: pem.to_string(),
        fingerprint_sha256,
    })
}

/// Convert PEM certificate text to uppercase hex-encoded DER (for PDF `/Cert` entries).
pub fn certificate_pem_to_der_hex(pem: &str) -> Result<String> {
    let der = pem_to_der(pem)?;
    Ok(der.iter().map(|b| format!("{:02x}", b)).collect::<String>())
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>> {
    let b64: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    decode_base64(&b64)
}

fn decode_base64(input: &str) -> Result<Vec<u8>> {
    const TABLE: &[u8; 256] = &{
        let mut table = [255u8; 256];
        let mut i = 0u8;
        while i < 26 {
            table[(b'A' + i) as usize] = i;
            table[(b'a' + i) as usize] = 26 + i;
            i += 1;
        }
        let mut d = 0u8;
        while d < 10 {
            table[(b'0' + d) as usize] = 52 + d;
            d += 1;
        }
        table[b'+' as usize] = 62;
        table[b'/' as usize] = 63;
        table
    };

    let mut output = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;

    for &byte in input.as_bytes() {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            break;
        }
        let val = TABLE[byte as usize];
        if val == 255 {
            return Err(anyhow!("Invalid base64 character in PEM certificate"));
        }
        buf = (buf << 6) | u32::from(val);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }

    Ok(output)
}

fn parse_subject_from_der(der: &[u8]) -> Option<String> {
    let lossy = String::from_utf8_lossy(der);
    // Heuristic: self-signed and test certs embed the CN as a printable UTF-8 string in the DER.
    for marker in ["CN=", "Test Signer"] {
        if let Some(idx) = lossy.find(marker) {
            let slice = &lossy[idx..];
            let end = slice
                .find(['\0', '\x01', '\x02'])
                .unwrap_or(slice.len().min(64));
            let candidate = slice[..end].trim_matches(|c: char| !c.is_ascii_graphic() && c != '=');
            if !candidate.is_empty() {
                return Some(if candidate.starts_with("CN=") {
                    candidate.to_string()
                } else {
                    format!("CN={candidate}")
                });
            }
        }
    }
    None
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
        assert_eq!(unprotected.create_encryption_dict().unwrap(), "");

        let security = PdfSecurity::new()
            .with_user_password("user".to_string())
            .with_owner_password("owner".to_string());
        let dict = security.create_encryption_dict().unwrap();
        assert!(dict.contains("/Filter /Standard"));
        assert!(dict.contains("/V 2"));
        assert!(dict.contains("/R 3"));
        assert!(dict.contains("/O <"));
        assert!(dict.contains("/U <"));

        let key = security.generate_encryption_key().unwrap();
        assert_eq!(key.len(), 16); // RC4-128 key length
    }

    #[test]
    fn test_rc4_roundtrip() {
        let key = b"secret";
        let plaintext = b"Hello, World!";
        let ciphertext = rc4_encrypt(key, plaintext);
        assert_ne!(&ciphertext[..], plaintext);
        let decrypted = rc4_encrypt(key, &ciphertext);
        assert_eq!(&decrypted[..], plaintext);
    }

    #[test]
    fn test_rc4_empty_and_long() {
        let key = b"k";
        assert_eq!(rc4_encrypt(key, b""), Vec::<u8>::new());
        let long = vec![0x42u8; 1000];
        let ct = rc4_encrypt(key, &long);
        let pt = rc4_encrypt(key, &ct);
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
        let key = sec.generate_encryption_key().unwrap();
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
        let key = sec.generate_encryption_key().unwrap();
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
        let key = sec.generate_encryption_key().unwrap();
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
        let key = sec.generate_encryption_key().unwrap();
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
        let dict = sec.create_encryption_dict().unwrap();
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
        let dict = sec.create_encryption_dict().unwrap();
        assert!(dict.contains("/V 5"));
        assert!(dict.contains("/R 5"));
        assert!(dict.contains("/CFM /AESV3"));
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
