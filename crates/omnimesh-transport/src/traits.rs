//! Transport and Connection trait definitions.
//!
//! These traits define the interface that all transport implementations
//! must satisfy. Application code programs against these traits, making
//! the transport layer fully swappable.

use std::net::SocketAddr;

use async_trait::async_trait;
use bytes::Bytes;
use omnimesh_core::error::Result;
use omnimesh_core::types::PeerId;

/// Events emitted by a transport layer.
#[derive(Debug, Clone)]
pub enum TransportEvent {
    /// A new inbound connection was accepted.
    IncomingConnection {
        /// The PeerId of the remote peer (if known).
        peer_id: Option<PeerId>,
        /// The remote socket address.
        remote_addr: SocketAddr,
    },
    /// A connection was closed (gracefully or due to error).
    ConnectionClosed {
        /// The PeerId of the disconnected peer.
        peer_id: PeerId,
        /// Reason for closure.
        reason: String,
    },
}

/// A pluggable network transport.
///
/// Implementations handle the details of establishing connections,
/// listening for inbound connections, and data transfer over a specific
/// protocol (QUIC, UDP, WebRTC, etc.).
///
/// # Modularity
///
/// Each transport is a separate, self-contained module. To add a new
/// transport (e.g., Bluetooth), implement this trait in a new file.
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    /// The connection type produced by this transport.
    type Conn: Connection;

    /// Start listening for incoming connections on the given address.
    async fn listen(&mut self, addr: SocketAddr) -> Result<()>;

    /// Accept the next incoming connection.
    ///
    /// This blocks until a new connection arrives.
    async fn accept(&mut self) -> Result<Self::Conn>;

    /// Establish an outbound connection to a remote peer.
    async fn connect(&mut self, addr: SocketAddr) -> Result<Self::Conn>;

    /// Stop listening and close all connections.
    async fn shutdown(&mut self) -> Result<()>;

    /// Get the local address this transport is bound to.
    fn local_addr(&self) -> Option<SocketAddr>;
}

/// A bidirectional connection to a remote peer.
///
/// Provides simple send/receive semantics over an established connection.
#[async_trait]
pub trait Connection: Send + Sync + 'static {
    /// Send data to the remote peer.
    async fn send(&mut self, data: Bytes) -> Result<()>;

    /// Receive data from the remote peer.
    ///
    /// Returns `None` if the connection was closed gracefully.
    async fn recv(&mut self) -> Result<Option<Bytes>>;

    /// Close the connection gracefully.
    async fn close(&mut self) -> Result<()>;

    /// Get the remote socket address.
    fn remote_addr(&self) -> SocketAddr;

    /// Check if the connection is still alive.
    fn is_connected(&self) -> bool;
}
