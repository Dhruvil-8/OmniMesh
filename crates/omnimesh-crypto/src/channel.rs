//! Secure bidirectional channel with key ratcheting.
//!
//! Combines AEAD encryption with HKDF key ratcheting to provide
//! forward secrecy. After each message, the session key is ratcheted
//! forward so that compromising the current key cannot decrypt past messages.
//!
//! Inspired by QuantumVault's `PQSession` ratcheting pattern.

use zeroize::Zeroize;

use omnimesh_core::error::{OmniMeshError, Result};

use crate::aead;
use crate::kdf;

/// Maximum messages before mandatory rekeying.
/// After this many messages, the channel must be re-established
/// via a new handshake to prevent nonce exhaustion.
const REKEY_THRESHOLD: u64 = 1_000_000;

/// A secure bidirectional channel with forward secrecy.
///
/// Each encrypt/decrypt operation ratchets the key forward,
/// ensuring that old messages cannot be decrypted even if the
/// current key is compromised.
pub struct SecureChannel {
    /// Current sending key (ratcheted after each encrypt).
    send_key: [u8; 32],
    /// Current receiving key (ratcheted after each decrypt).
    recv_key: [u8; 32],
    /// Number of messages sent.
    send_counter: u64,
    /// Number of messages received.
    recv_counter: u64,
}

impl SecureChannel {
    /// Create a new secure channel from a shared secret.
    ///
    /// The shared secret (e.g., from a Noise handshake) is split
    /// into separate send and receive keys using domain-separated
    /// key derivation.
    ///
    /// # Arguments
    /// * `shared_secret` — The key material from the handshake
    /// * `is_initiator` — If true, this side uses key_a for sending and key_b for receiving.
    ///   The responder side does the opposite.
    pub fn new(shared_secret: &[u8; 32], is_initiator: bool) -> Result<Self> {
        let (key_a, key_b) = kdf::derive_key_pair(shared_secret, None, kdf::domain::TRANSPORT)?;

        let (send_key, recv_key) = if is_initiator {
            (key_a, key_b)
        } else {
            (key_b, key_a)
        };

        Ok(Self {
            send_key,
            recv_key,
            send_counter: 0,
            recv_counter: 0,
        })
    }

    /// Encrypt a message and ratchet the send key.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        if self.send_counter >= REKEY_THRESHOLD {
            return Err(OmniMeshError::Crypto(
                "send key exhausted — rekeying required".into(),
            ));
        }

        let ciphertext = aead::encrypt(&self.send_key, plaintext)?;

        // Ratchet the send key forward
        self.send_key = kdf::ratchet_key(&self.send_key)?;
        self.send_counter += 1;

        Ok(ciphertext)
    }

    /// Decrypt a message and ratchet the receive key.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if self.recv_counter >= REKEY_THRESHOLD {
            return Err(OmniMeshError::Crypto(
                "receive key exhausted — rekeying required".into(),
            ));
        }

        let plaintext = aead::decrypt(&self.recv_key, ciphertext)?;

        // Ratchet the receive key forward
        self.recv_key = kdf::ratchet_key(&self.recv_key)?;
        self.recv_counter += 1;

        Ok(plaintext)
    }

    /// Get the number of messages sent through this channel.
    pub fn send_count(&self) -> u64 {
        self.send_counter
    }

    /// Get the number of messages received through this channel.
    pub fn recv_count(&self) -> u64 {
        self.recv_counter
    }

    /// Get remaining messages before mandatory rekeying.
    pub fn messages_remaining(&self) -> u64 {
        REKEY_THRESHOLD.saturating_sub(self.send_counter.max(self.recv_counter))
    }
}

impl Drop for SecureChannel {
    fn drop(&mut self) {
        self.send_key.zeroize();
        self.recv_key.zeroize();
    }
}

impl std::fmt::Debug for SecureChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecureChannel")
            .field("send_counter", &self.send_counter)
            .field("recv_counter", &self.recv_counter)
            .field("messages_remaining", &self.messages_remaining())
            .field("keys", &"[REDACTED]")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_channel_pair() -> (SecureChannel, SecureChannel) {
        let secret = [42u8; 32];
        let alice = SecureChannel::new(&secret, true).unwrap();
        let bob = SecureChannel::new(&secret, false).unwrap();
        (alice, bob)
    }

    #[test]
    fn test_channel_roundtrip() {
        let (mut alice, mut bob) = create_channel_pair();
        let plaintext = b"hello bob";

        let ct = alice.encrypt(plaintext).unwrap();
        let pt = bob.decrypt(&ct).unwrap();

        assert_eq!(pt, plaintext);
    }

    #[test]
    fn test_channel_bidirectional() {
        let (mut alice, mut bob) = create_channel_pair();

        // Alice → Bob
        let ct1 = alice.encrypt(b"from alice").unwrap();
        let pt1 = bob.decrypt(&ct1).unwrap();
        assert_eq!(pt1, b"from alice");

        // Bob → Alice
        let ct2 = bob.encrypt(b"from bob").unwrap();
        let pt2 = alice.decrypt(&ct2).unwrap();
        assert_eq!(pt2, b"from bob");
    }

    #[test]
    fn test_channel_ratcheting() {
        let (mut alice, mut bob) = create_channel_pair();

        // Send multiple messages — each uses a different key
        for i in 0..5 {
            let msg = format!("message {}", i);
            let ct = alice.encrypt(msg.as_bytes()).unwrap();
            let pt = bob.decrypt(&ct).unwrap();
            assert_eq!(pt, msg.as_bytes());
        }

        assert_eq!(alice.send_count(), 5);
        assert_eq!(bob.recv_count(), 5);
    }

    #[test]
    fn test_channel_replay_fails() {
        let (mut alice, mut bob) = create_channel_pair();

        let ct = alice.encrypt(b"first message").unwrap();
        let _pt = bob.decrypt(&ct).unwrap();

        // Replaying the same ciphertext should fail because
        // bob's receive key has ratcheted forward
        assert!(bob.decrypt(&ct).is_err());
    }

    #[test]
    fn test_channel_wrong_direction_fails() {
        let (mut alice, mut _bob) = create_channel_pair();

        let ct = alice.encrypt(b"from alice").unwrap();
        // Alice cannot decrypt her own messages (wrong key)
        assert!(alice.decrypt(&ct).is_err());
    }

    #[test]
    fn test_channel_counters() {
        let (mut alice, mut bob) = create_channel_pair();

        assert_eq!(alice.send_count(), 0);
        assert_eq!(alice.messages_remaining(), REKEY_THRESHOLD);

        let ct = alice.encrypt(b"test").unwrap();
        bob.decrypt(&ct).unwrap();

        assert_eq!(alice.send_count(), 1);
        assert_eq!(bob.recv_count(), 1);
    }

    #[test]
    fn test_channel_debug_redacts_keys() {
        let (alice, _bob) = create_channel_pair();
        let debug = format!("{:?}", alice);
        assert!(debug.contains("[REDACTED]"));
    }
}
