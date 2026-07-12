//! Topic-based Publish/Subscribe (PubSub) example using the OmniMesh SDK.
//!
//! Run node 1 (Subscriber):
//!   `cargo run --example p2p_pubsub -- --subscribe`
//!
//! Run node 2 (Publisher):
//!   `cargo run --example p2p_pubsub -- --connect <node_1_address> --publish "My Message"`

use std::net::SocketAddr;
use std::sync::Arc;

use omnimesh_core::config::Config;
use omnimesh_core::telemetry;
use omnimesh_identity::keypair::Keypair;
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_transport::quic::QuicTransport;
use omnimesh_sdk::OmniMeshBuilder;
use omnimesh_service::PubSubService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable simple terminal logs
    telemetry::init("info", false);

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut connect_addr = None;
    let mut is_subscriber = false;
    let mut publish_msg = None;
    let mut listen_port = 0;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--connect" | "-c" => {
                if i + 1 < args.len() {
                    connect_addr = Some(args[i + 1].parse::<SocketAddr>()?);
                    i += 2;
                } else {
                    eprintln!("Error: Missing address after --connect");
                    std::process::exit(1);
                }
            }
            "--subscribe" | "-s" => {
                is_subscriber = true;
                i += 1;
            }
            "--publish" | "-p" => {
                if i + 1 < args.len() {
                    publish_msg = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing message content after --publish");
                    std::process::exit(1);
                }
            }
            "--listen-port" => {
                if i + 1 < args.len() {
                    listen_port = args[i + 1].parse::<u16>()?;
                    i += 2;
                } else {
                    eprintln!("Error: Missing port after --listen-port");
                    std::process::exit(1);
                }
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                eprintln!("Usage: cargo run --example p2p_pubsub -- [--subscribe] [--listen-port PORT] [-c CONNECT_ADDR] [-p MSG]");
                std::process::exit(1);
            }
        }
    }

    if !is_subscriber && publish_msg.is_none() {
        eprintln!("Error: Must either specify --subscribe or --publish <MSG>");
        std::process::exit(1);
    }

    // Configure config overrides
    let mut config = Config::default();
    config.transport.listen_addr = SocketAddr::from(([127, 0, 0, 1], listen_port));
    let listen_addr = config.transport.listen_addr;

    // Generate random keypair for this example session
    let keypair = Keypair::generate();
    let peer_id = keypair.to_peer_id();

    // Create QUIC transport
    let transport = QuicTransport::new();

    // Build mesh node
    let mesh = OmniMeshBuilder::new()
        .config(config)
        .keypair(keypair)
        .transport(transport)
        .build()
        .await?;

    // Instantiate PubSub Service
    let pubsub = Arc::new(PubSubService::new(mesh.clone()));
    mesh.register_service(pubsub.clone());

    // Display identity and listening address
    let actual_addr = mesh.discovery().lookup(&peer_id);
    println!("==================================================");
    println!("OmniMesh P2P PubSub Example Running");
    println!("  PeerID:      {}", peer_id.short());
    println!("  Virtual IP:  {}", mesh.virtual_ip());
    println!("  Listening:   {}", listen_addr);
    if is_subscriber {
        if let Some(addr) = actual_addr.first() {
            println!("\nSubscriber is active on topic 'news'. Run a publisher using:");
            println!("  cargo run --example p2p_pubsub -- --connect {} --publish \"Hello PubSub\"", addr);
        }
    }
    println!("==================================================\n");

    if is_subscriber {
        // Subscribe to topic "news"
        let mut sub_rx = pubsub.subscribe("news");
        println!("✓ Subscribed to topic 'news'. Waiting for publications... Press Ctrl+C to stop.");

        tokio::spawn(async move {
            while let Some(msg_bytes) = sub_rx.recv().await {
                if let Ok(msg_str) = String::from_utf8(msg_bytes) {
                    println!("\n[PubSub Topic: 'news'] Received: '{}'", msg_str);
                }
            }
        });

        tokio::signal::ctrl_c().await?;
    } else if let Some(msg_str) = publish_msg {
        if let Some(addr) = connect_addr {
            println!("Connecting to subscriber at {}...", addr);
            let remote_id = mesh.connect_addr(addr).await?;
            println!("✓ Connected successfully to peer: {}", remote_id.short());

            // Sleep a brief moment to allow session handshake and registration to settle
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            // Publish message to topic "news"
            println!("Publishing '{}' to topic 'news'...", msg_str);
            pubsub.publish("news", msg_str.into_bytes()).await?;
            println!("✓ Publication sent successfully!");

            // Sleep to let package transmit before shutdown
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        } else {
            eprintln!("Error: Publisher must specify --connect <ADDR>");
            std::process::exit(1);
        }
    }

    println!("Shutting down mesh node...");
    mesh.shutdown().await?;

    Ok(())
}
