//! OmniMesh Node — CLI entrypoint.
//!
//! Usage:
//!   omnimesh --config config.toml
//!   omnimesh init
//!   omnimesh identity show

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use clap::{Parser, Subcommand};
use tracing::info;

use omnimesh_core::config::Config;
use omnimesh_core::error::Result;
use omnimesh_core::telemetry;
use omnimesh_core::types::PeerId;
use omnimesh_identity::keypair::Keypair;
use omnimesh_identity::keystore::{FileKeyStore, KeyStore};
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_identity::virtual_ip::VirtualIp;

/// OmniMesh — A modular P2P mesh networking node.
#[derive(Parser)]
#[command(name = "omnimesh", version, about)]
struct Cli {
    /// Path to configuration file.
    #[arg(short, long, default_value = "config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new node (generate identity and config).
    Init,

    /// Show node identity information.
    Identity {
        #[command(subcommand)]
        action: IdentityAction,
    },

    /// Start the node daemon.
    Run,
}

#[derive(Subcommand)]
enum IdentityAction {
    /// Display the current node identity.
    Show,
    /// Generate a new identity.
    Generate,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // Load config (or use defaults)
    let config = Config::load(Some(&cli.config)).unwrap_or_default();

    // Initialize telemetry
    telemetry::init(&config.telemetry.log_level, config.telemetry.json_logs);

    match cli.command {
        Some(Commands::Init) => {
            cmd_init(&config).await?;
        }
        Some(Commands::Identity { action }) => match action {
            IdentityAction::Show => cmd_identity_show(&config)?,
            IdentityAction::Generate => cmd_identity_generate(&config)?,
        },
        Some(Commands::Run) | None => {
            cmd_run(&config).await?;
        }
    }

    Ok(())
}

/// Initialize a new node: generate identity + write default config.
async fn cmd_init(config: &Config) -> anyhow::Result<()> {
    info!("initializing new OmniMesh node");

    // Create data directory
    std::fs::create_dir_all(&config.node.data_dir)?;

    // Generate identity
    let kp = Keypair::generate();
    let peer_id = kp.to_peer_id();
    let vip = VirtualIp::from_peer_id(&peer_id);

    // Store encrypted key
    let store = FileKeyStore::new(&config.identity.key_path);
    if store.exists() {
        println!(
            "⚠  Identity already exists at {}",
            config.identity.key_path.display()
        );
        println!("   PeerId: {}", peer_id);
        return Ok(());
    }

    // For init, use an empty passphrase (user can change later)
    store.store(&kp, "")?;

    println!("✓  Node initialized successfully!");
    println!("   PeerId:     {}", peer_id);
    println!("   Virtual IP: {}", vip);
    println!("   Key file:   {}", config.identity.key_path.display());
    println!("   Data dir:   {}", config.node.data_dir.display());

    // Write default config
    let config_toml = config.to_toml()?;
    std::fs::write("config.toml", &config_toml)?;
    println!("   Config:     config.toml");

    Ok(())
}

/// Show the current node identity.
fn cmd_identity_show(config: &Config) -> anyhow::Result<()> {
    let store = FileKeyStore::new(&config.identity.key_path);
    if !store.exists() {
        println!("No identity found. Run `omnimesh init` first.");
        return Ok(());
    }

    let kp = store.load("")?;
    let peer_id = kp.to_peer_id();
    let vip = VirtualIp::from_peer_id(&peer_id);

    println!("Node Identity:");
    println!("  PeerId:      {}", peer_id);
    println!("  Short ID:    {}", peer_id.short());
    println!("  Virtual IP:  {}", vip);
    println!("  Public Key:  {}", kp.public_key().to_hex());
    println!("  Key file:    {}", config.identity.key_path.display());

    Ok(())
}

/// Generate a new identity (overwriting any existing one).
fn cmd_identity_generate(config: &Config) -> anyhow::Result<()> {
    let kp = Keypair::generate();
    let peer_id = kp.to_peer_id();
    let vip = VirtualIp::from_peer_id(&peer_id);

    let store = FileKeyStore::new(&config.identity.key_path);
    store.store(&kp, "")?;

    println!("✓  New identity generated!");
    println!("   PeerId:     {}", peer_id);
    println!("   Virtual IP: {}", vip);

    Ok(())
}

/// Start the node daemon.
async fn cmd_run(config: &Config) -> anyhow::Result<()> {
    info!(
        name = %config.node.name,
        listen = %config.transport.listen_addr,
        "starting OmniMesh node"
    );

    // Load identity
    let store = FileKeyStore::new(&config.identity.key_path);
    if !store.exists() {
        println!("No identity found. Run `omnimesh init` first.");
        return Ok(());
    }

    let kp = store.load("")?;
    let peer_id = kp.to_peer_id();
    let vip = VirtualIp::from_peer_id(&peer_id);

    info!(peer_id = %peer_id.short(), vip = %vip, "node identity loaded");

    // Initialize QuicTransport
    let transport = omnimesh_transport::quic::QuicTransport::new();

    // Build the orchestrator client
    let mesh = omnimesh_sdk::OmniMeshBuilder::new()
        .config(config.clone())
        .keypair(kp)
        .transport(transport)
        .build()
        .await?;

    // Instantiate and register default services
    let chat = Arc::new(omnimesh_service::ChatService::new(mesh.clone()));
    mesh.register_service(chat.clone());

    struct DefaultRpcHandler;
    #[async_trait]
    impl omnimesh_service::RpcHandler for DefaultRpcHandler {
        async fn handle_request(&self, _from: PeerId, method: &str, request: Vec<u8>) -> Result<Vec<u8>> {
            info!("Received RPC request for method {}", method);
            Ok(request)
        }
    }
    let rpc = Arc::new(omnimesh_service::RpcService::new(mesh.clone(), Arc::new(DefaultRpcHandler)));
    mesh.register_service(rpc);

    let pubsub = Arc::new(omnimesh_service::PubSubService::new(mesh.clone()));
    mesh.register_service(pubsub);

    println!("OmniMesh node running");
    println!("  Name:        {}", config.node.name);
    println!("  PeerId:      {}", peer_id.short());
    println!("  Virtual IP:  {}", vip);
    println!("  Listening:   {}", config.transport.listen_addr);
    println!();
    println!("Press Ctrl+C to stop.");

    // Subscribe to chat and log incoming messages to stdout
    let mut chat_rx = chat.subscribe();
    tokio::spawn(async move {
        while let Ok((from, text)) = chat_rx.recv().await {
            println!("\n[{}] Chat: {}", from.short(), text);
        }
    });

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;
    info!("shutting down");

    mesh.shutdown().await?;

    Ok(())
}
