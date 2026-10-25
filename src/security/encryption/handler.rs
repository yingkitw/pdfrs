//! Standard security handler encrypt/decrypt and materials generation.

use super::crypto::{
    aes_cbc_decrypt, aes_cbc_encrypt, compute_owner_entry, compute_user_entry, compute_v5_entries,
    derive_object_key_aes, derive_object_key_rc4, derive_standard_key, random_vec, rc4_encrypt,
};
use super::types::{EncryptionAlgorithm, PdfSecurity};
use crate::error::Result;

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
