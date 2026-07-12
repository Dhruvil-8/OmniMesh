//! Conversions between OmniMesh identity types and `rust-libp2p` equivalents.
//!
//! Maps our standard keypairs and PeerIds to the required types in libp2p
//! without losing cryptographic information.

use libp2p::identity::ed25519;
use libp2p::identity::Keypair as Libp2pKeypair;
use libp2p::PeerId as Libp2pPeerId;

use omnimesh_core::error::{OmniMeshError, Result};
use omnimesh_core::types::PeerId;
use omnimesh_identity::Keypair;

/// Convert an OmniMesh `Keypair` to a `libp2p::identity::Keypair`.
pub fn to_libp2p_keypair(keypair: &Keypair) -> Result<Libp2pKeypair> {
    // libp2p's ed25519 decode function expects a mutable 64-byte array
    // containing both the 32-byte secret key and 32-byte public key.
    let seed = keypair.seed();
    let public_bytes = keypair.public_key().as_bytes().to_owned();

    let mut raw_bytes = [0u8; 64];
    raw_bytes[..32].copy_from_slice(&seed);
    raw_bytes[32..].copy_from_slice(&public_bytes);

    let ed_kp = ed25519::Keypair::try_from_bytes(&mut raw_bytes).map_err(|e| {
        OmniMeshError::Identity(format!("failed to decode libp2p ed25519 keypair: {}", e))
    })?;

    Ok(Libp2pKeypair::from(ed_kp))
}

/// Convert an OmniMesh `PeerId` to a `libp2p::PeerId`.
///
/// Since PeerId is defined as the 32-byte Ed25519 public key, we construct
/// the libp2p PeerId as an identity multihash of this public key.
pub fn to_libp2p_peer_id(peer_id: &PeerId) -> Result<Libp2pPeerId> {
    let raw_pk = peer_id.as_bytes();
    let ed_pk = ed25519::PublicKey::try_from_bytes(raw_pk)
        .map_err(|e| OmniMeshError::Identity(format!("invalid ed25519 public key bytes: {}", e)))?;
    let libp2p_pk = libp2p::identity::PublicKey::from(ed_pk);
    Ok(Libp2pPeerId::from(libp2p_pk))
}

/// Convert a `libp2p::PeerId` to an OmniMesh `PeerId`.
pub fn from_libp2p_peer_id(peer_id: Libp2pPeerId) -> Result<PeerId> {
    let multihash = peer_id.as_ref();
    if multihash.code() != 0 {
        return Err(OmniMeshError::Identity(format!(
            "cannot extract public key from non-identity libp2p PeerId (code: {})",
            multihash.code()
        )));
    }

    let libp2p_pk =
        libp2p::identity::PublicKey::try_decode_protobuf(multihash.digest()).map_err(|e| {
            OmniMeshError::Identity(format!(
                "failed to decode libp2p public key protobuf: {}",
                e
            ))
        })?;

    let ed_pk = libp2p_pk
        .try_into_ed25519()
        .map_err(|_| OmniMeshError::Identity("libp2p public key is not Ed25519".into()))?;

    Ok(PeerId::from_bytes(ed_pk.to_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use omnimesh_identity::peer_id::PeerIdExt;

    #[test]
    fn test_key_conversions() {
        let kp = Keypair::generate();
        let peer_id = kp.to_peer_id();

        // Keypair conversion
        let libp2p_kp = to_libp2p_keypair(&kp).unwrap();
        let libp2p_peer_id_from_kp = Libp2pPeerId::from(libp2p_kp.public());

        // PeerId conversion
        let libp2p_peer_id = to_libp2p_peer_id(&peer_id).unwrap();
        assert_eq!(libp2p_peer_id, libp2p_peer_id_from_kp);

        // Roundtrip conversion
        let recovered = from_libp2p_peer_id(libp2p_peer_id).unwrap();
        assert_eq!(recovered, peer_id);
    }
}
