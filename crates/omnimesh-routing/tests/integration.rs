use omnimesh_core::types::PeerId;
use omnimesh_routing::{LinkMetrics, LinkStateUpdate, Router};

fn dummy_peer(val: u8) -> PeerId {
    PeerId::from_bytes([val; 32])
}

#[test]
fn test_topology_routing_convergence() {
    let alice = dummy_peer(1);
    let bob = dummy_peer(2);
    let charlie = dummy_peer(3);
    let dave = dummy_peer(4);

    let router = Router::new(alice.clone());

    // 1. Establish local links from Alice to Bob and Charlie
    router.record_latency(bob.clone(), 10.0);
    router.record_latency(charlie.clone(), 40.0);

    // 2. Process gossip updates representing the rest of the network
    // Link: Bob -> Dave (latency 10ms)
    let g1 = LinkStateUpdate {
        source: bob.clone(),
        destination: dave.clone(),
        metrics: LinkMetrics {
            latency_ms: 10.0,
            bandwidth_kbps: 10000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        },
        sequence: 1,
        timestamp: 100,
    };
    assert!(router.process_link_state_update(g1));

    // Link: Charlie -> Dave (latency 5ms)
    let g2 = LinkStateUpdate {
        source: charlie.clone(),
        destination: dave.clone(),
        metrics: LinkMetrics {
            latency_ms: 5.0,
            bandwidth_kbps: 10000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        },
        sequence: 1,
        timestamp: 100,
    };
    assert!(router.process_link_state_update(g2));

    // 3. Verify Alice initially routes to Dave via Bob (cost: 1.0 + 1.0 = 2.0 vs 1.6 + 0.5 = 2.1)
    let path = router.find_route(&dave).unwrap();
    assert_eq!(path, vec![alice.clone(), bob.clone(), dave.clone()]);
    assert_eq!(router.next_hop(&dave), Some(bob.clone()));

    // 4. Bob -> Dave link degrades (latency spikes to 100ms)
    let g3 = LinkStateUpdate {
        source: bob.clone(),
        destination: dave.clone(),
        metrics: LinkMetrics {
            latency_ms: 100.0, // spike
            bandwidth_kbps: 10000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        },
        sequence: 2, // higher sequence to overwrite previous
        timestamp: 105,
    };
    assert!(router.process_link_state_update(g3));

    // 5. Verify Alice's routing converges dynamically to route via Charlie
    // New cost via Bob: 10 + 100 = 110
    // New cost via Charlie: 40 + 5 = 45
    let path2 = router.find_route(&dave).unwrap();
    assert_eq!(path2, vec![alice, charlie.clone(), dave.clone()]);
    assert_eq!(router.next_hop(&dave), Some(charlie));
}
