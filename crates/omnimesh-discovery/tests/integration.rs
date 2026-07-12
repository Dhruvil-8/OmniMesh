use std::time::Duration;

use omnimesh_core::config::Config;
use omnimesh_discovery::DiscoveryService;
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_identity::Keypair;

#[tokio::test]
async fn test_mdns_discovery_integration() {
    // Enable logging for debugging
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Generate keypairs and identities
    let kp_alice = Keypair::generate();
    let id_alice = kp_alice.to_peer_id();

    let kp_bob = Keypair::generate();
    let id_bob = kp_bob.to_peer_id();

    // 2. Set up configurations
    let config_alice = Config::default();
    let config_bob = Config::default();

    // 3. Initialize discovery services on ephemeral TCP ports
    // Using port 0 lets OS assign an ephemeral port
    let service_alice = DiscoveryService::new(&config_alice, &kp_alice, 0)
        .await
        .expect("Failed to start Alice's discovery service");

    let service_bob = DiscoveryService::new(&config_bob, &kp_bob, 0)
        .await
        .expect("Failed to start Bob's discovery service");

    // 4. Register local data plane addresses to advertise
    let alice_data_addr = "127.0.0.1:4433".parse().unwrap();
    let bob_data_addr = "127.0.0.1:4434".parse().unwrap();

    service_alice
        .register_local_address(alice_data_addr)
        .await
        .unwrap();
    service_bob
        .register_local_address(bob_data_addr)
        .await
        .unwrap();

    // 5. Wait for mDNS multicast to discover peers and perform Identify exchange (usually takes 1-2 seconds)
    let mut discovered = false;
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(500)).await;

        let alice_bob_addrs = service_alice.lookup(&id_bob);
        let bob_alice_addrs = service_bob.lookup(&id_alice);

        if alice_bob_addrs.contains(&bob_data_addr) && bob_alice_addrs.contains(&alice_data_addr) {
            discovered = true;
            break;
        }
    }

    assert!(
        discovered,
        "Peers failed to discover each other via mDNS within timeout"
    );

    // 6. Verify correct data plane address mapping is cached
    let bob_addrs = service_alice.lookup(&id_bob);
    assert!(bob_addrs.contains(&bob_data_addr));

    let alice_addrs = service_bob.lookup(&id_alice);
    assert!(alice_addrs.contains(&alice_data_addr));
}
