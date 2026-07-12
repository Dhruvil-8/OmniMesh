//! Thread-safe peer address registry.
//!
//! Tracks known socket addresses of remote peers discovered by Kademlia,
//! mDNS, or manual bootstrapper definitions.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use dashmap::DashMap;

use omnimesh_core::types::PeerId;

/// Default address expiration duration (e.g., 30 minutes).
const DEFAULT_TTL: Duration = Duration::from_secs(1800);

/// Metadata associated with a peer's cached address.
#[derive(Debug, Clone)]
struct CachedAddress {
    addr: SocketAddr,
    expires_at: Instant,
}

/// Thread-safe local cache mapping PeerIds to active network endpoints.
#[derive(Clone, Default)]
pub struct PeerAddressCache {
    /// Thread-safe inner map tracking peers and their associated address lists.
    inner: Arc<DashMap<PeerId, Vec<CachedAddress>>>,
}

impl PeerAddressCache {
    /// Create a new, empty cache.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DashMap::new()),
        }
    }

    /// Add a discovered address for a peer with default TTL.
    pub fn insert(&self, peer_id: PeerId, addr: SocketAddr) {
        self.insert_with_ttl(peer_id, addr, DEFAULT_TTL);
    }

    pub fn insert_with_ttl(&self, peer_id: PeerId, addr: SocketAddr, ttl: Duration) {
        let expires_at = Instant::now() + ttl;
        let entry = CachedAddress { addr, expires_at };

        let mut list = self.inner.entry(peer_id).or_default();
        list.retain(|x| x.addr != addr);
        list.push(entry);
    }

    /// Add multiple addresses for a peer.
    pub fn insert_many(&self, peer_id: PeerId, addrs: impl IntoIterator<Item = SocketAddr>) {
        for addr in addrs {
            self.insert(peer_id.clone(), addr);
        }
    }

    /// Get all active (non-expired) addresses for a peer.
    pub fn lookup(&self, peer_id: &PeerId) -> Vec<SocketAddr> {
        let now = Instant::now();
        if let Some(entry) = self.inner.get(peer_id) {
            entry
                .value()
                .iter()
                .filter(|x| x.expires_at > now)
                .map(|x| x.addr)
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Remove a peer's cached records.
    pub fn remove(&self, peer_id: &PeerId) {
        self.inner.remove(peer_id);
    }

    /// Retain only active addresses and remove expired keys.
    pub fn prune(&self) {
        let now = Instant::now();
        self.inner.retain(|_, list| {
            list.retain(|x| x.expires_at > now);
            !list.is_empty()
        });
    }

    /// List all discovered peer identities currently present in the cache.
    pub fn all_peers(&self) -> Vec<(PeerId, Vec<SocketAddr>)> {
        let now = Instant::now();
        self.inner
            .iter()
            .map(|entry| {
                let peer_id = entry.key().clone();
                let addrs: Vec<SocketAddr> = entry
                    .value()
                    .iter()
                    .filter(|x| x.expires_at > now)
                    .map(|x| x.addr)
                    .collect();
                (peer_id, addrs)
            })
            .filter(|(_, addrs)| !addrs.is_empty())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_expiration() {
        let cache = PeerAddressCache::new();
        let peer_id = PeerId::from_bytes([7; 32]);
        let addr = "127.0.0.1:1234".parse().unwrap();

        // Expired entry
        cache.insert_with_ttl(peer_id.clone(), addr, Duration::from_millis(0));
        assert!(cache.lookup(&peer_id).is_empty());

        // Valid entry
        cache.insert_with_ttl(peer_id.clone(), addr, Duration::from_secs(10));
        assert_eq!(cache.lookup(&peer_id), vec![addr]);
    }

    #[test]
    fn test_cache_deduplication() {
        let cache = PeerAddressCache::new();
        let peer_id = PeerId::from_bytes([9; 32]);
        let addr = "127.0.0.1:1234".parse().unwrap();

        cache.insert(peer_id.clone(), addr);
        cache.insert(peer_id.clone(), addr);

        assert_eq!(cache.lookup(&peer_id).len(), 1);
    }
}
