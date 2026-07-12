//! Common types used across the OmniMesh workspace.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Protocol version for wire compatibility.
///
/// Every handshake includes version negotiation. Nodes must agree on a
/// compatible version before exchanging data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProtocolVersion {
    /// Major version — breaking changes.
    pub major: u16,
    /// Minor version — backward-compatible additions.
    pub minor: u16,
    /// Patch version — backward-compatible fixes.
    pub patch: u16,
}

impl ProtocolVersion {
    /// Current protocol version.
    pub const CURRENT: Self = Self {
        major: 0,
        minor: 1,
        patch: 0,
    };

    /// Check if this version is compatible with another.
    ///
    /// Compatibility rules:
    /// - Same major version → compatible
    /// - Different major version → incompatible
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        self.major == other.major
    }
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Unique identifier for a peer in the mesh network.
///
/// Derived from the SHA-256 hash of the peer's Ed25519 public key,
/// truncated to 32 bytes. This is the primary addressing mechanism.
#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PeerId(pub [u8; 32]);

impl PeerId {
    /// Create a PeerId from raw bytes.
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Get the raw bytes of this PeerId.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Encode as a hex string.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }

    /// Decode from a hex string.
    pub fn from_hex(s: &str) -> crate::Result<Self> {
        let bytes = hex::decode(s)
            .map_err(|e| crate::OmniMeshError::Identity(format!("invalid hex PeerId: {}", e)))?;
        if bytes.len() != 32 {
            return Err(crate::OmniMeshError::Identity(format!(
                "PeerId must be 32 bytes, got {}",
                bytes.len()
            )));
        }
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&bytes);
        Ok(Self(arr))
    }

    /// Encode as a base64 string (URL-safe, no padding).
    pub fn to_base64(&self) -> String {
        use base64::Engine;
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(self.0)
    }

    /// Short display form (first 8 hex characters).
    pub fn short(&self) -> String {
        self.to_hex()[..8].to_string()
    }
}

impl fmt::Debug for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "PeerId({}…)", &self.to_hex()[..8])
    }
}

impl fmt::Display for PeerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// Information about a node in the mesh.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    /// Unique peer identifier.
    pub peer_id: PeerId,
    /// Human-readable node name.
    pub name: String,
    /// Protocol version this node speaks.
    pub version: ProtocolVersion,
    /// Addresses this node is reachable at.
    pub addresses: Vec<String>,
    /// Unix timestamp of when this info was last updated.
    pub last_seen: u64,
    /// Whether this node acts as a relay.
    pub is_relay: bool,
}

/// Connection state of a peer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    /// Not connected.
    Disconnected,
    /// Connection attempt in progress.
    Connecting,
    /// Fully connected and authenticated.
    Connected,
    /// Connection is being gracefully closed.
    Disconnecting,
}

impl fmt::Display for ConnectionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "disconnected"),
            Self::Connecting => write!(f, "connecting"),
            Self::Connected => write!(f, "connected"),
            Self::Disconnecting => write!(f, "disconnecting"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_protocol_version_compatibility() {
        let v1 = ProtocolVersion {
            major: 1,
            minor: 0,
            patch: 0,
        };
        let v1_1 = ProtocolVersion {
            major: 1,
            minor: 1,
            patch: 0,
        };
        let v2 = ProtocolVersion {
            major: 2,
            minor: 0,
            patch: 0,
        };

        assert!(v1.is_compatible_with(&v1_1));
        assert!(!v1.is_compatible_with(&v2));
    }

    #[test]
    fn test_protocol_version_display() {
        assert_eq!(ProtocolVersion::CURRENT.to_string(), "0.1.0");
    }

    #[test]
    fn test_peer_id_hex_roundtrip() {
        let bytes = [42u8; 32];
        let peer_id = PeerId::from_bytes(bytes);
        let hex = peer_id.to_hex();
        let recovered = PeerId::from_hex(&hex).unwrap();
        assert_eq!(peer_id, recovered);
    }

    #[test]
    fn test_peer_id_short() {
        let peer_id = PeerId::from_bytes([0xAB; 32]);
        assert_eq!(peer_id.short().len(), 8);
    }

    #[test]
    fn test_peer_id_debug() {
        let peer_id = PeerId::from_bytes([0xFF; 32]);
        let debug = format!("{:?}", peer_id);
        assert!(debug.starts_with("PeerId(ffffffff"));
    }

    #[test]
    fn test_connection_state_display() {
        assert_eq!(ConnectionState::Connected.to_string(), "connected");
        assert_eq!(ConnectionState::Disconnected.to_string(), "disconnected");
    }
}
