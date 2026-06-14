//! Low-level cryptographic primitives: Argon2id key derivation and
//! XChaCha20-Poly1305 authenticated encryption.
//!
//! This module knows nothing about the on-disk file layout. It deals only in
//! keys, salts, nonces, plaintext, ciphertext, and associated data.

use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    Key, XChaCha20Poly1305, XNonce,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::error::{Result, VaultError};

/// Length of a derived key, in bytes (XChaCha20-Poly1305 uses a 256-bit key).
pub const KEY_LEN: usize = 32;

/// Length of the AEAD authentication tag, in bytes.
pub const TAG_LEN: usize = 16;

/// Upper bound on Argon2 memory cost (1 GiB, expressed in KiB) accepted when
/// deriving a key.
///
/// KDF parameters are read from the vault header, which is plaintext and not
/// authenticated until *after* derivation. Without a ceiling, a crafted vault
/// file could set `m_cost` to its `u32` maximum (~4 TiB) and provoke a huge
/// allocation inside Argon2 the moment someone tries to unlock it — a
/// denial-of-service that aborts the process before the password is even
/// checked. 1 GiB is far above any legitimate setting yet bounds the damage.
pub const MAX_M_COST_KIB: u32 = 1 << 20;

/// Upper bound on Argon2 time cost (iterations) accepted when deriving a key.
pub const MAX_T_COST: u32 = 16;

/// Upper bound on Argon2 parallelism (lanes) accepted when deriving a key.
pub const MAX_P_COST: u32 = 16;

/// Parameters for Argon2id key derivation.
///
/// The defaults match the Spooky-Pass design document: 64 MiB of memory,
/// 3 iterations, parallelism 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    /// Memory cost, in KiB.
    pub m_cost_kib: u32,
    /// Time cost (number of iterations).
    pub t_cost: u32,
    /// Degree of parallelism (lanes).
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        KdfParams {
            m_cost_kib: 65_536, // 64 MiB
            t_cost: 3,
            p_cost: 1,
        }
    }
}

impl KdfParams {
    /// Reject implausibly expensive parameters before they reach Argon2.
    ///
    /// Because header-stored params are attacker-influenceable (they live in
    /// the not-yet-authenticated plaintext header), an unbounded `m_cost` would
    /// let a crafted vault provoke a multi-terabyte allocation on unlock. We cap
    /// all three costs well above any legitimate value; see [`MAX_M_COST_KIB`].
    fn validate(&self) -> Result<()> {
        if self.m_cost_kib > MAX_M_COST_KIB || self.t_cost > MAX_T_COST || self.p_cost > MAX_P_COST
        {
            return Err(VaultError::InvalidKdfParams);
        }
        Ok(())
    }
}

/// A 32-byte symmetric key derived from the master password.
///
/// The key is wiped from memory on drop ([`ZeroizeOnDrop`]). It intentionally
/// implements neither `Debug` nor `Clone` so it cannot be accidentally logged
/// or duplicated.
#[derive(ZeroizeOnDrop)]
pub struct DerivedKey([u8; KEY_LEN]);

impl DerivedKey {
    /// Borrow the raw key bytes (e.g. to feed the cipher).
    fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

/// Derive a 32-byte key from `password` and `salt` using Argon2id.
///
/// The crate takes the password by reference and makes no extra heap copy of
/// it; the caller owns the password string and is responsible for zeroizing it
/// if desired.
pub fn derive_key(password: &str, salt: &[u8], params: &KdfParams) -> Result<DerivedKey> {
    // Bound resource use before Argon2 allocates — params may come from an
    // untrusted vault header (see `KdfParams::validate`).
    params.validate()?;

    let argon2_params = Params::new(
        params.m_cost_kib,
        params.t_cost,
        params.p_cost,
        Some(KEY_LEN),
    )
    .map_err(|_| VaultError::InvalidKdfParams)?;

    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, argon2_params);

    // Derive into a transient buffer, then move it into the zeroizing newtype.
    let mut raw = [0u8; KEY_LEN];
    let derive_result = argon2.hash_password_into(password.as_bytes(), salt, &mut raw);

    match derive_result {
        Ok(()) => {
            let key = DerivedKey(raw);
            // Note: `raw` is `Copy`, so `key` holds an independent copy. Wipe
            // the transient stack buffer so the key only lives in `DerivedKey`.
            raw.zeroize();
            Ok(key)
        }
        Err(_) => {
            raw.zeroize();
            Err(VaultError::KeyDerivation)
        }
    }
}

/// Encrypt `plaintext` with `key` and `nonce`, authenticating `aad`.
///
/// Returns ciphertext with the 16-byte authentication tag appended.
pub fn seal(key: &DerivedKey, nonce: &[u8; 24], plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .encrypt(
            xnonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| VaultError::Crypto)
}

/// Decrypt and authenticate `ciphertext` (tag appended) with `key` and
/// `nonce`, checking `aad`.
///
/// Any authentication failure (wrong key, tampered ciphertext, or tampered
/// AAD) is reported as [`VaultError::WrongPassword`] so callers cannot
/// distinguish a wrong password from tampering.
pub fn open(key: &DerivedKey, nonce: &[u8; 24], ciphertext: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new(Key::from_slice(key.as_bytes()));
    let xnonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(
            xnonce,
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| VaultError::WrongPassword)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_nonce() -> [u8; 24] {
        [7u8; 24]
    }

    #[test]
    fn derive_key_is_deterministic() {
        let salt = [1u8; 16];
        let params = KdfParams::default();
        let k1 = derive_key("hunter2", &salt, &params).unwrap();
        let k2 = derive_key("hunter2", &salt, &params).unwrap();
        assert_eq!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn derive_key_differs_on_salt() {
        let params = KdfParams::default();
        let k1 = derive_key("hunter2", &[1u8; 16], &params).unwrap();
        let k2 = derive_key("hunter2", &[2u8; 16], &params).unwrap();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn derive_key_differs_on_password() {
        let salt = [1u8; 16];
        let params = KdfParams::default();
        let k1 = derive_key("hunter2", &salt, &params).unwrap();
        let k2 = derive_key("hunter3", &salt, &params).unwrap();
        assert_ne!(k1.as_bytes(), k2.as_bytes());
    }

    #[test]
    fn seal_then_open_roundtrips() {
        let key = derive_key("pw", &[3u8; 16], &KdfParams::default()).unwrap();
        let nonce = test_nonce();
        let aad = b"header";
        let ct = seal(&key, &nonce, b"secret message", aad).unwrap();
        let pt = open(&key, &nonce, &ct, aad).unwrap();
        assert_eq!(pt, b"secret message");
    }

    #[test]
    fn open_with_wrong_key_fails() {
        let key = derive_key("pw", &[3u8; 16], &KdfParams::default()).unwrap();
        let wrong = derive_key("other", &[3u8; 16], &KdfParams::default()).unwrap();
        let nonce = test_nonce();
        let ct = seal(&key, &nonce, b"secret", b"aad").unwrap();
        assert_matches::assert_matches!(
            open(&wrong, &nonce, &ct, b"aad"),
            Err(VaultError::WrongPassword)
        );
    }

    #[test]
    fn open_with_tampered_ciphertext_fails() {
        let key = derive_key("pw", &[3u8; 16], &KdfParams::default()).unwrap();
        let nonce = test_nonce();
        let mut ct = seal(&key, &nonce, b"secret", b"aad").unwrap();
        ct[0] ^= 0xff;
        assert_matches::assert_matches!(
            open(&key, &nonce, &ct, b"aad"),
            Err(VaultError::WrongPassword)
        );
    }

    #[test]
    fn open_with_tampered_aad_fails() {
        let key = derive_key("pw", &[3u8; 16], &KdfParams::default()).unwrap();
        let nonce = test_nonce();
        let ct = seal(&key, &nonce, b"secret", b"aad").unwrap();
        assert_matches::assert_matches!(
            open(&key, &nonce, &ct, b"different-aad"),
            Err(VaultError::WrongPassword)
        );
    }

    #[test]
    fn default_kdf_params_match_design() {
        let p = KdfParams::default();
        assert_eq!(p.m_cost_kib, 65_536);
        assert_eq!(p.t_cost, 3);
        assert_eq!(p.p_cost, 1);
    }

    #[test]
    fn derive_key_rejects_excessive_m_cost() {
        // A hostile header could claim ~4 TiB of memory; reject it *before*
        // Argon2 tries to allocate, rather than OOM/abort the process.
        let params = KdfParams {
            m_cost_kib: u32::MAX,
            t_cost: 3,
            p_cost: 1,
        };
        // `.map(|_| ())` drops the `DerivedKey` (which has no `Debug`) so
        // `assert_matches!` can render the result on failure.
        assert_matches::assert_matches!(
            derive_key("pw", &[0u8; 16], &params).map(|_| ()),
            Err(VaultError::InvalidKdfParams)
        );
    }

    #[test]
    fn derive_key_rejects_excessive_time_and_parallelism() {
        let over_time = KdfParams {
            t_cost: MAX_T_COST + 1,
            ..KdfParams::default()
        };
        assert_matches::assert_matches!(
            derive_key("pw", &[0u8; 16], &over_time).map(|_| ()),
            Err(VaultError::InvalidKdfParams)
        );

        let over_parallel = KdfParams {
            p_cost: MAX_P_COST + 1,
            ..KdfParams::default()
        };
        assert_matches::assert_matches!(
            derive_key("pw", &[0u8; 16], &over_parallel).map(|_| ()),
            Err(VaultError::InvalidKdfParams)
        );
    }

    #[test]
    fn derive_key_accepts_params_at_the_ceiling() {
        // At the bound is allowed. Use cheap memory so the high iteration/lane
        // counts don't make the test slow. Argon2 requires m_cost >= 8*p_cost,
        // so 256 KiB comfortably satisfies p_cost = 16.
        let params = KdfParams {
            m_cost_kib: 256,
            t_cost: MAX_T_COST,
            p_cost: MAX_P_COST,
        };
        assert!(derive_key("pw", &[0u8; 16], &params).is_ok());
    }
}
