//! HKDF-based key derivation with domain separation.
//!
//! Provides strongly-typed key derivation to prevent cross-protocol key reuse.
//! Uses HKDF-SHA256 from the `hkdf` crate.

use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroize;

use omnimesh_core::error::{OmniMeshError, Result};

/// Domain separation labels for different key derivation contexts.
///
/// Using distinct labels ensures that keys derived for one purpose
/// cannot be used for another, even if the input keying material is the same.
pub mod domain {
    /// Key derivation for transport encryption keys.
    pub const TRANSPORT: &[u8] = b"omnimesh-transport-v1";
    /// Key derivation for session ratcheting.
    pub const SESSION_RATCHET: &[u8] = b"omnimesh-session-ratchet-v1";
    /// Key derivation for key storage encryption.
    pub const KEYSTORE: &[u8] = b"omnimesh-keystore-v1";
    /// Key derivation for Noise handshake prologue.
    pub const NOISE_PROLOGUE: &[u8] = b"omnimesh-noise-prologue-v1";
    /// Key derivation for message authentication.
    pub const MESSAGE_AUTH: &[u8] = b"omnimesh-message-auth-v1";
}

/// Derive a key using HKDF-SHA256.
///
/// # Arguments
/// * `ikm` — Input keying material (e.g., shared secret from key exchange)
/// * `salt` — Optional salt (if None, HKDF uses a zero-filled salt)
/// * `info` — Domain separation label (use constants from [`domain`])
/// * `output_len` — Desired output key length in bytes (max 8160 for SHA-256)
///
/// # Returns
/// A derived key of the requested length.
pub fn derive_key(
    ikm: &[u8],
    salt: Option<&[u8]>,
    info: &[u8],
    output_len: usize,
) -> Result<Vec<u8>> {
    if output_len == 0 {
        return Err(OmniMeshError::KeyDerivation(
            "output length must be > 0".into(),
        ));
    }
    if output_len > 255 * 32 {
        return Err(OmniMeshError::KeyDerivation(format!(
            "output length {} exceeds HKDF-SHA256 maximum (8160)",
            output_len
        )));
    }

    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut output = vec![0u8; output_len];
    hk.expand(info, &mut output)
        .map_err(|e| OmniMeshError::KeyDerivation(format!("HKDF expand failed: {}", e)))?;
    Ok(output)
}

/// Derive a fixed-size 256-bit key.
///
/// Convenience wrapper around [`derive_key`] for the common case.
pub fn derive_key_256(ikm: &[u8], salt: Option<&[u8]>, info: &[u8]) -> Result<[u8; 32]> {
    let derived = derive_key(ikm, salt, info, 32)?;
    let mut key = [0u8; 32];
    key.copy_from_slice(&derived);
    Ok(key)
}

/// Derive multiple keys from a single input (for split key patterns).
///
/// Returns `(encryption_key, authentication_key)` each 32 bytes.
pub fn derive_key_pair(
    ikm: &[u8],
    salt: Option<&[u8]>,
    info: &[u8],
) -> Result<([u8; 32], [u8; 32])> {
    let mut derived = derive_key(ikm, salt, info, 64)?;
    let mut enc_key = [0u8; 32];
    let mut auth_key = [0u8; 32];
    enc_key.copy_from_slice(&derived[..32]);
    auth_key.copy_from_slice(&derived[32..]);
    derived.zeroize();
    Ok((enc_key, auth_key))
}

/// Ratchet a key forward (derive the next key from the current one).
///
/// Used for forward secrecy — after ratcheting, the old key cannot
/// be recovered from the new one.
///
/// This follows the same pattern as QuantumVault's session ratcheting.
pub fn ratchet_key(current_key: &[u8; 32]) -> Result<[u8; 32]> {
    derive_key_256(current_key, None, domain::SESSION_RATCHET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_deterministic() {
        let ikm = b"input keying material";
        let salt = b"some salt";
        let info = domain::TRANSPORT;

        let key1 = derive_key(ikm, Some(salt), info, 32).unwrap();
        let key2 = derive_key(ikm, Some(salt), info, 32).unwrap();

        assert_eq!(key1, key2);
        assert_eq!(key1.len(), 32);
    }

    #[test]
    fn test_different_domains_different_keys() {
        let ikm = b"same input";

        let key1 = derive_key_256(ikm, None, domain::TRANSPORT).unwrap();
        let key2 = derive_key_256(ikm, None, domain::KEYSTORE).unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_different_salts_different_keys() {
        let ikm = b"same input";
        let info = domain::TRANSPORT;

        let key1 = derive_key_256(ikm, Some(b"salt-a"), info).unwrap();
        let key2 = derive_key_256(ikm, Some(b"salt-b"), info).unwrap();

        assert_ne!(key1, key2);
    }

    #[test]
    fn test_derive_key_pair() {
        let ikm = b"key exchange result";
        let (enc, auth) = derive_key_pair(ikm, None, domain::TRANSPORT).unwrap();

        assert_ne!(enc, auth);
        assert_eq!(enc.len(), 32);
        assert_eq!(auth.len(), 32);
    }

    #[test]
    fn test_ratchet_key_produces_different_key() {
        let key = [42u8; 32];
        let next = ratchet_key(&key).unwrap();
        assert_ne!(key, next);
    }

    #[test]
    fn test_ratchet_is_deterministic() {
        let key = [42u8; 32];
        let next1 = ratchet_key(&key).unwrap();
        let next2 = ratchet_key(&key).unwrap();
        assert_eq!(next1, next2);
    }

    #[test]
    fn test_ratchet_chain_is_one_way() {
        let key0 = [1u8; 32];
        let key1 = ratchet_key(&key0).unwrap();
        let key2 = ratchet_key(&key1).unwrap();

        // Each step produces a different key
        assert_ne!(key0, key1);
        assert_ne!(key1, key2);
        assert_ne!(key0, key2);
    }

    #[test]
    fn test_zero_length_rejected() {
        assert!(derive_key(b"ikm", None, b"info", 0).is_err());
    }

    #[test]
    fn test_excessive_length_rejected() {
        assert!(derive_key(b"ikm", None, b"info", 10_000).is_err());
    }

    #[test]
    fn test_variable_output_lengths() {
        for len in [16, 32, 48, 64, 128] {
            let key = derive_key(b"ikm", None, b"info", len).unwrap();
            assert_eq!(key.len(), len);
        }
    }
}
