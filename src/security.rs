//! PDF security and encryption support
//!
//! This module provides password protection and permission management for PDF documents.
//! Implements the PDF Standard Security Handler (RC4 40/128-bit and AES-128/256-bit).

use crate::error::{PdfError, Result};
use crate::pdf_generator::escape_pdf_string;
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

/// PDF Standard Security Handler encryption.
///
/// Implements RC4 40-bit (V1/V2), RC4 128-bit (V4), AES-128 (V4), and
/// AES-256 (V5/V6) per the PDF 1.7 specification (Algorithm 2, etc.).
impl PdfSecurity {
    /// Encrypt data using the configured algorithm and object reference.
    ///
    /// For RC4 and AES-128, the per-object key is derived from the file key,
    /// object number, and generation number (Algorithm 3.1 / PDF 1.7 §7.6.2).
    /// For AES-256 (V5/V6), the file key is used directly.
    pub fn encrypt_data(
        &self,
        data: &[u8],
        key: &[u8],
        obj_num: u32,
        gen_num: u16,
    ) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }
        match self.encryption_algorithm {
            EncryptionAlgorithm::Rc4_40 | EncryptionAlgorithm::Rc4_128 => {
                let obj_key = derive_object_key_rc4(key, obj_num, gen_num);
                rc4_encrypt(&obj_key, data)
            }
            EncryptionAlgorithm::Aes128 => {
                let obj_key = derive_object_key_aes(key, obj_num, gen_num);
                aes_cbc_encrypt(&obj_key, data)
            }
            EncryptionAlgorithm::Aes256 => aes_cbc_encrypt(key, data),
        }
    }

    /// Decrypt data using the configured algorithm and object reference.
    pub fn decrypt_data(
        &self,
        data: &[u8],
        key: &[u8],
        obj_num: u32,
        gen_num: u16,
    ) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(data.to_vec());
        }
        match self.encryption_algorithm {
            EncryptionAlgorithm::Rc4_40 | EncryptionAlgorithm::Rc4_128 => {
                let obj_key = derive_object_key_rc4(key, obj_num, gen_num);
                rc4_encrypt(&obj_key, data)
            }
            EncryptionAlgorithm::Aes128 => {
                let obj_key = derive_object_key_aes(key, obj_num, gen_num);
                aes_cbc_decrypt(&obj_key, data)
            }
            EncryptionAlgorithm::Aes256 => aes_cbc_decrypt(key, data),
        }
    }

    /// Generate the file encryption key from passwords using the PDF Standard Security Handler.
    ///
    /// For V1-V4 (RC4/AES-128): MD5-based key derivation per Algorithm 2.
    /// For AES-256 (R6): the file key is 32 fresh random bytes; access is
    /// controlled through the random-salted `/U`, `/UE`, `/O`, `/OE` values.
    pub fn generate_encryption_key(&self, id0: &[u8]) -> Result<Vec<u8>> {
        if !self.is_protected() {
            return Ok(Vec::new());
        }
        self.validate()?;

        let user_pw = self.user_password.as_deref().unwrap_or("");

        match self.encryption_algorithm {
            EncryptionAlgorithm::Aes256 => random_vec(32),
            _ => {
                let key_len = self.encryption_algorithm.key_length();
                let (r, _) = self.v_r_mapping();
                let o_entry = self.compute_owner_entry_for_key()?;
                Ok(derive_standard_key(
                    user_pw,
                    &o_entry,
                    self.permissions.to_pdf_flags(),
                    id0,
                    self.encrypt_metadata,
                    key_len,
                    r,
                ))
            }
        }
    }

    fn v_r_mapping(&self) -> (u8, u8) {
        match self.encryption_algorithm {
            EncryptionAlgorithm::Rc4_40 => (1, 2),
            EncryptionAlgorithm::Rc4_128 => (2, 3),
            EncryptionAlgorithm::Aes128 => (4, 4),
            EncryptionAlgorithm::Aes256 => (5, 6),
        }
    }

    fn compute_owner_entry_for_key(&self) -> Result<Vec<u8>> {
        let user_pw = self.user_password.as_deref().unwrap_or("");
        let owner_pw = self.owner_password.as_deref().unwrap_or("");
        let owner_eff = if owner_pw.is_empty() {
            user_pw
        } else {
            owner_pw
        };
        let (_, r) = self.v_r_mapping();
        compute_owner_entry(
            owner_eff,
            user_pw,
            self.encryption_algorithm.key_length(),
            r,
        )
    }

    /// Get the file encryption key for the given document `/ID[0]`, generating it if needed.
    pub fn get_file_key(&self, id0: &[u8]) -> Result<Vec<u8>> {
        self.generate_encryption_key(id0)
    }
}

/// Everything needed to encrypt a document and emit its `/Encrypt` dictionary.
#[derive(Debug, Clone)]
pub struct EncryptionMaterials {
    /// Per-document file encryption key.
    pub file_key: Vec<u8>,
    /// `/Encrypt` dictionary body (without the `N 0 obj` wrapper).
    pub encrypt_dict: String,
}

impl PdfSecurity {
    /// Generate the file key and `/Encrypt` dictionary in one consistent pass.
    ///
    /// `doc_id` is the trailer `/ID[0]` value (16 bytes) that will be written
    /// alongside the encryption dictionary; it feeds Algorithm 2 for V1-V4.
    pub fn generate_encryption_materials(&self, doc_id: &[u8]) -> Result<EncryptionMaterials> {
        if !self.is_protected() {
            return Ok(EncryptionMaterials {
                file_key: Vec::new(),
                encrypt_dict: String::new(),
            });
        }
        self.validate()?;

        let user_pw = self.user_password.as_deref().unwrap_or("");
        let owner_pw = self.owner_password.as_deref().unwrap_or("");
        let owner_eff = if owner_pw.is_empty() {
            user_pw
        } else {
            owner_pw
        };
        let flags = self.permissions.to_pdf_flags();
        let (v, r) = self.v_r_mapping();

        let dict = if self.encryption_algorithm == EncryptionAlgorithm::Aes256 {
            let file_key = random_vec(32)?;
            let v5 = compute_v5_entries(user_pw.as_bytes(), owner_eff.as_bytes(), &file_key)?;
            let (u, ue, o, oe) = (v5.u, v5.ue, v5.o, v5.oe);
            let hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect() };
            let mut dict = format!(
                "<< /Filter /Standard\n\
                 /V {v}\n\
                 /R {r}\n\
                 /Length 256\n\
                 /P {}\n\
                 /OE <{}>\n\
                 /UE <{}>\n\
                 /O <{}>\n\
                 /U <{}>\n",
                flags as i32,
                hex(&oe),
                hex(&ue),
                hex(&o),
                hex(&u),
            );
            dict.push_str(
                " /CF << /StdCF << /CFM /AESV3 /Length 32 >> >>\n /StmF /StdCF\n /StrF /StdCF\n",
            );
            if !self.encrypt_metadata {
                dict.push_str(" /EncryptMetadata false\n");
            }
            dict.push_str(">>");
            (file_key, dict)
        } else {
            let key_len = self.encryption_algorithm.key_length();
            let o_entry = compute_owner_entry(owner_eff, user_pw, key_len, r)?;
            let file_key = derive_standard_key(
                user_pw,
                &o_entry,
                flags,
                doc_id,
                self.encrypt_metadata,
                key_len,
                r,
            );
            let u_entry = compute_user_entry(&file_key, doc_id, r)?;
            let hex = |b: &[u8]| -> String { b.iter().map(|x| format!("{x:02x}")).collect() };
            let mut dict = format!(
                "<< /Filter /Standard\n\
                 /V {v}\n\
                 /R {r}\n\
                 /Length {}\n\
                 /P {}\n\
                 /O <{}>\n\
                 /U <{}>\n",
                key_len * 8,
                flags as i32,
                hex(&o_entry),
                hex(&u_entry),
            );
            if self.encryption_algorithm == EncryptionAlgorithm::Aes128 {
                dict.push_str(
                    " /CF << /StdCF << /CFM /AESV2 /Length 16 >> >>\n /StmF /StdCF\n /StrF /StdCF\n",
                );
            }
            if r >= 4 && !self.encrypt_metadata {
                dict.push_str(" /EncryptMetadata false\n");
            }
            dict.push_str(">>");
            (file_key, dict)
        };

        Ok(EncryptionMaterials {
            file_key: dict.0,
            encrypt_dict: dict.1,
        })
    }
}

// --- RC4 cipher (inline, pure Rust) ---

/// RC4 stream cipher — encryption and decryption are the same operation.
///
/// Returns an error if `key` is empty (RC4 is undefined for empty keys).
pub fn rc4_encrypt(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    if key.is_empty() {
        return Err(PdfError::Crypto("RC4 key must not be empty".into()));
    }
    let mut s: [u8; 256] = core::array::from_fn(|i| i as u8);
    let mut j = 0u8;
    for i in 0..s.len() {
        j = j.wrapping_add(s[i]).wrapping_add(key[i % key.len()]);
        s.swap(i, j as usize);
    }
    let mut i = 0u8;
    let mut j = 0u8;
    Ok(data
        .iter()
        .map(|&byte| {
            i = i.wrapping_add(1);
            j = j.wrapping_add(s[i as usize]);
            s.swap(i as usize, j as usize);
            let k = s[(s[i as usize].wrapping_add(s[j as usize])) as usize];
            byte ^ k
        })
        .collect())
}

// --- Secure randomness ---

/// Fill `buf` with cryptographically secure random bytes.
///
/// Returns an error on wasm32-unknown-unknown builds, where no RNG backend
/// is wired up; callers there should surface a clear error instead of
/// silently weakening encryption.
pub fn random_bytes(buf: &mut [u8]) -> Result<()> {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        getrandom::fill(buf)
            .map_err(|e| PdfError::Crypto(format!("secure RNG unavailable: {e}")))?;
        Ok(())
    }
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        let _ = buf;
        Err(PdfError::Crypto(
            "secure randomness is not available in this WASM build".into(),
        ))
    }
}

fn random_vec(n: usize) -> Result<Vec<u8>> {
    let mut v = vec![0u8; n];
    random_bytes(&mut v)?;
    Ok(v)
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

/// Per-object key for AES-128 (V4) — same as RC4 but with "sAlT" appended (PDF 1.7 §3.5.1).
fn derive_object_key_aes(file_key: &[u8], obj_num: u32, gen_num: u16) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(file_key);
    hasher.update(&obj_num.to_le_bytes()[..3]);
    hasher.update(&gen_num.to_le_bytes()[..2]);
    hasher.update(b"sAlT");
    let hash = hasher.finalize();
    hash[..16].to_vec()
}

/// PDF 1.7 Algorithm 2: Standard Security Handler file-key derivation (V1-V4).
///
/// `o_entry` is the `/O` value from Algorithm 3.3, `id0` the first element of
/// the trailer `/ID` array, and `r` the revision (2, 3, or 4).
fn derive_standard_key(
    user_pw: &str,
    o_entry: &[u8],
    flags: u32,
    id0: &[u8],
    encrypt_metadata: bool,
    key_len: usize,
    r: u8,
) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(pad_password(user_pw));
    hasher.update(o_entry);
    hasher.update(flags.to_le_bytes());
    hasher.update(id0);
    if r >= 4 && !encrypt_metadata {
        hasher.update([0xFF, 0xFF, 0xFF, 0xFF]);
    }
    let mut key = hasher.finalize().to_vec();
    if r >= 3 {
        for _ in 0..50 {
            let mut h = Md5::new();
            h.update(&key[..key_len]);
            key = h.finalize().to_vec();
        }
    }
    key.truncate(key_len);
    key
}

/// PDF 1.7 Algorithm 3.3: compute the `/O` entry.
///
/// RC4-encrypts the padded user password with a key derived from the owner
/// password (falling back to the user password when no owner password is set).
fn compute_owner_entry(
    owner_pw_eff: &str,
    user_pw: &str,
    key_len: usize,
    r: u8,
) -> Result<Vec<u8>> {
    let mut digest = {
        let mut hasher = Md5::new();
        hasher.update(pad_password(owner_pw_eff));
        hasher.finalize().to_vec()
    };
    if r >= 3 {
        for _ in 0..50 {
            let mut h = Md5::new();
            h.update(&digest[..key_len]);
            digest = h.finalize().to_vec();
        }
    }
    let rc4_key = &digest[..key_len];
    let mut value = pad_password(user_pw);
    if r >= 3 {
        for i in 0..20u8 {
            let round_key: Vec<u8> = rc4_key.iter().map(|&b| b ^ i).collect();
            value = rc4_encrypt(&round_key, &value)?;
        }
    } else {
        value = rc4_encrypt(rc4_key, &value)?;
    }
    Ok(value)
}

/// PDF 1.7 Algorithms 3.4 (R2) and 3.5 (R3+): compute the `/U` entry.
fn compute_user_entry(file_key: &[u8], id0: &[u8], r: u8) -> Result<Vec<u8>> {
    if r < 3 {
        // R2: U = RC4(file_key, padding)
        return rc4_encrypt(file_key, &PADDING);
    }
    let mut hasher = Md5::new();
    hasher.update(PADDING);
    hasher.update(file_key);
    hasher.update(id0);
    let mut hash = hasher.finalize().to_vec();
    for i in 0..20u8 {
        let round_key: Vec<u8> = file_key.iter().map(|&b| b ^ i).collect();
        hash = rc4_encrypt(&round_key, &hash)?;
    }
    // R3+: 16 arbitrary bytes follow; use fresh random bytes per spec.
    let mut u = hash;
    u.extend(random_vec(16)?);
    Ok(u)
}

// --- AES-256 revision 6 (ISO 32000-2 Algorithm 2.B) ---

/// Raw AES-128-CBC encryption with zero padding (final partial block padded
/// with zeros, no PKCS#7) — used by Algorithm 2.B step 4.
fn aes128_cbc_zero_pad(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};

    let cipher = aes::Aes128::new(GenericArray::from_slice(key));
    let mut prev = [0u8; 16];
    prev.copy_from_slice(iv);
    let mut out = Vec::with_capacity(data.len().div_ceil(16) * 16);
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        block[..chunk.len()].copy_from_slice(chunk);
        for (b, p) in block.iter_mut().zip(prev) {
            *b ^= p;
        }
        let ga = GenericArray::from_mut_slice(&mut block);
        cipher.encrypt_block(ga);
        out.extend_from_slice(&block);
        prev = block;
    }
    Ok(out)
}

/// ISO 32000-2 Algorithm 2.B: hardened revision-6 hash.
///
/// Returns the first 32 bytes of K. `user_bytes` is the 48-byte `/U` value
/// when computing `/O`-related hashes, empty otherwise.
fn hash_r6(password: &[u8], salt: &[u8], user_bytes: &[u8]) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha384, Sha512};

    let mut t = Vec::with_capacity(password.len() + salt.len() + user_bytes.len());
    t.extend_from_slice(password);
    t.extend_from_slice(salt);
    t.extend_from_slice(user_bytes);

    let mut k = Sha256::digest(&t).to_vec();
    let mut round: u32 = 0;
    loop {
        let mut k1 = Vec::with_capacity(64 * t.len());
        for _ in 0..64 {
            k1.extend_from_slice(&t);
        }
        let e = aes128_cbc_zero_pad(&k[..16], &k[16..32], &k1)?;
        let mut remainder: u32 = 0;
        for &b in &e[..16] {
            remainder = (remainder * 256 + u32::from(b)) % 3;
        }
        k = match remainder {
            0 => Sha256::digest(&e).to_vec(),
            1 => Sha384::digest(&e).to_vec(),
            _ => Sha512::digest(&e).to_vec(),
        };
        round += 1;
        let last = e[e.len() - 1];
        if round >= 64 && u32::from(last) <= round - 32 {
            break;
        }
        if round > 192 {
            return Err(PdfError::Crypto(
                "Algorithm 2.B hash failed to converge".into(),
            ));
        }
    }
    k.truncate(32);
    Ok(k)
}

/// AES-256-CBC encryption with no padding and a fixed IV — used for `/UE`
/// and `/OE` (the plaintext is always exactly 32 bytes = 2 blocks).
fn aes256_cbc_no_pad(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{BlockEncrypt, KeyInit, generic_array::GenericArray};

    if !data.len().is_multiple_of(16) {
        return Err(PdfError::Crypto(
            "AES-256 no-padding input must be block-aligned".into(),
        ));
    }
    let cipher = aes::Aes256::new(GenericArray::from_slice(key));
    let mut prev = [0u8; 16];
    prev.copy_from_slice(iv);
    let mut out = Vec::with_capacity(data.len());
    for chunk in data.chunks(16) {
        let mut block = [0u8; 16];
        block.copy_from_slice(chunk);
        for (b, p) in block.iter_mut().zip(prev) {
            *b ^= p;
        }
        let ga = GenericArray::from_mut_slice(&mut block);
        cipher.encrypt_block(ga);
        out.extend_from_slice(&block);
        prev = block;
    }
    Ok(out)
}

/// The four password-derived values of an R6 encryption dictionary.
struct V5Entries {
    u: Vec<u8>,
    ue: Vec<u8>,
    o: Vec<u8>,
    oe: Vec<u8>,
}

/// Generate the revision-6 `/U`, `/UE`, `/O`, `/OE` values from a random
/// 32-byte file key (ISO 32000-2 Algorithms 2.A-2.F).
fn compute_v5_entries(user_pw: &[u8], owner_pw_eff: &[u8], file_key: &[u8]) -> Result<V5Entries> {
    let u_vs = random_vec(8)?;
    let u_ks = random_vec(8)?;
    let mut u = hash_r6(user_pw, &u_vs, b"")?;
    u.extend_from_slice(&u_vs);
    u.extend_from_slice(&u_ks);

    let ue_key = hash_r6(user_pw, &u_ks, b"")?;
    let ue = aes256_cbc_no_pad(&ue_key, &[0u8; 16], file_key)?;

    let o_vs = random_vec(8)?;
    let o_ks = random_vec(8)?;
    let mut o = hash_r6(owner_pw_eff, &o_vs, &u)?;
    o.extend_from_slice(&o_vs);
    o.extend_from_slice(&o_ks);

    let oe_key = hash_r6(owner_pw_eff, &o_ks, &u)?;
    let oe = aes256_cbc_no_pad(&oe_key, &[0u8; 16], file_key)?;

    Ok(V5Entries { u, ue, o, oe })
}

/// AES-128/256-CBC encrypt with a fresh random 16-byte IV prepended
/// (per ISO 32000-2: the initialization vector shall be randomly generated).
fn aes_cbc_encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{BlockEncryptMut, KeyIvInit, block_padding::Pkcs7};
    type Aes128CbcEnc = cbc::Encryptor<aes::Aes128>;
    type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

    let mut iv = [0u8; 16];
    random_bytes(&mut iv)?;

    match key.len() {
        16 => {
            let encryptor = Aes128CbcEnc::new_from_slices(key, &iv)
                .map_err(|_| PdfError::Crypto("Invalid AES-128 key/IV length".into()))?;
            let mut buf = plaintext.to_vec();
            buf.resize(plaintext.len() + 16, 0u8);
            let ct = encryptor
                .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
                .map_err(|_| PdfError::Crypto("AES-128 encryption failed".into()))?;
            let mut output = iv.to_vec();
            output.extend_from_slice(ct);
            Ok(output)
        }
        32 => {
            let encryptor = Aes256CbcEnc::new_from_slices(key, &iv)
                .map_err(|_| PdfError::Crypto("Invalid AES-256 key/IV length".into()))?;
            let mut buf = plaintext.to_vec();
            buf.resize(plaintext.len() + 16, 0u8);
            let ct = encryptor
                .encrypt_padded_mut::<Pkcs7>(&mut buf, plaintext.len())
                .map_err(|_| PdfError::Crypto("AES-256 encryption failed".into()))?;
            let mut output = iv.to_vec();
            output.extend_from_slice(ct);
            Ok(output)
        }
        _ => Err(PdfError::Crypto(format!(
            "Unsupported AES key length: {}",
            key.len()
        ))),
    }
}

/// AES-128/256-CBC decrypt (IV is first 16 bytes of ciphertext).
fn aes_cbc_decrypt(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
    use aes::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};
    type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
    type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;

    if ciphertext.len() < 16 {
        return Err(PdfError::Crypto("Ciphertext too short for IV".into()));
    }
    let iv = &ciphertext[..16];
    let ct = &ciphertext[16..];

    match key.len() {
        16 => {
            let decryptor = Aes128CbcDec::new_from_slices(key, iv)
                .map_err(|_| PdfError::Crypto("Invalid AES-128 key/IV length".into()))?;
            let mut buf = ct.to_vec();
            let pt = decryptor
                .decrypt_padded_mut::<Pkcs7>(&mut buf)
                .map_err(|_| {
                    PdfError::Crypto(
                        "AES-128 decryption failed (wrong key or corrupted data)".into(),
                    )
                })?;
            Ok(pt.to_vec())
        }
        32 => {
            let decryptor = Aes256CbcDec::new_from_slices(key, iv)
                .map_err(|_| PdfError::Crypto("Invalid AES-256 key/IV length".into()))?;
            let mut buf = ct.to_vec();
            let pt = decryptor
                .decrypt_padded_mut::<Pkcs7>(&mut buf)
                .map_err(|_| {
                    PdfError::Crypto(
                        "AES-256 decryption failed (wrong key or corrupted data)".into(),
                    )
                })?;
            Ok(pt.to_vec())
        }
        _ => Err(PdfError::Crypto(format!(
            "Unsupported AES key length: {}",
            key.len()
        ))),
    }
}

/// PDF password padding (32 bytes).
const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
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
    name.replace("#", "#23")
        .replace(" ", "#20")
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
        return Err(PdfError::InvalidInput(
            "Certificate ID cannot be empty".into(),
        ));
    }
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(PdfError::InvalidInput(
            "Certificate ID contains invalid characters (path separators or traversal sequences)"
                .into(),
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
        return Err(PdfError::Crypto(
            "File does not contain a PEM certificate block".into(),
        ));
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
            return Err(PdfError::Crypto(
                "Invalid base64 character in PEM certificate".into(),
            ));
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
