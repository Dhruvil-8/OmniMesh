//! Unified error types for the OmniMesh ecosystem.
//!
//! Every crate in the workspace uses [`OmniMeshError`] as its primary error type.
//! This ensures consistent error handling, logging, and propagation across layers.

use thiserror::Error;

/// Unified error type for all OmniMesh operations.
#[derive(Error, Debug)]
pub enum OmniMeshError {
    // ── Identity Errors ──────────────────────────────────────────────
    /// Failed to generate or parse a cryptographic keypair.
    #[error("identity error: {0}")]
    Identity(String),

    /// Key storage operation failed (read, write, or decrypt).
    #[error("keystore error: {0}")]
    KeyStore(String),

    /// Key rotation failed or epoch mismatch.
    #[error("key rotation error: {0}")]
    KeyRotation(String),

    // ── Crypto Errors ────────────────────────────────────────────────
    /// Encryption or decryption failed.
    #[error("crypto error: {0}")]
    Crypto(String),

    /// Noise protocol handshake failed.
    #[error("handshake error: {0}")]
    Handshake(String),

    /// Key derivation failed.
    #[error("key derivation error: {0}")]
    KeyDerivation(String),

    // ── Transport Errors ─────────────────────────────────────────────
    /// Connection establishment failed.
    #[error("connection error: {0}")]
    Connection(String),

    /// Data transmission failed.
    #[error("transport error: {0}")]
    Transport(String),

    /// Connection timed out.
    #[error("timeout after {0}ms")]
    Timeout(u64),

    /// Address could not be resolved or is invalid.
    #[error("address error: {0}")]
    Address(String),

    // ── Routing Errors ───────────────────────────────────────────────
    /// No route found to destination.
    #[error("no route to peer: {0}")]
    NoRoute(String),

    /// Routing table inconsistency detected.
    #[error("routing error: {0}")]
    Routing(String),

    // ── Discovery Errors ─────────────────────────────────────────────
    /// Peer discovery failed.
    #[error("discovery error: {0}")]
    Discovery(String),

    // ── Service Errors ───────────────────────────────────────────────
    /// Service registration or lookup failed.
    #[error("service error: {0}")]
    Service(String),

    // ── Configuration Errors ─────────────────────────────────────────
    /// Configuration file is missing or malformed.
    #[error("config error: {0}")]
    Config(String),

    // ── Storage Errors ───────────────────────────────────────────────
    /// Persistent storage operation failed.
    #[error("storage error: {0}")]
    Storage(String),

    // ── Protocol Errors ──────────────────────────────────────────────
    /// Protocol version mismatch.
    #[error("protocol version mismatch: local={local}, remote={remote}")]
    ProtocolMismatch { local: String, remote: String },

    /// Malformed or invalid packet received.
    #[error("invalid packet: {0}")]
    InvalidPacket(String),

    // ── Generic Errors ───────────────────────────────────────────────
    /// An I/O error occurred.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Serialization or deserialization error.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// An unexpected internal error.
    #[error("internal error: {0}")]
    Internal(String),
}

/// Convenience Result type using [`OmniMeshError`].
pub type Result<T> = std::result::Result<T, OmniMeshError>;

impl OmniMeshError {
    /// Returns `true` if this error is retryable (transient failures).
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            OmniMeshError::Connection(_)
                | OmniMeshError::Timeout(_)
                | OmniMeshError::Transport(_)
                | OmniMeshError::Discovery(_)
                | OmniMeshError::Io(_)
        )
    }

    /// Returns `true` if this error represents a security violation.
    pub fn is_security_error(&self) -> bool {
        matches!(
            self,
            OmniMeshError::Crypto(_)
                | OmniMeshError::Handshake(_)
                | OmniMeshError::KeyStore(_)
                | OmniMeshError::InvalidPacket(_)
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retryable_errors() {
        assert!(OmniMeshError::Connection("refused".into()).is_retryable());
        assert!(OmniMeshError::Timeout(5000).is_retryable());
        assert!(!OmniMeshError::Crypto("bad key".into()).is_retryable());
        assert!(!OmniMeshError::Config("missing".into()).is_retryable());
    }

    #[test]
    fn test_security_errors() {
        assert!(OmniMeshError::Crypto("invalid".into()).is_security_error());
        assert!(OmniMeshError::Handshake("failed".into()).is_security_error());
        assert!(!OmniMeshError::Connection("reset".into()).is_security_error());
    }

    #[test]
    fn test_error_display() {
        let err = OmniMeshError::Timeout(3000);
        assert_eq!(err.to_string(), "timeout after 3000ms");

        let err = OmniMeshError::ProtocolMismatch {
            local: "1.0".into(),
            remote: "2.0".into(),
        };
        assert_eq!(
            err.to_string(),
            "protocol version mismatch: local=1.0, remote=2.0"
        );
    }
}
