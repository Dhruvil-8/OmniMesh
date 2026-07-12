//! Performance benchmarks for OmniMesh core, cryptography, and routing layers.

use std::net::SocketAddr;
use std::time::Instant;

use bytes::Bytes;
use omnimesh_core::types::PeerId;
use omnimesh_identity::keypair::Keypair;
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_crypto::noise::{NoiseHandshake, Role};
use omnimesh_routing::graph::RouteGraph;
use omnimesh_routing::metrics::LinkMetrics;
use omnimesh_transport::mock::MockTransport;
use omnimesh_transport::traits::{Connection, Transport};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("==================================================");
    println!("             OmniMesh Benchmarks Suite             ");
    println!("==================================================");

    benchmark_noise_handshake()?;
    benchmark_crypto_throughput()?;
    benchmark_routing_dijkstra();

    println!("==================================================");
    Ok(())
}

/// Benchmark the handshakes/sec rate of Noise_XX.
fn benchmark_noise_handshake() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n1. Handshake Performance (Noise_XX)");

    let count = 200;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    runtime.block_on(async {
        let (_registry, mut transport_alice, mut transport_bob) = MockTransport::create_pair();

        let addr = "127.0.0.1:9090".parse::<SocketAddr>()?;
        transport_bob.listen(addr).await?;

        let start = Instant::now();
        for _ in 0..count {
            let conn_alice_fut = transport_alice.connect(addr);
            let conn_bob_fut = transport_bob.accept();

            let (conn_alice_res, conn_bob_res) = tokio::join!(conn_alice_fut, conn_bob_fut);
            let mut conn_alice = conn_alice_res?;
            let mut conn_bob = conn_bob_res?;

            // Setup handshake states
            let kp_alice = Keypair::generate();
            let kp_bob = Keypair::generate();

            let mut hs_alice = NoiseHandshake::with_keypair(Role::Initiator, &kp_alice.seed())?;
            let mut hs_bob = NoiseHandshake::with_keypair(Role::Responder, &kp_bob.seed())?;

            // Handshake Message 1 (Alice -> Bob)
            let msg1 = hs_alice.write_message(b"Alice info")?;
            conn_alice.send(Bytes::from(msg1)).await?;

            let rec1 = conn_bob.recv().await?.unwrap();
            let _ = hs_bob.read_message(&rec1)?;

            // Handshake Message 2 (Bob -> Alice)
            let msg2 = hs_bob.write_message(b"Bob info")?;
            conn_bob.send(Bytes::from(msg2)).await?;

            let rec2 = conn_alice.recv().await?.unwrap();
            let _ = hs_alice.read_message(&rec2)?;

            // Handshake Message 3 (Alice -> Bob)
            let msg3 = hs_alice.write_message(b"Alice finalize")?;
            conn_alice.send(Bytes::from(msg3)).await?;

            let rec3 = conn_bob.recv().await?.unwrap();
            let _ = hs_bob.read_message(&rec3)?;

            assert!(hs_alice.is_finished());
            assert!(hs_bob.is_finished());
        }
        let duration = start.elapsed();
        let rate = (count as f64) / duration.as_secs_f64();
        println!("  Executed {} full handshakes in {:?}", count, duration);
        println!("  Throughput: {:.2} handshakes/sec", rate);
        Ok::<(), Box<dyn std::error::Error>>(())
    })?;

    Ok(())
}

fn benchmark_crypto_throughput() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n2. Symmetric Crypto Throughput (ChaCha20-Poly1305)");

    // Alice / Bob key setup
    let kp_alice = Keypair::generate();
    let kp_bob = Keypair::generate();

    let mut hs_alice = NoiseHandshake::with_keypair(Role::Initiator, &kp_alice.seed())?;
    let mut hs_bob = NoiseHandshake::with_keypair(Role::Responder, &kp_bob.seed())?;

    // Perform minimal in-memory handshake exchange
    let msg1 = hs_alice.write_message(&[])?;
    let _ = hs_bob.read_message(&msg1)?;
    let msg2 = hs_bob.write_message(&[])?;
    let _ = hs_alice.read_message(&msg2)?;
    let msg3 = hs_alice.write_message(&[])?;
    let _ = hs_bob.read_message(&msg3)?;

    let mut alice_transport = hs_alice.into_transport()?;
    let mut bob_transport = hs_bob.into_transport()?;

    // 60 KB payload to fit within standard Noise max frame size (65,535 bytes)
    let payload_size = 60 * 1024;
    let payload = vec![0u8; payload_size];
    let count = 8000; // ~480 MB total

    let start = Instant::now();
    for _ in 0..count {
        let ciphertext = alice_transport.encrypt(&payload)?;
        let decrypted = bob_transport.decrypt(&ciphertext)?;
        assert_eq!(decrypted.len(), payload_size);
    }
    let duration = start.elapsed();
    let total_mb = (count * payload_size) as f64 / (1024.0 * 1024.0);
    let rate = total_mb / duration.as_secs_f64();

    println!("  Processed {:.2} MB of data in {:?}", total_mb, duration);
    println!("  Throughput: {:.2} MB/sec", rate);

    Ok(())
}

/// Benchmark Dijkstra routing pathfinding computations on a 100-node topology.
fn benchmark_routing_dijkstra() {
    println!("\n3. Dijkstra Pathfinding computations (100-node graph)");

    let node_count = 100;
    let mut graph = RouteGraph::new();
    let peers: Vec<PeerId> = (0..node_count)
        .map(|_| Keypair::generate().to_peer_id())
        .collect();

    // Create a mesh-like topology where each node has 4 edges
    for i in 0..node_count {
        for offset in [1, 2, 5, 10] {
            let neighbor_idx = (i + offset) % node_count;
            let metrics = LinkMetrics {
                latency_ms: 10.0 + (i % 5) as f64,
                loss_rate: 0.01,
                bandwidth_kbps: 100000.0, // 100 Mbps
                relay_cost: 0.1,
            };
            graph.add_link(peers[i].clone(), peers[neighbor_idx].clone(), metrics);
        }
    }

    let iterations = 2000;
    let start = Instant::now();
    for i in 0..iterations {
        let src = &peers[i % node_count];
        let dest = &peers[(i + 50) % node_count];
        let _path = graph.find_shortest_path(src, dest);
    }
    let duration = start.elapsed();
    let rate = (iterations as f64) / duration.as_secs_f64();

    println!(
        "  Computed {} shortest-path queries in {:?}",
        iterations, duration
    );
    println!("  Throughput: {:.2} path queries/sec", rate);
}
