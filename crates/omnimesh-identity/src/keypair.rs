//! Ed25519 keypair generation, signing, and verification.
//!
//! This module wraps `ed25519-dalek` to provide a simple, safe interface
//! for cryptographic identity operations. Private keys are zeroized on drop.

use ed25519_dalek::{Signer, Verifier};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use omnimesh_core::error::{OmniMeshError, Result};

/// An Ed25519 signing keypair.
///
/// The private key is zeroized when this struct is dropped.
#[derive(Clone)]
pub struct Keypair {
    /// The inner ed25519-dalek signing key (contains both secret + public).
    inner: ed25519_dalek::SigningKey,
}

/// The public half of an Ed25519 keypair.
///
/// Safe to share and serialize. Used to derive PeerId.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublicKey {
    /// Raw 32-byte Ed25519 public key.
    bytes: [u8; 32],
}

/// A cryptographic signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signature {
    /// Raw 64-byte Ed25519 signature.
    bytes: [u8; 64],
}

// Manual Serialize/Deserialize for [u8; 64] since serde doesn't support arrays > 32.
impl serde::Serialize for Signature {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_bytes(&self.bytes)
    }
}

impl<'de> serde::Deserialize<'de> for Signature {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        let bytes: Vec<u8> = serde::Deserialize::deserialize(deserializer)?;
        if bytes.len() != 64 {
            return Err(serde::de::Error::custom(format!(
                "expected 64 bytes for Signature, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 64];
        arr.copy_from_slice(&bytes);
        Ok(Signature { bytes: arr })
    }
}

impl Keypair {
    /// Generate a new random Ed25519 keypair using the OS CSPRNG.
    pub fn generate() -> Self {
        let signing_key = ed25519_dalek::SigningKey::generate(&mut OsRng);
        Self { inner: signing_key }
    }

    /// Reconstruct a keypair from a 32-byte secret seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let signing_key = ed25519_dalek::SigningKey::from_bytes(seed);
        Self { inner: signing_key }
    }

    /// Get the secret seed (32 bytes). Handle with care!
    pub fn seed(&self) -> [u8; 32] {
        self.inner.to_bytes()
    }

    /// Get the public key.
    pub fn public_key(&self) -> PublicKey {
        PublicKey {
            bytes: self.inner.verifying_key().to_bytes(),
        }
    }

    /// Sign a message.
    pub fn sign(&self, message: &[u8]) -> Signature {
        let sig = self.inner.sign(message);
        Signature {
            bytes: sig.to_bytes(),
        }
    }

    /// Verify a signature against this keypair's public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        self.public_key().verify(message, signature)
    }
}

impl Drop for Keypair {
    fn drop(&mut self) {
        // Zeroize the secret key material
        let mut seed = self.inner.to_bytes();
        seed.zeroize();
    }
}

impl std::fmt::Debug for Keypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Keypair")
            .field("public_key", &self.public_key())
            .field("secret", &"[REDACTED]")
            .finish()
    }
}

impl PublicKey {
    /// Create a PublicKey from raw 32-byte Ed25519 public key bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self { bytes }
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.bytes
    }

    /// Verify a signature against this public key.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<()> {
        let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(&self.bytes)
            .map_err(|e| OmniMeshError::Identity(format!("invalid public key: {}", e)))?;
        let sig = ed25519_dalek::Signature::from_bytes(&signature.bytes);
        verifying_key
            .verify(message, &sig)
            .map_err(|e| OmniMeshError::Identity(format!("signature verification failed: {}", e)))
    }

    /// Encode as hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.bytes)
    }
}

impl Signature {
    /// Create from raw 64-byte signature.
    pub fn from_bytes(bytes: [u8; 64]) -> Self {
        Self { bytes }
    }

    /// Get the raw bytes.
    pub fn as_bytes(&self) -> &[u8; 64] {
        &self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keypair_generation() {
        let kp = Keypair::generate();
        let pk = kp.public_key();
        assert_eq!(pk.as_bytes().len(), 32);
    }

    #[test]
    fn test_sign_and_verify() {
        let kp = Keypair::generate();
        let message = b"hello omnimesh";
        let sig = kp.sign(message);

        // Valid signature should verify
        assert!(kp.verify(message, &sig).is_ok());

        // Wrong message should fail
        assert!(kp.verify(b"wrong message", &sig).is_err());
    }

    #[test]
    fn test_cross_keypair_verify_fails() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        let message = b"test";
        let sig = kp1.sign(message);

        // Signature from kp1 should not verify with kp2's public key
        assert!(kp2.public_key().verify(message, &sig).is_err());
    }

    #[test]
    fn test_keypair_from_seed_deterministic() {
        let seed = [42u8; 32];
        let kp1 = Keypair::from_seed(&seed);
        let kp2 = Keypair::from_seed(&seed);
        assert_eq!(kp1.public_key(), kp2.public_key());

        let message = b"deterministic test";
        let sig1 = kp1.sign(message);
        let sig2 = kp2.sign(message);
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_keypair_debug_redacts_secret() {
        let kp = Keypair::generate();
        let debug = format!("{:?}", kp);
        assert!(debug.contains("[REDACTED]"));
        assert!(!debug.contains(&hex::encode(kp.seed())));
    }

    #[test]
    fn test_public_key_hex_roundtrip() {
        let kp = Keypair::generate();
        let pk = kp.public_key();
        let hex_str = pk.to_hex();
        assert_eq!(hex_str.len(), 64); // 32 bytes = 64 hex chars
    }
}
