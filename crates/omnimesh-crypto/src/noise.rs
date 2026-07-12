//! Noise_XX handshake wrapper using the `snow` crate.
//!
//! The Noise_XX pattern provides mutual authentication:
//! - Both sides prove they hold their private key
//! - Both sides learn each other's static public key
//! - Forward secrecy via ephemeral Diffie-Hellman
//!
//! ## Handshake Flow
//!
//! ```text
//! Initiator                         Responder
//!     │                                 │
//!     │──── e ─────────────────────────▶│  (message 1)
//!     │                                 │
//!     │◀─── e, ee, s, es ──────────────│  (message 2)
//!     │                                 │
//!     │──── s, se ─────────────────────▶│  (message 3)
//!     │                                 │
//!     │◀═══════ encrypted channel ═════▶│
//! ```

use rand::RngCore;
use snow::{params::NoiseParams, Builder, HandshakeState, TransportState};
use tracing::info;

use omnimesh_core::error::{OmniMeshError, Result};

/// Noise protocol pattern used by OmniMesh.
const NOISE_PATTERN: &str = "Noise_XX_25519_ChaChaPoly_BLAKE2s";

/// Maximum handshake message size.
const MAX_HANDSHAKE_MSG: usize = 65535;

/// Role in the Noise handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The side that initiates the connection.
    Initiator,
    /// The side that accepts the connection.
    Responder,
}

/// A Noise_XX handshake in progress.
///
/// Call [`NoiseHandshake::write_message`] and [`NoiseHandshake::read_message`]
/// alternately until [`NoiseHandshake::is_finished`] returns true, then
/// call [`NoiseHandshake::into_transport`] to get the secure channel.
pub struct NoiseHandshake {
    state: HandshakeState,
    role: Role,
}

/// A completed Noise transport (encrypted channel).
///
/// After the handshake completes, this provides `encrypt` and `decrypt`
/// methods for application data.
pub struct NoiseTransport {
    state: TransportState,
    remote_static: Option<[u8; 32]>,
}

impl NoiseHandshake {
    /// Create a new handshake with a freshly generated keypair.
    pub fn new(role: Role) -> Result<Self> {
        let mut private_key = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut private_key);
        Self::with_keypair(role, &private_key)
    }

    /// Create a handshake with a specific static keypair.
    ///
    /// Use this when you want the handshake to authenticate with a
    /// pre-existing identity key rather than a generated one.
    pub fn with_keypair(role: Role, private_key: &[u8; 32]) -> Result<Self> {
        let params: NoiseParams = NOISE_PATTERN
            .parse()
            .map_err(|e| OmniMeshError::Handshake(format!("invalid noise params: {}", e)))?;

        let builder = Builder::new(params).local_private_key(private_key);
        let state = match role {
            Role::Initiator => builder
                .build_initiator()
                .map_err(|e| OmniMeshError::Handshake(format!("init failed: {}", e)))?,
            Role::Responder => builder
                .build_responder()
                .map_err(|e| OmniMeshError::Handshake(format!("init failed: {}", e)))?,
        };

        Ok(Self { state, role })
    }

    /// Write the next handshake message.
    ///
    /// Returns the bytes to send to the remote peer.
    pub fn write_message(&mut self, payload: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; MAX_HANDSHAKE_MSG];
        let len = self
            .state
            .write_message(payload, &mut buf)
            .map_err(|e| OmniMeshError::Handshake(format!("write failed: {}", e)))?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Read a handshake message from the remote peer.
    ///
    /// Returns any payload embedded in the message.
    pub fn read_message(&mut self, message: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; MAX_HANDSHAKE_MSG];
        let len = self
            .state
            .read_message(message, &mut buf)
            .map_err(|e| OmniMeshError::Handshake(format!("read failed: {}", e)))?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Check if the handshake is complete.
    pub fn is_finished(&self) -> bool {
        self.state.is_handshake_finished()
    }

    /// Get the role of this side.
    pub fn role(&self) -> Role {
        self.role
    }

    /// Convert the completed handshake into a transport (encrypted channel).
    ///
    /// # Errors
    /// Returns an error if the handshake is not yet finished.
    pub fn into_transport(self) -> Result<NoiseTransport> {
        if !self.state.is_handshake_finished() {
            return Err(OmniMeshError::Handshake(
                "handshake not yet complete".into(),
            ));
        }

        let remote_static = self.state.get_remote_static().map(|rs| {
            let mut key = [0u8; 32];
            key.copy_from_slice(rs);
            key
        });

        let transport = self
            .state
            .into_transport_mode()
            .map_err(|e| OmniMeshError::Handshake(format!("transport mode failed: {}", e)))?;

        info!("noise handshake complete — secure channel established");
        Ok(NoiseTransport {
            state: transport,
            remote_static,
        })
    }
}

impl NoiseTransport {
    /// Encrypt application data.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; plaintext.len() + 16]; // +16 for auth tag
        let len = self
            .state
            .write_message(plaintext, &mut buf)
            .map_err(|e| OmniMeshError::Crypto(format!("noise encrypt: {}", e)))?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Decrypt application data.
    pub fn decrypt(&mut self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; ciphertext.len()];
        let len = self
            .state
            .read_message(ciphertext, &mut buf)
            .map_err(|e| OmniMeshError::Crypto(format!("noise decrypt: {}", e)))?;
        buf.truncate(len);
        Ok(buf)
    }

    /// Get the remote peer's static public key (if available).
    ///
    /// After a Noise_XX handshake, this contains the authenticated
    /// public key of the remote peer.
    pub fn remote_static_key(&self) -> Option<&[u8; 32]> {
        self.remote_static.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to perform a full 3-message Noise_XX handshake.
    fn do_handshake() -> (NoiseTransport, NoiseTransport) {
        let mut initiator = NoiseHandshake::new(Role::Initiator).unwrap();
        let mut responder = NoiseHandshake::new(Role::Responder).unwrap();

        // Message 1: Initiator → Responder
        let msg1 = initiator.write_message(b"").unwrap();
        responder.read_message(&msg1).unwrap();

        // Message 2: Responder → Initiator
        let msg2 = responder.write_message(b"").unwrap();
        initiator.read_message(&msg2).unwrap();

        // Message 3: Initiator → Responder
        let msg3 = initiator.write_message(b"").unwrap();
        responder.read_message(&msg3).unwrap();

        assert!(initiator.is_finished());
        assert!(responder.is_finished());

        let i_transport = initiator.into_transport().unwrap();
        let r_transport = responder.into_transport().unwrap();

        (i_transport, r_transport)
    }

    #[test]
    fn test_noise_handshake_completes() {
        let (i, r) = do_handshake();
        // Both sides should have each other's static key
        assert!(i.remote_static_key().is_some());
        assert!(r.remote_static_key().is_some());
    }

    #[test]
    fn test_noise_encrypt_decrypt() {
        let (mut i, mut r) = do_handshake();

        // Initiator encrypts, Responder decrypts
        let plaintext = b"hello from initiator";
        let ciphertext = i.encrypt(plaintext).unwrap();
        let decrypted = r.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);

        // Responder encrypts, Initiator decrypts
        let plaintext2 = b"hello from responder";
        let ciphertext2 = r.encrypt(plaintext2).unwrap();
        let decrypted2 = i.decrypt(&ciphertext2).unwrap();
        assert_eq!(decrypted2, plaintext2);
    }

    #[test]
    fn test_noise_bidirectional_multiple_messages() {
        let (mut i, mut r) = do_handshake();

        for n in 0..10 {
            let msg = format!("message {}", n);
            let ct = i.encrypt(msg.as_bytes()).unwrap();
            let pt = r.decrypt(&ct).unwrap();
            assert_eq!(pt, msg.as_bytes());
        }
    }

    #[test]
    fn test_noise_tampered_ciphertext_fails() {
        let (mut i, mut r) = do_handshake();

        let mut ct = i.encrypt(b"sensitive data").unwrap();
        ct[0] ^= 0xFF; // Tamper

        assert!(r.decrypt(&ct).is_err());
    }

    #[test]
    fn test_into_transport_before_complete_fails() {
        let handshake = NoiseHandshake::new(Role::Initiator).unwrap();
        assert!(!handshake.is_finished());
        assert!(handshake.into_transport().is_err());
    }

    #[test]
    fn test_with_keypair() {
        let private_key = [42u8; 32];

        let mut init = NoiseHandshake::with_keypair(Role::Initiator, &private_key).unwrap();
        let mut resp = NoiseHandshake::new(Role::Responder).unwrap();

        let msg1 = init.write_message(b"").unwrap();
        resp.read_message(&msg1).unwrap();
        let msg2 = resp.write_message(b"").unwrap();
        init.read_message(&msg2).unwrap();
        let msg3 = init.write_message(b"").unwrap();
        resp.read_message(&msg3).unwrap();

        assert!(init.is_finished());
        assert!(resp.is_finished());
    }
}
