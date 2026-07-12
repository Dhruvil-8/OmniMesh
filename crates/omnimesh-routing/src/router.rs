//! Router coordinator mapping network topology, telemetry, and gossip state.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use omnimesh_core::types::PeerId;

use crate::graph::RouteGraph;
use crate::metrics::{LinkMetricTracker, LinkMetrics};

/// Gossip packet representing link metrics between two peers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkStateUpdate {
    /// Peer that measured the link.
    pub source: PeerId,
    /// Remote destination peer of the link.
    pub destination: PeerId,
    /// Measured link metrics.
    pub metrics: LinkMetrics,
    /// Incremental sequence counter to prevent out-of-order/old updates.
    pub sequence: u64,
    /// Time when the measurement was taken (seconds since UNIX epoch).
    pub timestamp: u64,
}

/// Dynamic routing coordinator.
pub struct Router {
    /// Local peer identifier.
    local_peer_id: PeerId,
    /// Thread-safe map tracking smoothed metrics for local direct neighbours.
    local_trackers: DashMap<PeerId, LinkMetricTracker>,
    /// Global routing graph representing the network topology.
    graph: parking_lot::RwLock<RouteGraph>,
    /// Tracked gossip sequence numbers per peer to drop redundant updates.
    gossip_sequences: DashMap<(PeerId, PeerId), u64>,
    /// Local monotonic sequence generator for outgoing gossip packets.
    sequence_generator: AtomicU64,
}

impl Router {
    /// Create a new router instance.
    pub fn new(local_peer_id: PeerId) -> Self {
        Self {
            local_peer_id,
            local_trackers: DashMap::new(),
            graph: parking_lot::RwLock::new(RouteGraph::new()),
            gossip_sequences: DashMap::new(),
            sequence_generator: AtomicU64::new(1),
        }
    }

    /// Record a latency measurement for a direct neighbour.
    pub fn record_latency(&self, peer: PeerId, latency_ms: f64) {
        self.local_trackers
            .entry(peer.clone())
            .or_insert_with(|| LinkMetricTracker::new(LinkMetrics::default()))
            .value_mut()
            .record_latency(latency_ms);

        self.update_graph_link(&peer);
    }

    /// Record a bandwidth measurement for a direct neighbour.
    pub fn record_bandwidth(&self, peer: PeerId, bandwidth_kbps: f64) {
        self.local_trackers
            .entry(peer.clone())
            .or_insert_with(|| LinkMetricTracker::new(LinkMetrics::default()))
            .value_mut()
            .record_bandwidth(bandwidth_kbps);

        self.update_graph_link(&peer);
    }

    /// Record a packet loss rate measurement for a direct neighbour.
    pub fn record_loss_rate(&self, peer: PeerId, loss_rate: f64) {
        self.local_trackers
            .entry(peer.clone())
            .or_insert_with(|| LinkMetricTracker::new(LinkMetrics::default()))
            .value_mut()
            .record_loss_rate(loss_rate);

        self.update_graph_link(&peer);
    }

    /// Update the local route graph topology with our current tracked link metrics.
    fn update_graph_link(&self, peer: &PeerId) {
        if let Some(tracker) = self.local_trackers.get(peer) {
            let metrics = tracker.metrics();
            let mut g = self.graph.write();
            g.add_link(self.local_peer_id.clone(), peer.clone(), metrics);
        }
    }

    /// Generate an outgoing `LinkStateUpdate` to gossip to the network.
    pub fn get_link_state_update(&self, peer: &PeerId) -> Option<LinkStateUpdate> {
        let tracker = self.local_trackers.get(peer)?;
        let metrics = tracker.metrics();
        let sequence = self.sequence_generator.fetch_add(1, Ordering::SeqCst);
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Some(LinkStateUpdate {
            source: self.local_peer_id.clone(),
            destination: peer.clone(),
            metrics,
            sequence,
            timestamp,
        })
    }

    /// Process an incoming gossip topology update.
    ///
    /// Returns `true` if the update is new and was successfully applied (indicating
    /// the caller should gossip it further), or `false` if it was dropped as redundant/old.
    pub fn process_link_state_update(&self, update: LinkStateUpdate) -> bool {
        let key = (update.source.clone(), update.destination.clone());

        // Check sequence to discard old packets
        let mut current_seq = self.gossip_sequences.entry(key.clone()).or_insert(0);
        if update.sequence <= *current_seq.value() {
            return false;
        }

        // Update stored sequence number
        *current_seq.value_mut() = update.sequence;

        // Apply edge to the local graph representation
        let mut g = self.graph.write();
        g.add_link(update.source, update.destination, update.metrics);
        true
    }

    /// Calculate the full dynamic multi-hop route to a target node.
    ///
    /// Returns the sequence of node PeerIds including both start and target nodes.
    pub fn find_route(&self, target: &PeerId) -> Option<Vec<PeerId>> {
        let g = self.graph.read();
        g.find_shortest_path(&self.local_peer_id, target)
    }

    /// Determine the first-hop PeerId required to route packets towards a target node.
    pub fn next_hop(&self, target: &PeerId) -> Option<PeerId> {
        let route = self.find_route(target)?;
        // If the path length is <= 1, target is self or empty
        if route.len() > 1 {
            Some(route[1].clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_peer(val: u8) -> PeerId {
        PeerId::from_bytes([val; 32])
    }

    #[test]
    fn test_router_local_link_state() {
        let alice = dummy_peer(1);
        let bob = dummy_peer(2);
        let router = Router::new(alice.clone());

        // Alice records metrics against Bob
        router.record_latency(bob.clone(), 15.0);
        router.record_bandwidth(bob.clone(), 5000.0);

        let route = router.find_route(&bob).unwrap();
        assert_eq!(route, vec![alice.clone(), bob.clone()]);

        let update = router.get_link_state_update(&bob).unwrap();
        assert_eq!(update.source, alice);
        assert_eq!(update.destination, bob);
        assert_eq!(update.metrics.latency_ms, 11.0);
        assert_eq!(update.sequence, 1);
    }

    #[test]
    fn test_router_process_gossip_sequence() {
        let alice = dummy_peer(1);
        let bob = dummy_peer(2);
        let charlie = dummy_peer(3);

        let router = Router::new(alice.clone());

        // Gossip packet: Bob -> Charlie (seq 5)
        let u1 = LinkStateUpdate {
            source: bob.clone(),
            destination: charlie.clone(),
            metrics: LinkMetrics::default(),
            sequence: 5,
            timestamp: 100,
        };

        // First time should succeed
        assert!(router.process_link_state_update(u1.clone()));

        // Old seq (3) should be rejected
        let mut u2 = u1.clone();
        u2.sequence = 3;
        assert!(!router.process_link_state_update(u2));

        // Same seq (5) should be rejected
        assert!(!router.process_link_state_update(u1.clone()));

        // Newer seq (6) should succeed
        let mut u3 = u1.clone();
        u3.sequence = 6;
        assert!(router.process_link_state_update(u3));
    }
}
