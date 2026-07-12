//! High-level SDK orchestrator for the OmniMesh network.
//!
//! Combines transport, identity, cryptography, discovery, and routing layers
//! into a unified, developer-friendly client interface.

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, mpsc, Mutex};
use tracing::{error, info};

use omnimesh_core::config::Config;
use omnimesh_core::error::{OmniMeshError, Result};
use omnimesh_core::types::{PeerId, ProtocolVersion};
use omnimesh_identity::keypair::Keypair;
use omnimesh_identity::peer_id::PeerIdExt;
use omnimesh_identity::virtual_ip::VirtualIp;
use omnimesh_transport::traits::Connection;

use omnimesh_crypto::noise::{NoiseHandshake, NoiseTransport, Role};
use omnimesh_discovery::service::DiscoveryService;
use omnimesh_routing::router::{LinkStateUpdate, Router};
use omnimesh_service::{MessageSender, Service};

/// Opaque object-safe connection trait.
#[async_trait]
pub trait DynConnection: Send + Sync + 'static {
    async fn send(&mut self, data: Bytes) -> Result<()>;
    async fn recv(&mut self) -> Result<Option<Bytes>>;
    async fn close(&mut self) -> Result<()>;
    fn remote_addr(&self) -> SocketAddr;
    fn is_connected(&self) -> bool;
}

#[async_trait]
impl<T: Connection> DynConnection for T {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        self.send(data).await
    }
    async fn recv(&mut self) -> Result<Option<Bytes>> {
        self.recv().await
    }
    async fn close(&mut self) -> Result<()> {
        self.close().await
    }
    fn remote_addr(&self) -> SocketAddr {
        self.remote_addr()
    }
    fn is_connected(&self) -> bool {
        self.is_connected()
    }
}

/// Opaque object-safe transport trait.
#[async_trait]
pub trait DynTransport: Send + Sync + 'static {
    async fn listen(&mut self, addr: SocketAddr) -> Result<()>;
    async fn accept(&mut self) -> Result<Box<dyn DynConnection>>;
    async fn connect(&mut self, addr: SocketAddr) -> Result<Box<dyn DynConnection>>;
    async fn shutdown(&mut self) -> Result<()>;
    fn local_addr(&self) -> Option<SocketAddr>;
}

#[async_trait]
impl<T: omnimesh_transport::traits::Transport> DynTransport for T {
    async fn listen(&mut self, addr: SocketAddr) -> Result<()> {
        self.listen(addr).await
    }
    async fn accept(&mut self) -> Result<Box<dyn DynConnection>> {
        let conn = self.accept().await?;
        Ok(Box::new(conn))
    }
    async fn connect(&mut self, addr: SocketAddr) -> Result<Box<dyn DynConnection>> {
        let conn = self.connect(addr).await?;
        Ok(Box::new(conn))
    }
    async fn shutdown(&mut self) -> Result<()> {
        self.shutdown().await
    }
    fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr()
    }
}

/// Dynamic wrapper holding an active peer connection session's write channel and transport.
#[derive(Clone)]
pub struct Session {
    write_tx: mpsc::Sender<Bytes>,
    transport: Arc<Mutex<NoiseTransport>>,
}

/// Wire protocol frame structure.
#[derive(Debug, Serialize, Deserialize)]
pub struct WirePacket {
    pub magic: [u8; 4],
    pub version: [u8; 3],
    pub packet_type: u8,
    pub payload: Vec<u8>,
}

/// Inner payload container dispatching bytes to services.
#[derive(Debug, Serialize, Deserialize)]
pub struct ServicePayload {
    pub service_id: String,
    pub payload: Vec<u8>,
}

/// The main orchestrator managing the local P2P node.
pub struct OmniMesh {
    keypair: Keypair,
    local_peer_id: PeerId,
    vip: VirtualIp,
    transport: Arc<Mutex<Box<dyn DynTransport>>>,
    discovery: Arc<DiscoveryService>,
    router: Arc<Router>,
    sessions: Arc<DashMap<PeerId, Session>>,
    services: Arc<DashMap<&'static str, Arc<dyn Service>>>,
    shutdown_tx: broadcast::Sender<()>,
    running: Arc<AtomicBool>,
}

#[async_trait]
impl MessageSender for OmniMesh {
    async fn send_message(&self, to: PeerId, service_id: &'static str, payload: Vec<u8>) -> Result<()> {
        self.send_to_peer(to, service_id, payload).await
    }

    async fn broadcast_message(&self, service_id: &'static str, payload: Vec<u8>) -> Result<()> {
        self.broadcast_to_peers(service_id, payload).await
    }
}

impl OmniMesh {
    /// Accessor for the PeerId of this node.
    pub fn peer_id(&self) -> PeerId {
        self.local_peer_id.clone()
    }

    /// Accessor for the Virtual IPv6 IP of this node.
    pub fn virtual_ip(&self) -> VirtualIp {
        self.vip
    }

    /// Accessor for the Router.
    pub fn router(&self) -> Arc<Router> {
        self.router.clone()
    }

    /// Accessor for the DiscoveryService.
    pub fn discovery(&self) -> Arc<DiscoveryService> {
        self.discovery.clone()
    }

    /// Register a service to process incoming messages.
    pub fn register_service(&self, service: Arc<dyn Service>) {
        self.services.insert(service.id(), service);
    }

    /// Connect to a remote node's data plane socket address.
    pub async fn connect_addr(&self, addr: SocketAddr) -> Result<PeerId> {
        let mut tx_guard = self.transport.lock().await;
        let mut conn = tx_guard.connect(addr).await?;
        drop(tx_guard);

        // Perform initiator handshake
        let handshake = NoiseHandshake::with_keypair(Role::Initiator, &self.keypair.seed())?;
        let (transport, remote_peer_id) = perform_handshake(&mut *conn, handshake, self.local_peer_id.clone()).await?;

        let transport_arc = Arc::new(Mutex::new(transport));
        let (write_tx, write_rx) = mpsc::channel(100);

        let session = Session {
            write_tx,
            transport: transport_arc.clone(),
        };
        self.sessions.insert(remote_peer_id.clone(), session);

        // Spawn read/write loop for session
        let services = self.services.clone();
        let router = self.router.clone();
        let shutdown_rx = self.shutdown_tx.subscribe();
        let peer_id_clone = remote_peer_id.clone();
        tokio::spawn(async move {
            run_session_loop(peer_id_clone, conn, transport_arc, write_rx, services, router, shutdown_rx).await;
        });

        Ok(remote_peer_id)
    }

    /// Try to establish a connection with a peer using discovered addresses.
    pub async fn connect_peer(&self, peer: &PeerId) -> Result<()> {
        if self.sessions.contains_key(peer) {
            return Ok(());
        }

        let addrs = self.discovery.lookup(peer);
        if addrs.is_empty() {
            return Err(OmniMeshError::Discovery(format!(
                "no addresses found for peer {}",
                peer
            )));
        }

        let mut last_err = None;
        for addr in addrs {
            match self.connect_addr(addr).await {
                Ok(_) => return Ok(()),
                Err(e) => last_err = Some(e),
            }
        }

        Err(last_err.unwrap_or_else(|| {
            OmniMeshError::Connection("failed to connect to peer addresses".into())
        }))
    }

    /// Send application message to a specific peer.
    async fn send_to_peer(&self, to: PeerId, service_id: &'static str, payload: Vec<u8>) -> Result<()> {
        // Resolve target peer (connecting if needed)
        if !self.sessions.contains_key(&to) {
            self.connect_peer(&to).await?;
        }

        let session = self.sessions.get(&to).ok_or_else(|| {
            OmniMeshError::Connection(format!("no active session to peer {}", to))
        })?;

        let service_payload = ServicePayload {
            service_id: service_id.to_string(),
            payload,
        };
        let plain_bytes = bincode::serialize(&service_payload).unwrap();

        let encrypted = {
            let mut trans = session.transport.lock().await;
            trans.encrypt(&plain_bytes)?
        };

        let packet = WirePacket {
            magic: *b"OMSH",
            version: [0, 1, 0],
            packet_type: 0x02, // Data
            payload: encrypted,
        };
        let wire_bytes = bincode::serialize(&packet).unwrap();
        session.write_tx.send(Bytes::from(wire_bytes)).await
            .map_err(|_| OmniMeshError::Connection("session write channel closed".into()))
    }

    /// Broadcast application message to all active/connected peers.
    async fn broadcast_to_peers(&self, service_id: &'static str, payload: Vec<u8>) -> Result<()> {
        let service_payload = ServicePayload {
            service_id: service_id.to_string(),
            payload,
        };
        let plain_bytes = bincode::serialize(&service_payload).unwrap();

        for entry in self.sessions.iter() {
            let session = entry.value();

            if let Ok(encrypted) = {
                let mut trans = session.transport.lock().await;
                trans.encrypt(&plain_bytes)
            } {
                let packet = WirePacket {
                    magic: *b"OMSH",
                    version: [0, 1, 0],
                    packet_type: 0x02, // Data
                    payload: encrypted,
                };
                if let Ok(wire_bytes) = bincode::serialize(&packet) {
                    let _ = session.write_tx.send(Bytes::from(wire_bytes)).await;
                }
            }
        }

        Ok(())
    }

    /// Gracefully shutdown the P2P engine.
    pub async fn shutdown(&self) -> Result<()> {
        if !self.running.swap(false, Ordering::SeqCst) {
            return Ok(());
        }

        let _ = self.shutdown_tx.send(());
        self.sessions.clear();

        // Shutdown transport
        let mut tx_guard = self.transport.lock().await;
        tx_guard.shutdown().await?;

        Ok(())
    }
}

// ── Background Workers ────────────────────────────────────────────────

async fn run_accept_loop(
    keypair: Keypair,
    transport: Arc<Mutex<Box<dyn DynTransport>>>,
    sessions: Arc<DashMap<PeerId, Session>>,
    services: Arc<DashMap<&'static str, Arc<dyn Service>>>,
    router: Arc<Router>,
    shutdown_tx: broadcast::Sender<()>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        let mut tx_guard = transport.lock().await;
        tokio::select! {
            accept_res = tx_guard.accept() => {
                drop(tx_guard);
                match accept_res {
                    Ok(mut conn) => {
                        let kp = keypair.clone();
                        let sessions_clone = sessions.clone();
                        let services_clone = services.clone();
                        let router_clone = router.clone();
                        let shutdown_tx_clone = shutdown_tx.clone();
                        let local_id = kp.to_peer_id();

                        tokio::spawn(async move {
                            match perform_handshake(&mut *conn, NoiseHandshake::with_keypair(Role::Responder, &kp.seed()).unwrap(), local_id).await {
                                Ok((transport, remote_peer_id)) => {
                                    let transport_arc = Arc::new(Mutex::new(transport));
                                    let (write_tx, write_rx) = mpsc::channel(100);

                                    let session = Session {
                                        write_tx,
                                        transport: transport_arc.clone(),
                                    };
                                    sessions_clone.insert(remote_peer_id.clone(), session);

                                    let shutdown_rx_conn = shutdown_tx_clone.subscribe();
                                    run_session_loop(remote_peer_id, conn, transport_arc, write_rx, services_clone, router_clone, shutdown_rx_conn).await;
                                }
                                Err(e) => {
                                    error!("Inbound handshake failed: {:?}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept connection failed: {:?}", e);
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}

async fn run_session_loop(
    peer_id: PeerId,
    mut conn: Box<dyn DynConnection>,
    transport: Arc<Mutex<NoiseTransport>>,
    mut write_rx: mpsc::Receiver<Bytes>,
    services: Arc<DashMap<&'static str, Arc<dyn Service>>>,
    router: Arc<Router>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        tokio::select! {
            // 1. Send outgoing bytes
            outgoing = write_rx.recv() => {
                match outgoing {
                    Some(bytes) => {
                        if let Err(e) = conn.send(bytes).await {
                            error!("Session write error with peer {}: {:?}", peer_id, e);
                            break;
                        }
                    }
                    None => {
                        break;
                    }
                }
            }
            // 2. Receive incoming bytes
            res = conn.recv() => {
                match res {
                    Ok(Some(bytes)) => {
                        match handle_wire_packet(peer_id.clone(), &bytes, &transport, &services, &router).await {
                            Ok(Some(reply)) => {
                                if let Err(e) = conn.send(reply).await {
                                    error!("Failed to send packet reply to peer {}: {:?}", peer_id, e);
                                    break;
                                }
                            }
                            Ok(None) => {}
                            Err(e) => {
                                error!("Error processing packet from peer {}: {:?}", peer_id, e);
                            }
                        }
                    }
                    Ok(None) => {
                        info!("Session disconnected: {}", peer_id);
                        break;
                    }
                    Err(e) => {
                        error!("Session read error with peer {}: {:?}", peer_id, e);
                        break;
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
    let _ = conn.close().await;
}

async fn handle_wire_packet(
    from: PeerId,
    bytes: &[u8],
    transport: &Arc<Mutex<NoiseTransport>>,
    services: &Arc<DashMap<&'static str, Arc<dyn Service>>>,
    router: &Arc<Router>,
) -> Result<Option<Bytes>> {
    let packet: WirePacket = bincode::deserialize(bytes)
        .map_err(|e| OmniMeshError::Serialization(format!("packet deserialize: {}", e)))?;

    if packet.magic != *b"OMSH" {
        return Err(OmniMeshError::InvalidPacket("invalid packet magic".into()));
    }

    match packet.packet_type {
        0x02 => {
            // Application Data
            let decrypted = {
                let mut trans = transport.lock().await;
                trans.decrypt(&packet.payload)?
            };
            let service_payload: ServicePayload =
                bincode::deserialize(&decrypted).map_err(|e| {
                    OmniMeshError::Serialization(format!("service payload deserialize: {}", e))
                })?;

            if let Some(service) = services.get(service_payload.service_id.as_str()) {
                service
                    .handle_message(from, service_payload.payload)
                    .await?;
            }
            Ok(None)
        }
        0x03 => {
            // Control plane routing update
            let decrypted = {
                let mut trans = transport.lock().await;
                trans.decrypt(&packet.payload)?
            };
            if let Ok(update) = bincode::deserialize::<LinkStateUpdate>(&decrypted) {
                router.process_link_state_update(update);
            }
            Ok(None)
        }
        0x06 => {
            // Ping
            let decrypted = {
                let mut trans = transport.lock().await;
                trans.decrypt(&packet.payload)?
            };
            let encrypted = {
                let mut trans = transport.lock().await;
                trans.encrypt(&decrypted)?
            };
            let pong_packet = WirePacket {
                magic: *b"OMSH",
                version: [0, 1, 0],
                packet_type: 0x07, // Pong
                payload: encrypted,
            };
            let wire_bytes = bincode::serialize(&pong_packet).unwrap();
            Ok(Some(Bytes::from(wire_bytes)))
        }
        0x07 => {
            // Pong
            let decrypted = {
                let mut trans = transport.lock().await;
                trans.decrypt(&packet.payload)?
            };
            if let Ok(sent_time_ms) = bincode::deserialize::<u64>(&decrypted) {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;
                if now_ms >= sent_time_ms {
                    let rtt = (now_ms - sent_time_ms) as f64;
                    router.record_latency(from, rtt);
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

async fn run_probing_loop(
    sessions: Arc<DashMap<PeerId, Session>>,
    shutdown_tx: broadcast::Sender<()>,
) {
    let mut shutdown_rx = shutdown_tx.subscribe();
    let mut interval = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let payload = bincode::serialize(&now_ms).unwrap();

                for entry in sessions.iter() {
                    let session = entry.value();
                    let encrypted_res = {
                        let mut trans = session.transport.lock().await;
                        trans.encrypt(&payload)
                    };

                    if let Ok(encrypted) = encrypted_res {
                        let packet = WirePacket {
                            magic: *b"OMSH",
                            version: [0, 1, 0],
                            packet_type: 0x06, // Ping
                            payload: encrypted,
                        };
                        if let Ok(wire_bytes) = bincode::serialize(&packet) {
                            let _ = session.write_tx.send(Bytes::from(wire_bytes)).await;
                        }
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}

async fn run_gossip_loop(
    sessions: Arc<DashMap<PeerId, Session>>,
    router: Arc<Router>,
    shutdown_tx: broadcast::Sender<()>,
) {
    let mut shutdown_rx = shutdown_tx.subscribe();
    let mut interval = tokio::time::interval(Duration::from_secs(10));

    loop {
        tokio::select! {
            _ = interval.tick() => {
                for entry in sessions.iter() {
                    let peer_id = entry.key();

                    if let Some(update) = router.get_link_state_update(peer_id) {
                        let payload = bincode::serialize(&update).unwrap();

                        // Broadcast update to all sessions
                        for peer_entry in sessions.iter() {
                            let session = peer_entry.value();
                            let encrypted_res = {
                                let mut trans = session.transport.lock().await;
                                trans.encrypt(&payload)
                            };
                            if let Ok(encrypted) = encrypted_res {
                                let packet = WirePacket {
                                    magic: *b"OMSH",
                                    version: [0, 1, 0],
                                    packet_type: 0x03, // Control (Gossip Update)
                                    payload: encrypted,
                                };
                                if let Ok(wire_bytes) = bincode::serialize(&packet) {
                                    let _ = session.write_tx.send(Bytes::from(wire_bytes)).await;
                                }
                            }
                        }
                    }
                }
            }
            _ = shutdown_rx.recv() => {
                break;
            }
        }
    }
}

#[derive(Serialize, Deserialize)]
struct HandshakePayload {
    version: ProtocolVersion,
    peer_id: PeerId,
}

async fn perform_handshake(
    conn: &mut dyn DynConnection,
    mut handshake: NoiseHandshake,
    local_peer_id: PeerId,
) -> Result<(NoiseTransport, PeerId)> {
    let mut remote_peer_id = None;
    info!("Starting handshake for role {:?}", handshake.role());

    loop {
        if handshake.is_finished() {
            info!("Handshake finished for role {:?}", handshake.role());
            break;
        }

        match handshake.role() {
            Role::Initiator => {
                let payload_struct = HandshakePayload {
                    version: ProtocolVersion::CURRENT,
                    peer_id: local_peer_id.clone(),
                };
                let payload = bincode::serialize(&payload_struct).unwrap();
                info!("Initiator writing and sending message 1...");
                let msg = handshake.write_message(&payload)?;
                conn.send(Bytes::from(msg)).await?;

                if handshake.is_finished() {
                    info!("Initiator finished handshake loop after sending msg");
                    break;
                }

                info!("Initiator waiting to receive message 2 reply...");
                let reply = conn
                    .recv()
                    .await?
                    .ok_or_else(|| OmniMeshError::Handshake("closed during handshake".into()))?;
                info!("Initiator received message 2 reply, size = {}", reply.len());
                let reply_payload = handshake.read_message(&reply)?;
                let remote_payload: HandshakePayload = bincode::deserialize(&reply_payload)
                    .map_err(|e| OmniMeshError::Serialization(format!("handshake payload: {}", e)))?;

                if !ProtocolVersion::CURRENT.is_compatible_with(&remote_payload.version) {
                    return Err(OmniMeshError::ProtocolMismatch {
                        local: ProtocolVersion::CURRENT.to_string(),
                        remote: remote_payload.version.to_string(),
                    });
                }
                remote_peer_id = Some(remote_payload.peer_id);
            }
            Role::Responder => {
                info!("Responder waiting to receive message 1...");
                let incoming = conn
                    .recv()
                    .await?
                    .ok_or_else(|| OmniMeshError::Handshake("closed during handshake".into()))?;
                info!("Responder received message 1, size = {}", incoming.len());
                let inc_payload = handshake.read_message(&incoming)?;
                let remote_payload: HandshakePayload = bincode::deserialize(&inc_payload)
                    .map_err(|e| OmniMeshError::Serialization(format!("handshake payload: {}", e)))?;

                if !ProtocolVersion::CURRENT.is_compatible_with(&remote_payload.version) {
                    return Err(OmniMeshError::ProtocolMismatch {
                        local: ProtocolVersion::CURRENT.to_string(),
                        remote: remote_payload.version.to_string(),
                    });
                }
                remote_peer_id = Some(remote_payload.peer_id);

                if handshake.is_finished() {
                    info!("Responder finished handshake loop after reading msg 1");
                    break;
                }

                let payload_struct = HandshakePayload {
                    version: ProtocolVersion::CURRENT,
                    peer_id: local_peer_id.clone(),
                };
                let payload = bincode::serialize(&payload_struct).unwrap();
                info!("Responder writing and sending message 2 reply...");
                let reply = handshake.write_message(&payload)?;
                conn.send(Bytes::from(reply)).await?;
            }
        }
    }

    let role = handshake.role();
    info!("Handshake loops exited for role {:?}, creating transport...", role);
    let transport = handshake.into_transport()?;
    let remote_id = remote_peer_id.ok_or_else(|| {
        OmniMeshError::Handshake("handshake completed without remote peer id".into())
    })?;
    info!("Handshake fully successful for role {:?}, remote_peer_id = {:?}", role, remote_id);
    Ok((transport, remote_id))
}

// ── Orchestrator Builder ──────────────────────────────────────────────

/// Builder pattern to configure and construct an `OmniMesh` orchestrator.
#[derive(Default)]
pub struct OmniMeshBuilder {
    config: Config,
    keypair: Option<Keypair>,
    transport: Option<Box<dyn DynTransport>>,
}

impl OmniMeshBuilder {
    /// Initialize a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load options from an existing Config structure.
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// Override the node cryptographic identity keypair.
    pub fn keypair(mut self, keypair: Keypair) -> Self {
        self.keypair = Some(keypair);
        self
    }

    /// Override the swappable transport layer implementation.
    pub fn transport<T: omnimesh_transport::traits::Transport>(mut self, transport: T) -> Self {
        self.transport = Some(Box::new(transport));
        self
    }

    /// Build and start the `OmniMesh` orchestrator background execution workers.
    pub async fn build(self) -> Result<Arc<OmniMesh>> {
        let keypair = self.keypair.unwrap_or_else(Keypair::generate);
        let peer_id = keypair.to_peer_id();
        let vip = VirtualIp::from_peer_id(&peer_id);

        let mut transport = self.transport.ok_or_else(|| {
            OmniMeshError::Config("missing required transport layer configuration".into())
        })?;

        // Bind transport to listen address
        transport.listen(self.config.transport.listen_addr).await?;

        // Create discovery service
        let discovery = Arc::new(DiscoveryService::new(&self.config, &keypair, 0).await?);

        // Register the local data plane address so it can be advertised
        if let Some(local_addr) = transport.local_addr() {
            let _ = discovery.register_local_address(local_addr).await;
        }

        // Create routing coordinator
        let router = Arc::new(Router::new(peer_id.clone()));

        let (shutdown_tx, shutdown_rx) = broadcast::channel(1);
        let sessions = Arc::new(DashMap::new());
        let services = Arc::new(DashMap::new());

        let node = Arc::new(OmniMesh {
            keypair: keypair.clone(),
            local_peer_id: peer_id,
            vip,
            transport: Arc::new(Mutex::new(transport)),
            discovery,
            router: router.clone(),
            sessions: sessions.clone(),
            services: services.clone(),
            shutdown_tx: shutdown_tx.clone(),
            running: Arc::new(AtomicBool::new(true)),
        });

        // 1. Inbound listener worker
        let transport_clone = node.transport.clone();
        let shutdown_tx_clone = shutdown_tx.clone();
        tokio::spawn(async move {
            run_accept_loop(
                keypair,
                transport_clone,
                sessions.clone(),
                services,
                router,
                shutdown_tx_clone,
                shutdown_rx,
            )
            .await;
        });

        // 2. Active probing loop
        let sessions_clone = node.sessions.clone();
        let shutdown_tx_clone2 = shutdown_tx.clone();
        tokio::spawn(async move {
            run_probing_loop(sessions_clone, shutdown_tx_clone2).await;
        });

        // 3. Gossip updates loop
        let sessions_clone2 = node.sessions.clone();
        let router_clone2 = node.router.clone();
        let shutdown_tx_clone3 = shutdown_tx.clone();
        tokio::spawn(async move {
            run_gossip_loop(sessions_clone2, router_clone2, shutdown_tx_clone3).await;
        });

        Ok(node)
    }
}
