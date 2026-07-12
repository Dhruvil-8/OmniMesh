//! Virtual IPv6 address derivation from PeerId.
//!
//! Maps each PeerId to a deterministic IPv6 address in the `fd00::/8`
//! Unique Local Address (ULA) range. This allows mesh nodes to be
//! addressable via standard IPv6 without external coordination.

use std::net::Ipv6Addr;

use omnimesh_core::types::PeerId;

/// A virtual IPv6 address derived from a PeerId.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VirtualIp {
    addr: Ipv6Addr,
}

/// OmniMesh ULA prefix: `fd4f:4d00::/32`
/// - `fd` = ULA prefix
/// - `4f:4d` = "OM" (OmniMesh) in ASCII hex
const ULA_PREFIX: [u8; 4] = [0xfd, 0x4f, 0x4d, 0x00];

impl VirtualIp {
    /// Derive a virtual IPv6 address from a PeerId.
    ///
    /// The address is constructed as:
    /// - Bytes 0-3: ULA prefix (`fd4f:4d00`)
    /// - Bytes 4-15: First 12 bytes of the PeerId hash
    ///
    /// This gives each peer a unique, deterministic IPv6 address.
    pub fn from_peer_id(peer_id: &PeerId) -> Self {
        let mut octets = [0u8; 16];

        // Set ULA prefix
        octets[0..4].copy_from_slice(&ULA_PREFIX);

        // Fill remaining 12 bytes from PeerId
        octets[4..16].copy_from_slice(&peer_id.as_bytes()[0..12]);

        Self {
            addr: Ipv6Addr::from(octets),
        }
    }

    /// Get the underlying IPv6 address.
    pub fn addr(&self) -> Ipv6Addr {
        self.addr
    }

    /// Check if an IPv6 address belongs to the OmniMesh ULA range.
    pub fn is_omnimesh_address(addr: &Ipv6Addr) -> bool {
        let octets = addr.octets();
        octets[0..4] == ULA_PREFIX
    }
}

impl std::fmt::Display for VirtualIp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.addr)
    }
}

impl From<VirtualIp> for Ipv6Addr {
    fn from(vip: VirtualIp) -> Self {
        vip.addr
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_virtual_ip_from_peer_id() {
        let peer_id = PeerId::from_bytes([0xAA; 32]);
        let vip = VirtualIp::from_peer_id(&peer_id);
        let octets = vip.addr().octets();

        // Check ULA prefix
        assert_eq!(octets[0], 0xfd);
        assert_eq!(octets[1], 0x4f);
        assert_eq!(octets[2], 0x4d);
        assert_eq!(octets[3], 0x00);

        // Check PeerId bytes are used
        assert_eq!(octets[4], 0xAA);
    }

    #[test]
    fn test_virtual_ip_deterministic() {
        let peer_id = PeerId::from_bytes([42u8; 32]);
        let vip1 = VirtualIp::from_peer_id(&peer_id);
        let vip2 = VirtualIp::from_peer_id(&peer_id);
        assert_eq!(vip1, vip2);
    }

    #[test]
    fn test_virtual_ip_unique_per_peer() {
        let pid1 = PeerId::from_bytes([1u8; 32]);
        let pid2 = PeerId::from_bytes([2u8; 32]);
        let vip1 = VirtualIp::from_peer_id(&pid1);
        let vip2 = VirtualIp::from_peer_id(&pid2);
        assert_ne!(vip1, vip2);
    }

    #[test]
    fn test_is_omnimesh_address() {
        let peer_id = PeerId::from_bytes([0xFF; 32]);
        let vip = VirtualIp::from_peer_id(&peer_id);
        assert!(VirtualIp::is_omnimesh_address(&vip.addr()));

        let non_mesh: Ipv6Addr = "2001:db8::1".parse().unwrap();
        assert!(!VirtualIp::is_omnimesh_address(&non_mesh));
    }

    #[test]
    fn test_virtual_ip_display() {
        let peer_id = PeerId::from_bytes([0u8; 32]);
        let vip = VirtualIp::from_peer_id(&peer_id);
        let display = vip.to_string();
        assert!(display.starts_with("fd4f:4d00:"));
    }
}
