//! Encrypted key storage with the `KeyStore` trait.
//!
//! Private keys are encrypted at rest using ChaCha20-Poly1305 with a
//! passphrase-derived key (HKDF). The `KeyStore` trait allows swapping
//! storage backends (file, database, HSM) without changing business logic.

use std::path::PathBuf;

use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tracing::{debug, info};
use zeroize::Zeroize;

use omnimesh_core::error::{OmniMeshError, Result};

use crate::keypair::Keypair;

/// Trait for pluggable key storage backends.
///
/// Implementations must handle encryption/decryption transparently.
/// The `passphrase` parameter is used to derive the encryption key.
pub trait KeyStore: Send + Sync {
    /// Store a keypair, encrypted with the given passphrase.
    fn store(&self, keypair: &Keypair, passphrase: &str) -> Result<()>;

    /// Load a keypair, decrypting with the given passphrase.
    fn load(&self, passphrase: &str) -> Result<Keypair>;

    /// Check if a stored key exists.
    fn exists(&self) -> bool;

    /// Delete the stored key (zeroize and remove).
    fn delete(&self) -> Result<()>;
}

/// File-based encrypted key storage.
///
/// Keys are stored as JSON with the encrypted seed and nonce.
pub struct FileKeyStore {
    path: PathBuf,
}

/// On-disk format for an encrypted key.
#[derive(Serialize, Deserialize)]
struct EncryptedKey {
    /// Version of the storage format (for future migration).
    version: u32,
    /// 12-byte nonce used for ChaCha20-Poly1305.
    nonce: [u8; 12],
    /// Encrypted 32-byte seed + 16-byte auth tag = 48 bytes.
    ciphertext: Vec<u8>,
    /// Salt for HKDF key derivation (32 bytes).
    salt: [u8; 32],
    /// Timestamp of when this key was stored.
    created_at: String,
}

impl FileKeyStore {
    /// Create a new file-based key store at the given path.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Derive an encryption key from a passphrase and salt using HKDF.
    fn derive_key(passphrase: &str, salt: &[u8; 32]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(salt), passphrase.as_bytes());
        let mut key = [0u8; 32];
        hk.expand(b"omnimesh-keystore-v1", &mut key)
            .expect("HKDF expand failed — this should never happen with 32-byte output");
        key
    }
}

impl KeyStore for FileKeyStore {
    fn store(&self, keypair: &Keypair, passphrase: &str) -> Result<()> {
        // Generate random salt and nonce
        let mut salt = [0u8; 32];
        let mut nonce_bytes = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce_bytes);

        let mut enc_key = Self::derive_key(passphrase, &salt);
        let cipher = ChaCha20Poly1305::new_from_slice(&enc_key)
            .map_err(|e| OmniMeshError::KeyStore(format!("cipher init failed: {}", e)))?;
        enc_key.zeroize();
        let nonce = Nonce::from_slice(&nonce_bytes);

        // Encrypt the seed
        let seed = keypair.seed();
        let ciphertext = cipher
            .encrypt(nonce, seed.as_ref())
            .map_err(|e| OmniMeshError::KeyStore(format!("encryption failed: {}", e)))?;

        // Build on-disk record
        let record = EncryptedKey {
            version: 1,
            nonce: nonce_bytes,
            ciphertext,
            salt,
            created_at: chrono::Utc::now().to_rfc3339(),
        };

        // Ensure parent directory exists
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                OmniMeshError::KeyStore(format!("failed to create key directory: {}", e))
            })?;
        }

        // Write atomically (write to temp, then rename)
        let json = serde_json::to_string_pretty(&record)
            .map_err(|e| OmniMeshError::KeyStore(format!("serialization failed: {}", e)))?;
        let tmp_path = self.path.with_extension("tmp");
        std::fs::write(&tmp_path, &json)
            .map_err(|e| OmniMeshError::KeyStore(format!("failed to write key file: {}", e)))?;
        std::fs::rename(&tmp_path, &self.path)
            .map_err(|e| OmniMeshError::KeyStore(format!("failed to rename key file: {}", e)))?;

        info!(path = %self.path.display(), "keypair stored successfully");
        Ok(())
    }

    fn load(&self, passphrase: &str) -> Result<Keypair> {
        let json = std::fs::read_to_string(&self.path)
            .map_err(|e| OmniMeshError::KeyStore(format!("failed to read key file: {}", e)))?;
        let record: EncryptedKey = serde_json::from_str(&json)
            .map_err(|e| OmniMeshError::KeyStore(format!("invalid key file format: {}", e)))?;

        if record.version != 1 {
            return Err(OmniMeshError::KeyStore(format!(
                "unsupported key format version: {}",
                record.version
            )));
        }

        let mut enc_key = Self::derive_key(passphrase, &record.salt);
        let cipher = ChaCha20Poly1305::new_from_slice(&enc_key)
            .map_err(|e| OmniMeshError::KeyStore(format!("cipher init failed: {}", e)))?;
        enc_key.zeroize();
        let nonce = Nonce::from_slice(&record.nonce);

        // Decrypt seed
        let seed_bytes = cipher
            .decrypt(nonce, record.ciphertext.as_ref())
            .map_err(|_| {
                OmniMeshError::KeyStore(
                    "decryption failed — wrong passphrase or corrupted file".into(),
                )
            })?;

        if seed_bytes.len() != 32 {
            return Err(OmniMeshError::KeyStore(format!(
                "invalid seed length: expected 32, got {}",
                seed_bytes.len()
            )));
        }

        let mut seed = [0u8; 32];
        seed.copy_from_slice(&seed_bytes);
        let keypair = Keypair::from_seed(&seed);

        debug!(path = %self.path.display(), "keypair loaded successfully");
        Ok(keypair)
    }

    fn exists(&self) -> bool {
        self.path.exists()
    }

    fn delete(&self) -> Result<()> {
        if self.path.exists() {
            // Overwrite with zeros before deleting
            let zeros = vec![0u8; 512];
            std::fs::write(&self.path, &zeros).ok();
            std::fs::remove_file(&self.path).map_err(|e| {
                OmniMeshError::KeyStore(format!("failed to delete key file: {}", e))
            })?;
            info!(path = %self.path.display(), "keypair deleted");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::peer_id::PeerIdExt;
    use std::path::Path;
    use tempfile::TempDir;

    fn test_store(dir: &Path) -> FileKeyStore {
        FileKeyStore::new(dir.join("test.key"))
    }

    #[test]
    fn test_store_and_load() {
        let dir = TempDir::new().unwrap();
        let store = test_store(dir.path());
        let passphrase = "test-passphrase-123";

        let kp = Keypair::generate();
        let original_peer_id = kp.to_peer_id();

        store.store(&kp, passphrase).unwrap();
        assert!(store.exists());

        let loaded = store.load(passphrase).unwrap();
        assert_eq!(loaded.to_peer_id(), original_peer_id);
    }

    #[test]
    fn test_wrong_passphrase_fails() {
        let dir = TempDir::new().unwrap();
        let store = test_store(dir.path());

        let kp = Keypair::generate();
        store.store(&kp, "correct-passphrase").unwrap();

        let result = store.load("wrong-passphrase");
        assert!(result.is_err());
    }

    #[test]
    fn test_nonexistent_file() {
        let dir = TempDir::new().unwrap();
        let store = test_store(dir.path());

        assert!(!store.exists());
        assert!(store.load("any").is_err());
    }

    #[test]
    fn test_delete() {
        let dir = TempDir::new().unwrap();
        let store = test_store(dir.path());

        let kp = Keypair::generate();
        store.store(&kp, "pass").unwrap();
        assert!(store.exists());

        store.delete().unwrap();
        assert!(!store.exists());
    }

    #[test]
    fn test_store_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let nested_path = dir.path().join("deep").join("nested").join("key.json");
        let store = FileKeyStore::new(nested_path);

        let kp = Keypair::generate();
        store.store(&kp, "pass").unwrap();
        assert!(store.exists());
    }

    #[test]
    fn test_different_salts_produce_different_keys() {
        let salt1 = [1u8; 32];
        let salt2 = [2u8; 32];
        let key1 = FileKeyStore::derive_key("same-pass", &salt1);
        let key2 = FileKeyStore::derive_key("same-pass", &salt2);
        assert_ne!(key1, key2);
    }
}
