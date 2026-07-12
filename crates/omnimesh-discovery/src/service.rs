//! Main peer discovery service using `rust-libp2p`.
//!
//! Spawns a background task running a libp2p Swarm configured with Kademlia,
//! mDNS, and Identify behaviours. It monitors discovery events and updates
//! the local thread-safe `PeerAddressCache`.

use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use libp2p::futures::StreamExt;
use libp2p::identify;
use libp2p::kad::{self, store::MemoryStore};
use libp2p::mdns;
use libp2p::multiaddr::{Multiaddr, Protocol};
use libp2p::swarm::{NetworkBehaviour, SwarmEvent};
use libp2p::{noise, tcp, yamux, SwarmBuilder};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use omnimesh_core::config::Config;
use omnimesh_core::error::OmniMeshError;
use omnimesh_core::types::PeerId;
use omnimesh_identity::Keypair;

use crate::cache::PeerAddressCache;
use crate::key_conv;

/// Custom NetworkBehaviour combining mDNS, Kademlia, and Identify.
#[derive(NetworkBehaviour)]
#[behaviour(to_swarm = "BehaviourEvent")]
struct DiscoveryBehaviour {
    kad: kad::Behaviour<MemoryStore>,
    mdns: mdns::tokio::Behaviour,
    identify: identify::Behaviour,
}

/// Events produced by the custom NetworkBehaviour.
#[derive(Debug)]
enum BehaviourEvent {
    Kad(kad::Event),
    Mdns(mdns::Event),
    Identify(identify::Event),
}

impl From<kad::Event> for BehaviourEvent {
    fn from(event: kad::Event) -> Self {
        Self::Kad(event)
    }
}

impl From<mdns::Event> for BehaviourEvent {
    fn from(event: mdns::Event) -> Self {
        Self::Mdns(event)
    }
}

impl From<identify::Event> for BehaviourEvent {
    fn from(event: identify::Event) -> Self {
        Self::Identify(event)
    }
}

/// Commands sent to the background discovery task.
enum DiscoveryCommand {
    Bootstrap,
    RegisterLocalAddr(SocketAddr),
}

/// Peer discovery service wrapper.
pub struct DiscoveryService {
    cache: PeerAddressCache,
    cmd_tx: mpsc::Sender<DiscoveryCommand>,
    task_handle: tokio::task::JoinHandle<()>,
}

impl DiscoveryService {
    /// Create and start a new discovery service.
    pub async fn new(
        config: &Config,
        keypair: &Keypair,
        discovery_port: u16,
    ) -> std::result::Result<Self, OmniMeshError> {
        let cache = PeerAddressCache::new();
        let (cmd_tx, cmd_rx) = mpsc::channel(64);

        // Convert keypair and PeerId to libp2p representations
        let libp2p_kp = key_conv::to_libp2p_keypair(keypair)?;
        let local_peer_id = libp2p_kp.public().to_peer_id();

        // Build the swarm using libp2p v0.54 builders
        let mut swarm = SwarmBuilder::with_existing_identity(libp2p_kp.clone())
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )
            .map_err(|e| {
                OmniMeshError::Discovery(format!("failed to configure TCP transport: {}", e))
            })?
            .with_behaviour(|key| {
                let local_pub = key.public();
                let store = MemoryStore::new(local_peer_id);
                let kad = kad::Behaviour::new(local_peer_id, store);

                let mdns_config = mdns::Config::default();
                let mdns = mdns::tokio::Behaviour::new(mdns_config, local_peer_id)?;

                let identify_config =
                    identify::Config::new("/omnimesh/1.0.0".to_string(), local_pub);
                let identify = identify::Behaviour::new(identify_config);

                Ok::<_, Box<dyn std::error::Error + Send + Sync>>(DiscoveryBehaviour {
                    kad,
                    mdns,
                    identify,
                })
            })
            .map_err(|e| OmniMeshError::Discovery(format!("failed to construct behaviour: {}", e)))?
            .build();

        // Listen on the configured discovery port (IPv4 + IPv6 if possible)
        let listen_addr: Multiaddr = format!("/ip4/0.0.0.0/tcp/{}", discovery_port)
            .parse()
            .map_err(|e| OmniMeshError::Discovery(format!("invalid listen multiaddr: {}", e)))?;
        swarm.listen_on(listen_addr).map_err(|e| {
            OmniMeshError::Discovery(format!("failed to listen on discovery port: {}", e))
        })?;

        // Seed Kademlia routing table with bootstrap nodes from config
        for bootstrap_str in &config.transport.bootstrap_nodes {
            if let Ok(multiaddr) = bootstrap_str.parse::<Multiaddr>() {
                if let Some(Protocol::P2p(peer_id)) = multiaddr.iter().last() {
                    swarm
                        .behaviour_mut()
                        .kad
                        .add_address(&peer_id, multiaddr.clone());
                    info!(node = %bootstrap_str, "seeded bootstrap node");
                } else {
                    warn!(node = %bootstrap_str, "bootstrap node multiaddr lacks a peer ID (/p2p/...)");
                }
            } else {
                warn!(node = %bootstrap_str, "failed to parse bootstrap node multiaddr");
            }
        }

        // Spawn background task
        let cache_clone = cache.clone();
        let task_handle = tokio::spawn(async move {
            let mut prune_interval = tokio::time::interval(Duration::from_secs(60));
            let mut cmd_rx = cmd_rx;

            loop {
                tokio::select! {
                    _ = prune_interval.tick() => {
                        cache_clone.prune();
                    }
                    cmd = cmd_rx.recv() => {
                        if let Some(command) = cmd {
                            match command {
                                DiscoveryCommand::Bootstrap => {
                                    info!("initiating kademlia routing table bootstrap");
                                    if let Err(e) = swarm.behaviour_mut().kad.bootstrap() {
                                        warn!("kademlia bootstrap failed: {:?}", e);
                                    }
                                }
                                DiscoveryCommand::RegisterLocalAddr(addr) => {
                                    // Advertise our own data plane socket address
                                    let multiaddr = socket_addr_to_multiaddr(&addr);
                                    swarm.add_external_address(multiaddr);
                                }
                            }
                        }
                    }
                    event = swarm.select_next_some() => {
                        match event {
                            SwarmEvent::NewListenAddr { address, .. } => {
                                info!(addr = %address, "discovery node listening locally");
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                                for (peer_id, multiaddr) in list {
                                    debug!(peer = %peer_id, addr = %multiaddr, "discovered peer via local mDNS");
                                    swarm.behaviour_mut().kad.add_address(&peer_id, multiaddr.clone());
                                    let _ = swarm.dial(peer_id);
                                    if let Some(addr) = multiaddr_to_socket_addr(&multiaddr) {
                                        if let Ok(omni_peer_id) = key_conv::from_libp2p_peer_id(peer_id) {
                                            cache_clone.insert(omni_peer_id, addr);
                                        }
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received { peer_id, info, .. })) => {
                                for addr in info.listen_addrs {
                                    swarm.behaviour_mut().kad.add_address(&peer_id, addr.clone());
                                    if let Some(saddr) = multiaddr_to_socket_addr(&addr) {
                                        if let Ok(omni_peer_id) = key_conv::from_libp2p_peer_id(peer_id) {
                                            cache_clone.insert(omni_peer_id, saddr);
                                        }
                                    }
                                }
                            }
                            SwarmEvent::Behaviour(BehaviourEvent::Kad(kad::Event::OutboundQueryProgressed { result, .. })) => {
                                if let kad::QueryResult::GetClosestPeers(Ok(ok)) = result {
                                    for peer in ok.peers {
                                        debug!(peer = %peer.peer_id, "found close peer via kademlia lookup");
                                        for addr in peer.addrs {
                                            if let Some(saddr) = multiaddr_to_socket_addr(&addr) {
                                                if let Ok(omni_peer_id) = key_conv::from_libp2p_peer_id(peer.peer_id) {
                                                    cache_clone.insert(omni_peer_id, saddr);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            other => {
                                debug!("swarm event: {:?}", other);
                            }
                        }
                    }
                }
            }
        });

        // Trigger initial Kademlia bootstrap shortly after start
        let _ = cmd_tx.send(DiscoveryCommand::Bootstrap).await;

        Ok(Self {
            cache,
            cmd_tx,
            task_handle,
        })
    }

    /// Lookup active discovered addresses for a PeerId.
    pub fn lookup(&self, peer_id: &PeerId) -> Vec<SocketAddr> {
        self.cache.lookup(peer_id)
    }

    /// Retrieve all discovered peer entries in the cache.
    pub fn all_peers(&self) -> Vec<(PeerId, Vec<SocketAddr>)> {
        self.cache.all_peers()
    }

    /// Force a Kademlia routing table bootstrap.
    pub async fn bootstrap(&self) -> std::result::Result<(), OmniMeshError> {
        self.cmd_tx
            .send(DiscoveryCommand::Bootstrap)
            .await
            .map_err(|_| OmniMeshError::Discovery("failed to send bootstrap command".into()))
    }

    /// Register a local endpoint (e.g. data port QUIC address) to advertise to peers.
    pub async fn register_local_address(
        &self,
        addr: SocketAddr,
    ) -> std::result::Result<(), OmniMeshError> {
        self.cmd_tx
            .send(DiscoveryCommand::RegisterLocalAddr(addr))
            .await
            .map_err(|_| OmniMeshError::Discovery("failed to send registration command".into()))
    }
}

impl Drop for DiscoveryService {
    fn drop(&mut self) {
        self.task_handle.abort();
    }
}

// ── Converters ───────────────────────────────────────────────────────

fn multiaddr_to_socket_addr(addr: &Multiaddr) -> Option<SocketAddr> {
    let mut ip: Option<IpAddr> = None;
    let mut port: Option<u16> = None;

    for proto in addr.iter() {
        match proto {
            Protocol::Ip4(ipv4) => ip = Some(IpAddr::V4(ipv4)),
            Protocol::Ip6(ipv6) => ip = Some(IpAddr::V6(ipv6)),
            Protocol::Tcp(p) => port = Some(p),
            Protocol::Udp(p) => port = Some(p),
            _ => {}
        }
    }

    if let (Some(ip), Some(port)) = (ip, port) {
        Some(SocketAddr::new(ip, port))
    } else {
        None
    }
}

fn socket_addr_to_multiaddr(addr: &SocketAddr) -> Multiaddr {
    let mut ma = Multiaddr::empty();
    match addr.ip() {
        IpAddr::V4(ip) => ma.push(Protocol::Ip4(ip)),
        IpAddr::V6(ip) => ma.push(Protocol::Ip6(ip)),
    }
    ma.push(Protocol::Udp(addr.port()));
    ma
}
