//! Interactive peer-to-peer console chat example using the OmniMesh SDK.
//!
//! Run node 1:
//!   `cargo run --example p2p_chat`
//!
//! Run node 2:
//!   `cargo run --example p2p_chat -- --connect <node_1_address>`

use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

use omnimesh_core::config::Config;
use omnimesh_core::telemetry;
use omnimesh_identity::keypair::Keypair;
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_transport::quic::QuicTransport;
use omnimesh_sdk::OmniMeshBuilder;
use omnimesh_service::ChatService;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Enable simple terminal logs
    telemetry::init("info", false);

    // Parse CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut connect_addr = None;
    let mut listen_port = 0; // Random ephemeral port by default

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
                eprintln!("Usage: cargo run --example p2p_chat -- [-p PORT] [-c CONNECT_ADDR]");
                std::process::exit(1);
            }
        }
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

    // Instantiate Chat Service
    let chat = Arc::new(ChatService::new(mesh.clone()));
    mesh.register_service(chat.clone());

    // Display identity and connection commands
    let actual_addr = mesh.discovery().lookup(&peer_id);
    println!("==================================================");
    println!("OmniMesh P2P Chat Example Running");
    println!("  PeerID:      {}", peer_id.short());
    println!("  Virtual IP:  {}", mesh.virtual_ip());
    println!("  Listening:   {}", listen_addr);
    if let Some(addr) = actual_addr.first() {
        println!("\nTo connect another peer, run:");
        println!("  cargo run --example p2p_chat -- --connect {}", addr);
    }
    println!("==================================================\n");

    // Subscribe and print incoming chat messages in background
    let mut chat_rx = chat.subscribe();
    tokio::spawn(async move {
        while let Ok((from, text)) = chat_rx.recv().await {
            println!("\n[{}] Chat: {}", from.short(), text);
            print!("> ");
            use std::io::Write;
            std::io::stdout().flush().unwrap();
        }
    });

    // Dial remote peer if connect address was provided
    if let Some(addr) = connect_addr {
        println!("Connecting to {}...", addr);
        match mesh.connect_addr(addr).await {
            Ok(remote_id) => {
                println!("✓ Connected successfully to peer: {}", remote_id.short());
            }
            Err(e) => {
                eprintln!("✗ Failed to connect: {:?}", e);
            }
        }
    }

    println!("Type messages and press Enter to send (type 'exit' to quit):\n");
    print!("> ");
    use std::io::Write;
    std::io::stdout().flush().unwrap();

    let mut stdin_reader = tokio::io::BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = stdin_reader.next_line().await {
        let trimmed = line.trim();
        if trimmed == "exit" {
            break;
        }

        if !trimmed.is_empty() {
            if let Err(e) = chat.send_chat(trimmed).await {
                eprintln!("Failed to send: {:?}", e);
            }
        }
        print!("> ");
        std::io::stdout().flush().unwrap();
    }

    println!("Shutting down mesh node...");
    mesh.shutdown().await?;

    Ok(())
}
