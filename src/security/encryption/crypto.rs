//! Cryptographic primitives: RC4, AES-CBC, and PDF key derivation.

use crate::error::{PdfError, Result};
use md5::Md5;
use sha2::{Digest, Sha256};

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

pub(crate) fn random_vec(n: usize) -> Result<Vec<u8>> {
    let mut v = vec![0u8; n];
    random_bytes(&mut v)?;
    Ok(v)
}

// --- Key derivation helpers ---

/// PDF 1.7 Algorithm 3.1: per-object key for RC4 (V1-V4).
pub(crate) fn derive_object_key_rc4(file_key: &[u8], obj_num: u32, gen_num: u16) -> Vec<u8> {
    let mut hasher = Md5::new();
    hasher.update(file_key);
    hasher.update(&obj_num.to_le_bytes()[..3]);
    hasher.update(&gen_num.to_le_bytes()[..2]);
    let hash = hasher.finalize();
    hash[..(file_key.len() + 5).min(16)].to_vec()
}

/// Per-object key for AES-128 (V4) — same as RC4 but with "sAlT" appended (PDF 1.7 §3.5.1).
pub(super) fn derive_object_key_aes(file_key: &[u8], obj_num: u32, gen_num: u16) -> Vec<u8> {
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
pub(super) fn derive_standard_key(
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
pub(super) fn compute_owner_entry(
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
pub(super) fn compute_user_entry(file_key: &[u8], id0: &[u8], r: u8) -> Result<Vec<u8>> {
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
pub(super) fn aes128_cbc_zero_pad(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>> {
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
pub(crate) fn hash_r6(password: &[u8], salt: &[u8], user_bytes: &[u8]) -> Result<Vec<u8>> {
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
pub(super) fn aes256_cbc_no_pad(key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>> {
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
pub(super) struct V5Entries {
    pub(super) u: Vec<u8>,
    pub(super) ue: Vec<u8>,
    pub(super) o: Vec<u8>,
    pub(super) oe: Vec<u8>,
}

/// Generate the revision-6 `/U`, `/UE`, `/O`, `/OE` values from a random
/// 32-byte file key (ISO 32000-2 Algorithms 2.A-2.F).
pub(super) fn compute_v5_entries(
    user_pw: &[u8],
    owner_pw_eff: &[u8],
    file_key: &[u8],
) -> Result<V5Entries> {
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
pub(crate) fn aes_cbc_encrypt(key: &[u8], plaintext: &[u8]) -> Result<Vec<u8>> {
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
pub(crate) fn aes_cbc_decrypt(key: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>> {
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
pub(crate) const PADDING: [u8; 32] = [
    0x28, 0xBF, 0x4E, 0x5E, 0x4E, 0x75, 0x8A, 0x41, 0x64, 0x00, 0x4E, 0x56, 0xFF, 0xFA, 0x01, 0x08,
    0x2E, 0x2E, 0x00, 0xB6, 0xD0, 0x68, 0x3E, 0x80, 0x2F, 0x0C, 0xA9, 0xFE, 0x64, 0x53, 0x69, 0x7A,
];

/// Pad a password to 32 bytes using the PDF standard padding string.
pub(crate) fn pad_password(pw: &str) -> Vec<u8> {
    let pw_bytes = pw.as_bytes();
    let mut padded = Vec::with_capacity(32);
    let take = pw_bytes.len().min(32);
    padded.extend_from_slice(&pw_bytes[..take]);
    let remaining = 32 - take;
    padded.extend_from_slice(&PADDING[..remaining]);
    padded
}
