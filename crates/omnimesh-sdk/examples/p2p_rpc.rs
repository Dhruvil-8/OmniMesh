//! Remote Procedure Call (RPC) example using the OmniMesh SDK.
//!
//! Run node 1 (RPC Server):
//!   `cargo run --example p2p_rpc -- --server`
//!
//! Run node 2 (RPC Client):
//!   `cargo run --example p2p_rpc -- --connect <node_1_address>`

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;

use omnimesh_core::config::Config;
use omnimesh_core::telemetry;
use omnimesh_core::types::PeerId;
use omnimesh_identity::keypair::Keypair;
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_transport::quic::QuicTransport;
use omnimesh_sdk::OmniMeshBuilder;
use omnimesh_service::{RpcHandler, RpcService};

struct StringReverseHandler;

#[async_trait]
impl RpcHandler for StringReverseHandler {
    async fn handle_request(&self, _from: PeerId, method: &str, request: Vec<u8>) -> omnimesh_core::error::Result<Vec<u8>> {
        if method == "reverse" {
            let input = String::from_utf8(request).unwrap_or_default();
            let reversed: String = input.chars().rev().collect();
            println!("RPC Handler: reversed '{}' to '{}'", input, reversed);
            Ok(reversed.into_bytes())
        } else {
            Ok(b"Unknown method".to_vec())
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable simple terminal logs
    telemetry::init("info", false);

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut connect_addr = None;
    let mut is_server = false;
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
            "--server" | "-s" => {
                is_server = true;
                i += 1;
            }
            "--listen-port" | "-p" => {
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
                eprintln!("Usage: cargo run --example p2p_rpc -- [--server] [-p PORT] [-c CONNECT_ADDR]");
                std::process::exit(1);
            }
        }
    }

    if !is_server && connect_addr.is_none() {
        eprintln!("Error: Must either specify --server or --connect <ADDR>");
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

    // Instantiate RPC Service
    let rpc = Arc::new(RpcService::new(mesh.clone(), Arc::new(StringReverseHandler)));
    mesh.register_service(rpc.clone());

    // Display identity and listening address
    let actual_addr = mesh.discovery().lookup(&peer_id);
    println!("==================================================");
    println!("OmniMesh P2P RPC Example Running");
    println!("  PeerID:      {}", peer_id.short());
    println!("  Virtual IP:  {}", mesh.virtual_ip());
    println!("  Listening:   {}", listen_addr);
    if is_server {
        if let Some(addr) = actual_addr.first() {
            println!("\nServer is running. Connect a client using:");
            println!("  cargo run --example p2p_rpc -- --connect {}", addr);
        }
    }
    println!("==================================================\n");

    if is_server {
        println!("Server running. Press Ctrl+C to stop.");
        tokio::signal::ctrl_c().await?;
    } else if let Some(addr) = connect_addr {
        println!("Connecting to RPC Server at {}...", addr);
        let remote_id = mesh.connect_addr(addr).await?;
        println!("✓ Connected successfully to peer: {}", remote_id.short());

        // Call RPC method
        let payload = b"Hello from OmniMesh Client!".to_vec();
        println!("Sending RPC Request: 'Hello from OmniMesh Client!'");
        match rpc.call(remote_id, "reverse", payload).await {
            Ok(response_bytes) => {
                let response_str = String::from_utf8(response_bytes)?;
                println!("✓ RPC Response Received: '{}'", response_str);
            }
            Err(e) => {
                eprintln!("✗ RPC Call Failed: {:?}", e);
            }
        }

        // Wait a brief moment to settle down
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }

    println!("Shutting down mesh node...");
    mesh.shutdown().await?;

    Ok(())
}
