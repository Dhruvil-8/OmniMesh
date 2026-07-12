//! Route graph maintainer and Dijkstra shortest-path route computation engine.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap};

use omnimesh_core::types::PeerId;

use crate::metrics::LinkMetrics;

/// Dijkstra search state wrapper.
#[derive(Clone, PartialEq)]
struct SearchState {
    cost: f64,
    node: PeerId,
}

impl Eq for SearchState {}

impl Ord for SearchState {
    fn cmp(&self, other: &Self) -> Ordering {
        // Reverse standard ordering to turn BinaryHeap into a min-heap
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}

impl PartialOrd for SearchState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Dynamic weighted directed topology graph representing mesh connections.
#[derive(Debug, Clone, Default)]
pub struct RouteGraph {
    /// Adjacency map: source -> (destination -> link metrics).
    edges: HashMap<PeerId, HashMap<PeerId, LinkMetrics>>,
}

impl RouteGraph {
    /// Create a new, empty route graph.
    pub fn new() -> Self {
        Self {
            edges: HashMap::new(),
        }
    }

    /// Add or update a directed link in the graph.
    pub fn add_link(&mut self, from: PeerId, to: PeerId, metrics: LinkMetrics) {
        self.edges.entry(from).or_default().insert(to, metrics);
    }

    /// Remove a directed link from the graph.
    pub fn remove_link(&mut self, from: &PeerId, to: &PeerId) {
        if let Some(neighbours) = self.edges.get_mut(from) {
            neighbours.remove(to);
            if neighbours.is_empty() {
                self.edges.remove(from);
            }
        }
    }

    /// Retrieve the link metrics for a specific directed link, if it exists.
    pub fn get_link(&self, from: &PeerId, to: &PeerId) -> Option<&LinkMetrics> {
        self.edges.get(from)?.get(to)
    }

    /// Find the lowest-cost path between two PeerIds using Dijkstra's algorithm.
    ///
    /// Returns the sequence of node PeerIds including both start and target nodes.
    /// Returns `None` if no path is reachable.
    pub fn find_shortest_path(&self, start: &PeerId, target: &PeerId) -> Option<Vec<PeerId>> {
        let mut distances: HashMap<PeerId, f64> = HashMap::new();
        let mut predecessors: HashMap<PeerId, PeerId> = HashMap::new();
        let mut heap = BinaryHeap::new();

        // Seed search
        distances.insert(start.clone(), 0.0);
        heap.push(SearchState {
            cost: 0.0,
            node: start.clone(),
        });

        while let Some(SearchState { cost, node }) = heap.pop() {
            // Target node reached
            if &node == target {
                let mut path = Vec::new();
                let mut current = node;
                path.push(current.clone());

                while let Some(pred) = predecessors.get(&current) {
                    current = pred.clone();
                    path.push(current.clone());
                }
                path.reverse();
                return Some(path);
            }

            // Skip if a cheaper path to this node was already processed
            if let Some(&best_dist) = distances.get(&node) {
                if cost > best_dist {
                    continue;
                }
            }

            // Process neighbours
            if let Some(neighbours) = self.edges.get(&node) {
                for (neighbour, metrics) in neighbours {
                    let next_cost = cost + metrics.cost();

                    // If a cheaper route is found, record it and update search heap
                    let current_best = distances.get(neighbour).copied().unwrap_or(f64::MAX);
                    if next_cost < current_best {
                        distances.insert(neighbour.clone(), next_cost);
                        predecessors.insert(neighbour.clone(), node.clone());
                        heap.push(SearchState {
                            cost: next_cost,
                            node: neighbour.clone(),
                        });
                    }
                }
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_peer(val: u8) -> PeerId {
        PeerId::from_bytes([val; 32])
    }

    #[test]
    fn test_shortest_path_simple() {
        let mut graph = RouteGraph::new();
        let a = dummy_peer(1);
        let b = dummy_peer(2);
        let c = dummy_peer(3);

        let m1 = LinkMetrics {
            latency_ms: 10.0,
            bandwidth_kbps: 1000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        }; // cost = 10 * 1 / 1 = 10.0

        let m2 = LinkMetrics {
            latency_ms: 5.0,
            bandwidth_kbps: 1000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        }; // cost = 5.0

        graph.add_link(a.clone(), b.clone(), m1);
        graph.add_link(b.clone(), c.clone(), m2);

        let path = graph.find_shortest_path(&a, &c).unwrap();
        assert_eq!(path, vec![a, b, c]);
    }

    #[test]
    fn test_shortest_path_selects_cheapest() {
        let mut graph = RouteGraph::new();
        let a = dummy_peer(1);
        let b = dummy_peer(2);
        let c = dummy_peer(3);
        let d = dummy_peer(4);

        // Path A -> B -> D (high latency)
        // Path A -> C -> D (low latency)
        let slow = LinkMetrics {
            latency_ms: 50.0,
            bandwidth_kbps: 1000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        }; // cost = 50.0
        let fast = LinkMetrics {
            latency_ms: 5.0,
            bandwidth_kbps: 1000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        }; // cost = 5.0

        // A -> B (50), B -> D (50) = 100
        graph.add_link(a.clone(), b.clone(), slow);
        graph.add_link(b.clone(), d.clone(), slow);

        // A -> C (5), C -> D (5) = 10
        graph.add_link(a.clone(), c.clone(), fast);
        graph.add_link(c.clone(), d.clone(), fast);

        let path = graph.find_shortest_path(&a, &d).unwrap();
        assert_eq!(path, vec![a, c, d]);
    }

    #[test]
    fn test_unreachable_node_returns_none() {
        let mut graph = RouteGraph::new();
        let a = dummy_peer(1);
        let b = dummy_peer(2);
        let c = dummy_peer(3);

        graph.add_link(
            a.clone(),
            b.clone(),
            LinkMetrics {
                latency_ms: 10.0,
                bandwidth_kbps: 1000.0,
                loss_rate: 0.0,
                relay_cost: 0.0,
            },
        );

        // c is completely isolated
        let path = graph.find_shortest_path(&a, &c);
        assert!(path.is_none());
    }
}
