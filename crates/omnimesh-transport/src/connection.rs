//! Connection pool for managing and reusing transport connections.
//!
//! Tracks active connections by PeerId, limits concurrent connections,
//! and provides lookup/eviction functionality.

use std::net::SocketAddr;

use dashmap::DashMap;
use tracing::debug;

use omnimesh_core::error::{OmniMeshError, Result};
use omnimesh_core::types::{ConnectionState, PeerId};

/// Metadata about a tracked connection.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// The remote peer's ID.
    pub peer_id: PeerId,
    /// The remote address.
    pub remote_addr: SocketAddr,
    /// Current connection state.
    pub state: ConnectionState,
    /// Unix timestamp of when this connection was established.
    pub connected_at: u64,
    /// Number of bytes sent through this connection.
    pub bytes_sent: u64,
    /// Number of bytes received through this connection.
    pub bytes_received: u64,
}

/// A pool that tracks active connections by PeerId.
///
/// Thread-safe via `DashMap` — multiple tasks can read/write concurrently.
pub struct ConnectionPool {
    /// Active connections indexed by PeerId.
    connections: DashMap<PeerId, ConnectionInfo>,
    /// Maximum number of concurrent connections.
    max_connections: usize,
}

impl ConnectionPool {
    /// Create a new connection pool with the given capacity limit.
    pub fn new(max_connections: usize) -> Self {
        Self {
            connections: DashMap::new(),
            max_connections,
        }
    }

    /// Register a new connection.
    ///
    /// Returns an error if the pool is at capacity.
    pub fn insert(&self, info: ConnectionInfo) -> Result<()> {
        if self.connections.len() >= self.max_connections {
            return Err(OmniMeshError::Connection(format!(
                "connection pool full ({}/{})",
                self.connections.len(),
                self.max_connections
            )));
        }
        let peer_id = info.peer_id.clone();
        self.connections.insert(peer_id.clone(), info);
        debug!(peer = %peer_id.short(), "connection added to pool");
        Ok(())
    }

    /// Remove a connection by PeerId.
    pub fn remove(&self, peer_id: &PeerId) -> Option<ConnectionInfo> {
        let removed = self.connections.remove(peer_id).map(|(_, v)| v);
        if removed.is_some() {
            debug!(peer = %peer_id.short(), "connection removed from pool");
        }
        removed
    }

    /// Get connection info for a peer (read-only).
    pub fn get(&self, peer_id: &PeerId) -> Option<ConnectionInfo> {
        self.connections.get(peer_id).map(|entry| entry.clone())
    }

    /// Check if a peer is connected.
    pub fn is_connected(&self, peer_id: &PeerId) -> bool {
        self.connections
            .get(peer_id)
            .map(|entry| entry.state == ConnectionState::Connected)
            .unwrap_or(false)
    }

    /// Get the number of active connections.
    pub fn len(&self) -> usize {
        self.connections.len()
    }

    /// Check if the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.connections.is_empty()
    }

    /// Get all connected peer IDs.
    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.connections
            .iter()
            .filter(|entry| entry.state == ConnectionState::Connected)
            .map(|entry| entry.key().clone())
            .collect()
    }

    /// Update connection state for a peer.
    pub fn update_state(&self, peer_id: &PeerId, state: ConnectionState) {
        if let Some(mut entry) = self.connections.get_mut(peer_id) {
            entry.state = state;
        }
    }

    /// Record bytes sent on a connection.
    pub fn record_bytes_sent(&self, peer_id: &PeerId, bytes: u64) {
        if let Some(mut entry) = self.connections.get_mut(peer_id) {
            entry.bytes_sent += bytes;
        }
    }

    /// Record bytes received on a connection.
    pub fn record_bytes_received(&self, peer_id: &PeerId, bytes: u64) {
        if let Some(mut entry) = self.connections.get_mut(peer_id) {
            entry.bytes_received += bytes;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_info(id: u8) -> ConnectionInfo {
        ConnectionInfo {
            peer_id: PeerId::from_bytes([id; 32]),
            remote_addr: format!("127.0.0.1:{}", 1000 + id as u16).parse().unwrap(),
            state: ConnectionState::Connected,
            connected_at: 0,
            bytes_sent: 0,
            bytes_received: 0,
        }
    }

    #[test]
    fn test_insert_and_get() {
        let pool = ConnectionPool::new(10);
        let info = make_info(1);
        pool.insert(info.clone()).unwrap();

        assert_eq!(pool.len(), 1);
        assert!(pool.is_connected(&info.peer_id));
    }

    #[test]
    fn test_remove() {
        let pool = ConnectionPool::new(10);
        let info = make_info(2);
        pool.insert(info.clone()).unwrap();
        pool.remove(&info.peer_id);
        assert!(pool.is_empty());
    }

    #[test]
    fn test_capacity_limit() {
        let pool = ConnectionPool::new(2);
        pool.insert(make_info(1)).unwrap();
        pool.insert(make_info(2)).unwrap();
        assert!(pool.insert(make_info(3)).is_err());
    }

    #[test]
    fn test_connected_peers() {
        let pool = ConnectionPool::new(10);
        pool.insert(make_info(1)).unwrap();
        pool.insert(make_info(2)).unwrap();

        let peers = pool.connected_peers();
        assert_eq!(peers.len(), 2);
    }

    #[test]
    fn test_update_state() {
        let pool = ConnectionPool::new(10);
        let info = make_info(5);
        pool.insert(info.clone()).unwrap();

        pool.update_state(&info.peer_id, ConnectionState::Disconnected);
        assert!(!pool.is_connected(&info.peer_id));
    }

    #[test]
    fn test_bytes_tracking() {
        let pool = ConnectionPool::new(10);
        let info = make_info(3);
        pool.insert(info.clone()).unwrap();

        pool.record_bytes_sent(&info.peer_id, 100);
        pool.record_bytes_received(&info.peer_id, 200);

        let retrieved = pool.get(&info.peer_id).unwrap();
        assert_eq!(retrieved.bytes_sent, 100);
        assert_eq!(retrieved.bytes_received, 200);
    }
}
