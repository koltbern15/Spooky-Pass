//! On-disk vault file format (header encoding/decoding).
//!
//! This module is pure byte-twiddling: it knows the layout of the plaintext
//! header and the framing of the encrypted body, but performs no cryptography
//! and no I/O. All multi-byte integers are little-endian.
//!
//! ## Layout
//!
//! ```text
//! off  len  field            value
//!   0    4  magic            ASCII "SPK1"
//!   4    2  format_version   u16 = 1
//!   6    1  kdf_id           u8  = 1 (Argon2id)
//!   7    1  cipher_id        u8  = 1 (XChaCha20-Poly1305)
//!   8    4  argon2_m_cost    u32 (KiB)
//!  12    4  argon2_t_cost    u32
//!  16    4  argon2_p_cost    u32
//!  20    4  argon2_version   u32 = 0x13
//!  24    2  salt_len         u16 = 16
//!  26   16  salt             16 random bytes
//!  42    2  nonce_len        u16 = 24
//!  44   24  nonce            24 random bytes (fresh per save)
//!  --   --  (header ends at 68) -----------------------------------------
//!  68    8  ciphertext_len   u64
//!  76    N  ciphertext       AEAD output (plaintext + 16-byte tag)
//! ```
//!
//! The full 68-byte header is used as AEAD associated data, so every parameter
//! (KDF cost, salt, nonce) is authenticated along with the body.

use crate::error::{Result, VaultError};

/// Magic bytes at the start of every vault file: ASCII `"SPK1"`.
pub const MAGIC: [u8; 4] = *b"SPK1";

/// The format version this build reads and writes.
pub const FORMAT_VERSION: u16 = 1;

/// KDF identifier for Argon2id.
pub const KDF_ID_ARGON2ID: u8 = 1;

/// Cipher identifier for XChaCha20-Poly1305.
pub const CIPHER_ID_XCHACHA20POLY1305: u8 = 1;

/// Argon2 algorithm version recorded in the header (0x13 == v1.3, current).
pub const ARGON2_VERSION: u32 = 0x13;

/// Salt length, in bytes.
pub const SALT_LEN: usize = 16;

/// Nonce length, in bytes (XChaCha20-Poly1305 uses a 192-bit nonce).
pub const NONCE_LEN: usize = 24;

/// Fixed size of the plaintext header, in bytes.
pub const HEADER_LEN: usize = 68;

/// Offset of the `u64` ciphertext length field (immediately after the header).
const CIPHERTEXT_LEN_OFFSET: usize = HEADER_LEN; // 68
/// Offset of the ciphertext itself.
const CIPHERTEXT_OFFSET: usize = HEADER_LEN + 8; // 76

/// Parsed header fields recovered from a vault file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedHeader {
    pub format_version: u16,
    pub kdf_id: u8,
    pub cipher_id: u8,
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub argon2_version: u32,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
    /// The exact 68 header bytes, retained so they can be used as AEAD AAD.
    pub header_bytes: [u8; HEADER_LEN],
}

/// Header fields supplied by the caller when encoding a fresh vault file.
#[derive(Debug, Clone, Copy)]
pub struct HeaderFields {
    pub m_cost_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
}

/// Build the fixed 68-byte header from caller-supplied fields.
///
/// Exposed within the crate so the vault layer can compute the exact AAD bytes
/// it will encode, keeping the sealed header and the on-disk header identical.
pub(crate) fn build_header_bytes(fields: &HeaderFields) -> [u8; HEADER_LEN] {
    let mut h = [0u8; HEADER_LEN];
    h[0..4].copy_from_slice(&MAGIC);
    h[4..6].copy_from_slice(&FORMAT_VERSION.to_le_bytes());
    h[6] = KDF_ID_ARGON2ID;
    h[7] = CIPHER_ID_XCHACHA20POLY1305;
    h[8..12].copy_from_slice(&fields.m_cost_kib.to_le_bytes());
    h[12..16].copy_from_slice(&fields.t_cost.to_le_bytes());
    h[16..20].copy_from_slice(&fields.p_cost.to_le_bytes());
    h[20..24].copy_from_slice(&ARGON2_VERSION.to_le_bytes());
    h[24..26].copy_from_slice(&(SALT_LEN as u16).to_le_bytes());
    h[26..42].copy_from_slice(&fields.salt);
    h[42..44].copy_from_slice(&(NONCE_LEN as u16).to_le_bytes());
    h[44..68].copy_from_slice(&fields.nonce);
    h
}

/// Encode a complete vault file from header fields and the (already-sealed)
/// ciphertext.
pub fn encode(fields: &HeaderFields, ciphertext: &[u8]) -> Vec<u8> {
    let header = build_header_bytes(fields);
    let mut out = Vec::with_capacity(CIPHERTEXT_OFFSET + ciphertext.len());
    out.extend_from_slice(&header);
    out.extend_from_slice(&(ciphertext.len() as u64).to_le_bytes());
    out.extend_from_slice(ciphertext);
    out
}

/// Decode and validate a vault file, returning the parsed header and a slice
/// referencing the ciphertext bytes.
///
/// Rejects bad magic, unsupported versions/ids, wrong salt/nonce lengths, and
/// any size mismatch (truncation OR trailing garbage).
pub fn decode(bytes: &[u8]) -> Result<(ParsedHeader, &[u8])> {
    // Must at least contain the header + the ciphertext-length field.
    if bytes.len() < CIPHERTEXT_OFFSET {
        return Err(VaultError::MalformedFile);
    }

    // Magic first, so a non-vault file gets the clearest error.
    if bytes[0..4] != MAGIC {
        return Err(VaultError::BadMagic);
    }

    let format_version = u16::from_le_bytes([bytes[4], bytes[5]]);
    if format_version != FORMAT_VERSION {
        return Err(VaultError::UnsupportedVersion(format_version));
    }

    let kdf_id = bytes[6];
    if kdf_id != KDF_ID_ARGON2ID {
        return Err(VaultError::UnsupportedKdf(kdf_id));
    }

    let cipher_id = bytes[7];
    if cipher_id != CIPHER_ID_XCHACHA20POLY1305 {
        return Err(VaultError::UnsupportedCipher(cipher_id));
    }

    let m_cost_kib = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    let t_cost = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);
    let p_cost = u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let argon2_version = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);

    let salt_len = u16::from_le_bytes([bytes[24], bytes[25]]);
    if salt_len as usize != SALT_LEN {
        return Err(VaultError::MalformedFile);
    }
    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&bytes[26..42]);

    let nonce_len = u16::from_le_bytes([bytes[42], bytes[43]]);
    if nonce_len as usize != NONCE_LEN {
        return Err(VaultError::MalformedFile);
    }
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&bytes[44..68]);

    let ciphertext_len = u64::from_le_bytes([
        bytes[CIPHERTEXT_LEN_OFFSET],
        bytes[CIPHERTEXT_LEN_OFFSET + 1],
        bytes[CIPHERTEXT_LEN_OFFSET + 2],
        bytes[CIPHERTEXT_LEN_OFFSET + 3],
        bytes[CIPHERTEXT_LEN_OFFSET + 4],
        bytes[CIPHERTEXT_LEN_OFFSET + 5],
        bytes[CIPHERTEXT_LEN_OFFSET + 6],
        bytes[CIPHERTEXT_LEN_OFFSET + 7],
    ]);

    // Exact-size check: reject truncation AND trailing garbage.
    let expected_total = (CIPHERTEXT_OFFSET as u64)
        .checked_add(ciphertext_len)
        .ok_or(VaultError::MalformedFile)?;
    if bytes.len() as u64 != expected_total {
        return Err(VaultError::MalformedFile);
    }

    // Ciphertext must at least hold the AEAD tag.
    if (ciphertext_len as usize) < crate::crypto::TAG_LEN {
        return Err(VaultError::MalformedFile);
    }

    let mut header_bytes = [0u8; HEADER_LEN];
    header_bytes.copy_from_slice(&bytes[0..HEADER_LEN]);

    let header = ParsedHeader {
        format_version,
        kdf_id,
        cipher_id,
        m_cost_kib,
        t_cost,
        p_cost,
        argon2_version,
        salt,
        nonce,
        header_bytes,
    };

    Ok((header, &bytes[CIPHERTEXT_OFFSET..]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_fields() -> HeaderFields {
        HeaderFields {
            m_cost_kib: 65_536,
            t_cost: 3,
            p_cost: 1,
            salt: [0xAB; SALT_LEN],
            nonce: [0xCD; NONCE_LEN],
        }
    }

    // 16-byte tag minimum; use a plausible ciphertext.
    fn sample_ciphertext() -> Vec<u8> {
        (0u8..40).collect()
    }

    #[test]
    fn encode_decode_header_roundtrip() {
        let fields = sample_fields();
        let ct = sample_ciphertext();
        let bytes = encode(&fields, &ct);
        let (header, ct_out) = decode(&bytes).unwrap();
        assert_eq!(header.format_version, FORMAT_VERSION);
        assert_eq!(header.m_cost_kib, fields.m_cost_kib);
        assert_eq!(header.t_cost, fields.t_cost);
        assert_eq!(header.p_cost, fields.p_cost);
        assert_eq!(header.argon2_version, ARGON2_VERSION);
        assert_eq!(header.salt, fields.salt);
        assert_eq!(header.nonce, fields.nonce);
        assert_eq!(ct_out, &ct[..]);
    }

    #[test]
    fn header_has_correct_magic_and_version() {
        let bytes = encode(&sample_fields(), &sample_ciphertext());
        assert_eq!(&bytes[0..4], b"SPK1");
        assert_eq!(u16::from_le_bytes([bytes[4], bytes[5]]), 1);
        assert_eq!(bytes[6], KDF_ID_ARGON2ID);
        assert_eq!(bytes[7], CIPHER_ID_XCHACHA20POLY1305);
    }

    #[test]
    fn decode_rejects_bad_magic() {
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes[0] = b'X';
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::BadMagic));
    }

    #[test]
    fn decode_rejects_unknown_version() {
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes[4..6].copy_from_slice(&999u16.to_le_bytes());
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::UnsupportedVersion(999)));
    }

    #[test]
    fn decode_rejects_unknown_kdf_id() {
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes[6] = 9;
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::UnsupportedKdf(9)));
    }

    #[test]
    fn decode_rejects_unknown_cipher_id() {
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes[7] = 9;
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::UnsupportedCipher(9)));
    }

    #[test]
    fn decode_rejects_truncated_header() {
        let bytes = encode(&sample_fields(), &sample_ciphertext());
        // Cut off mid-header.
        assert_matches::assert_matches!(decode(&bytes[..40]), Err(VaultError::MalformedFile));
    }

    #[test]
    fn decode_rejects_truncated_body() {
        let bytes = encode(&sample_fields(), &sample_ciphertext());
        // Drop the last few ciphertext bytes; declared len no longer matches.
        let short = &bytes[..bytes.len() - 5];
        assert_matches::assert_matches!(decode(short), Err(VaultError::MalformedFile));
    }

    #[test]
    fn decode_rejects_trailing_garbage() {
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes.extend_from_slice(&[0xFF, 0xFF, 0xFF]);
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::MalformedFile));
    }

    #[test]
    fn decode_rejects_empty_input() {
        assert_matches::assert_matches!(decode(&[]), Err(VaultError::MalformedFile));
    }

    #[test]
    fn decode_rejects_wrong_salt_or_nonce_len() {
        // Wrong salt_len.
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes[24..26].copy_from_slice(&8u16.to_le_bytes());
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::MalformedFile));

        // Wrong nonce_len.
        let mut bytes = encode(&sample_fields(), &sample_ciphertext());
        bytes[42..44].copy_from_slice(&12u16.to_le_bytes());
        assert_matches::assert_matches!(decode(&bytes), Err(VaultError::MalformedFile));
    }

    #[test]
    fn integers_are_little_endian() {
        let fields = HeaderFields {
            m_cost_kib: 0x0102_0304,
            t_cost: 3,
            p_cost: 1,
            salt: [0; SALT_LEN],
            nonce: [0; NONCE_LEN],
        };
        let bytes = encode(&fields, &sample_ciphertext());
        // m_cost at offset 8, little-endian => 0x04 0x03 0x02 0x01.
        assert_eq!(&bytes[8..12], &[0x04, 0x03, 0x02, 0x01]);
    }
}
