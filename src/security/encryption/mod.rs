//! PDF security: standard security handler encryption (RC4/AES), key derivation.

mod crypto;
mod handler;
mod permissions;
mod types;

pub use handler::EncryptionMaterials;
pub use permissions::PdfPermissions;
pub use types::{EncryptionAlgorithm, PdfSecurity};

pub use crypto::{random_bytes, rc4_encrypt};

#[cfg(test)]
pub(crate) use crypto::{
    PADDING, aes_cbc_decrypt, aes_cbc_encrypt, derive_object_key_rc4, hash_r6, pad_password,
    random_vec,
};
