//! ChaCha20-Poly1305 authenticated encryption/decryption.
//!
//! Provides a simple, safe wrapper around the `chacha20poly1305` crate.
//! Each encryption generates a random 12-byte nonce, prepended to the ciphertext.
//!
//! Wire format: `[nonce: 12 bytes][ciphertext + tag: N + 16 bytes]`

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use rand::RngCore;

use omnimesh_core::error::{OmniMeshError, Result};

/// Size of the nonce in bytes.
pub const NONCE_SIZE: usize = 12;

/// Size of the authentication tag in bytes.
pub const TAG_SIZE: usize = 16;

/// Encrypt plaintext with a 256-bit key.
///
/// Returns `nonce || ciphertext || tag`.
/// A fresh random nonce is generated for each call.
pub fn encrypt(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| OmniMeshError::Crypto(format!("cipher init: {}", e)))?;

    let mut nonce_bytes = [0u8; NONCE_SIZE];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| OmniMeshError::Crypto(format!("encryption failed: {}", e)))?;

    // Prepend nonce to ciphertext
    let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(&nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

/// Decrypt ciphertext that was encrypted with [`encrypt`].
///
/// Input format: `nonce (12 bytes) || ciphertext || tag (16 bytes)`.
pub fn decrypt(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_SIZE + TAG_SIZE {
        return Err(OmniMeshError::Crypto(format!(
            "ciphertext too short: {} bytes (minimum {})",
            data.len(),
            NONCE_SIZE + TAG_SIZE
        )));
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_SIZE);
    let nonce = Nonce::from_slice(nonce_bytes);

    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| OmniMeshError::Crypto(format!("cipher init: {}", e)))?;

    cipher.decrypt(nonce, ciphertext).map_err(|_| {
        OmniMeshError::Crypto("decryption failed: invalid key or corrupted data".into())
    })
}

/// Encrypt with an explicit nonce (for deterministic testing or counter-based schemes).
///
/// **Warning:** Reusing a nonce with the same key is catastrophic.
/// Only use this if you have a reliable nonce generation scheme.
pub fn encrypt_with_nonce(
    key: &[u8; 32],
    nonce_bytes: &[u8; NONCE_SIZE],
    plaintext: &[u8],
) -> Result<Vec<u8>> {
    let cipher = ChaCha20Poly1305::new_from_slice(key)
        .map_err(|e| OmniMeshError::Crypto(format!("cipher init: {}", e)))?;

    let nonce = Nonce::from_slice(nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| OmniMeshError::Crypto(format!("encryption failed: {}", e)))?;

    let mut output = Vec::with_capacity(NONCE_SIZE + ciphertext.len());
    output.extend_from_slice(nonce_bytes);
    output.extend_from_slice(&ciphertext);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_key() -> [u8; 32] {
        let mut key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut key);
        key
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = test_key();
        let plaintext = b"hello omnimesh crypto";

        let encrypted = encrypt(&key, plaintext).unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_different_keys_fail() {
        let key1 = test_key();
        let key2 = test_key();
        let plaintext = b"secret data";

        let encrypted = encrypt(&key1, plaintext).unwrap();
        let result = decrypt(&key2, &encrypted);

        assert!(result.is_err());
    }

    #[test]
    fn test_tampered_ciphertext_fails() {
        let key = test_key();
        let plaintext = b"integrity check";

        let mut encrypted = encrypt(&key, plaintext).unwrap();
        // Flip a byte in the ciphertext
        let last = encrypted.len() - 1;
        encrypted[last] ^= 0xFF;

        assert!(decrypt(&key, &encrypted).is_err());
    }

    #[test]
    fn test_empty_plaintext() {
        let key = test_key();
        let encrypted = encrypt(&key, b"").unwrap();
        let decrypted = decrypt(&key, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_short_ciphertext_rejected() {
        let key = test_key();
        let result = decrypt(&key, &[0u8; 10]);
        assert!(result.is_err());
    }

    #[test]
    fn test_explicit_nonce_deterministic() {
        let key = test_key();
        let nonce = [42u8; NONCE_SIZE];
        let plaintext = b"deterministic";

        let enc1 = encrypt_with_nonce(&key, &nonce, plaintext).unwrap();
        let enc2 = encrypt_with_nonce(&key, &nonce, plaintext).unwrap();

        // Same key + nonce + plaintext = same output
        assert_eq!(enc1, enc2);
    }

    #[test]
    fn test_output_size() {
        let key = test_key();
        let plaintext = b"size check";
        let encrypted = encrypt(&key, plaintext).unwrap();

        assert_eq!(encrypted.len(), NONCE_SIZE + plaintext.len() + TAG_SIZE);
    }
}
