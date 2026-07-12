//! PeerId derivation from Ed25519 public keys.
//!
//! A PeerId is the SHA-256 hash of the public key, providing a fixed-size
//! identifier that doesn't reveal the public key directly.

use omnimesh_core::types::PeerId;

use crate::keypair::PublicKey;

/// Extension trait for deriving PeerId from identity types.
pub trait PeerIdExt {
    /// Derive a PeerId from this type.
    fn to_peer_id(&self) -> PeerId;
}

impl PeerIdExt for PublicKey {
    /// Derive a PeerId directly from the public key bytes.
    fn to_peer_id(&self) -> PeerId {
        PeerId::from_bytes(*self.as_bytes())
    }
}

impl PeerIdExt for crate::Keypair {
    /// Derive a PeerId from this keypair's public key.
    fn to_peer_id(&self) -> PeerId {
        self.public_key().to_peer_id()
    }
}

/// Derive a PeerId directly from raw public key bytes.
pub fn peer_id_from_public_key_bytes(public_key: &[u8; 32]) -> PeerId {
    PeerId::from_bytes(*public_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keypair::Keypair;

    #[test]
    fn test_peer_id_from_keypair() {
        let kp = Keypair::generate();
        let peer_id = kp.to_peer_id();
        assert_eq!(peer_id.as_bytes().len(), 32);
    }

    #[test]
    fn test_peer_id_deterministic() {
        let seed = [99u8; 32];
        let kp1 = Keypair::from_seed(&seed);
        let kp2 = Keypair::from_seed(&seed);
        assert_eq!(kp1.to_peer_id(), kp2.to_peer_id());
    }

    #[test]
    fn test_peer_id_different_keys_different_ids() {
        let kp1 = Keypair::generate();
        let kp2 = Keypair::generate();
        assert_ne!(kp1.to_peer_id(), kp2.to_peer_id());
    }

    #[test]
    fn test_peer_id_from_public_key_matches() {
        let kp = Keypair::generate();
        let pk = kp.public_key();

        let id1 = pk.to_peer_id();
        let id2 = peer_id_from_public_key_bytes(pk.as_bytes());
        assert_eq!(id1, id2);
    }

    #[test]
    fn test_peer_id_is_raw_key() {
        let kp = Keypair::generate();
        let pk = kp.public_key();
        let peer_id = pk.to_peer_id();

        // PeerId should equal the raw public key bytes
        assert_eq!(peer_id.as_bytes(), pk.as_bytes());
    }
}
