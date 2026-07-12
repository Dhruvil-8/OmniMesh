//! In-memory mock transport for testing.
//!
//! Provides a fully functional transport that operates over in-memory
//! channels. No network I/O occurs. This is invaluable for:
//! - Unit testing routing, discovery, and service layers
//! - Integration tests without port conflicts
//! - Benchmarking application logic in isolation

use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use parking_lot::Mutex;
use tokio::sync::mpsc;
use tracing::debug;

use omnimesh_core::error::{OmniMeshError, Result};

use crate::traits::{Connection, Transport};

/// A mock transport that uses in-memory channels.
pub struct MockTransport {
    /// Address this transport is "listening" on.
    local_addr: Option<SocketAddr>,
    /// Channel for incoming connections.
    incoming_tx: Option<mpsc::Sender<MockConnection>>,
    incoming_rx: Option<mpsc::Receiver<MockConnection>>,
    /// Shared registry of all mock transports (for cross-connect).
    registry: Arc<Mutex<MockRegistry>>,
}

/// Shared state for connecting mock transports to each other.
pub struct MockRegistry {
    listeners: std::collections::HashMap<SocketAddr, mpsc::Sender<MockConnection>>,
}

/// An in-memory bidirectional connection.
pub struct MockConnection {
    _local_addr: SocketAddr,
    remote_addr: SocketAddr,
    tx: mpsc::Sender<Bytes>,
    rx: mpsc::Receiver<Bytes>,
    connected: bool,
}

impl MockTransport {
    /// Create a new mock transport with a shared registry.
    ///
    /// All transports sharing the same registry can connect to each other.
    pub fn new(registry: Arc<Mutex<MockRegistry>>) -> Self {
        Self {
            local_addr: None,
            incoming_tx: None,
            incoming_rx: None,
            registry,
        }
    }

    /// Create a pair of connected mock transports for simple tests.
    pub fn create_pair() -> (Arc<Mutex<MockRegistry>>, Self, Self) {
        let registry = MockRegistry::new();
        let t1 = MockTransport::new(registry.clone());
        let t2 = MockTransport::new(registry.clone());
        (registry, t1, t2)
    }
}

impl MockRegistry {
    /// Create a new shared registry.
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            listeners: std::collections::HashMap::new(),
        }))
    }
}

#[async_trait]
impl Transport for MockTransport {
    type Conn = MockConnection;

    async fn listen(&mut self, addr: SocketAddr) -> Result<()> {
        let (tx, rx) = mpsc::channel(32);
        self.local_addr = Some(addr);
        self.incoming_tx = Some(tx.clone());
        self.incoming_rx = Some(rx);

        let mut reg = self.registry.lock();
        reg.listeners.insert(addr, tx);
        debug!(addr = %addr, "mock transport listening");
        Ok(())
    }

    async fn accept(&mut self) -> Result<MockConnection> {
        let rx = self
            .incoming_rx
            .as_mut()
            .ok_or_else(|| OmniMeshError::Transport("not listening".into()))?;

        rx.recv()
            .await
            .ok_or_else(|| OmniMeshError::Transport("listener closed".into()))
    }

    async fn connect(&mut self, addr: SocketAddr) -> Result<MockConnection> {
        let local = self
            .local_addr
            .unwrap_or_else(|| "127.0.0.1:0".parse().unwrap());

        // Create bidirectional channel pair
        let (a_tx, b_rx) = mpsc::channel(256);
        let (b_tx, a_rx) = mpsc::channel(256);

        let our_conn = MockConnection {
            _local_addr: local,
            remote_addr: addr,
            tx: a_tx,
            rx: a_rx,
            connected: true,
        };

        let their_conn = MockConnection {
            _local_addr: addr,
            remote_addr: local,
            tx: b_tx,
            rx: b_rx,
            connected: true,
        };

        // Deliver their side to the listener
        let listener_tx = {
            let reg = self.registry.lock();
            reg.listeners
                .get(&addr)
                .cloned()
                .ok_or_else(|| OmniMeshError::Connection(format!("no listener at {}", addr)))?
        };

        listener_tx
            .send(their_conn)
            .await
            .map_err(|_| OmniMeshError::Connection("listener dropped".into()))?;

        debug!(local = %local, remote = %addr, "mock connection established");
        Ok(our_conn)
    }

    async fn shutdown(&mut self) -> Result<()> {
        if let Some(addr) = self.local_addr.take() {
            let mut reg = self.registry.lock();
            reg.listeners.remove(&addr);
        }
        self.incoming_tx = None;
        self.incoming_rx = None;
        Ok(())
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
}

#[async_trait]
impl Connection for MockConnection {
    async fn send(&mut self, data: Bytes) -> Result<()> {
        if !self.connected {
            return Err(OmniMeshError::Connection("not connected".into()));
        }
        self.tx
            .send(data)
            .await
            .map_err(|_| OmniMeshError::Connection("send channel closed".into()))
    }

    async fn recv(&mut self) -> Result<Option<Bytes>> {
        if !self.connected {
            return Ok(None);
        }
        Ok(self.rx.recv().await)
    }

    async fn close(&mut self) -> Result<()> {
        self.connected = false;
        Ok(())
    }

    fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_connect_and_send() {
        let registry = MockRegistry::new();
        let mut server = MockTransport::new(registry.clone());
        let mut client = MockTransport::new(registry.clone());

        let server_addr: SocketAddr = "127.0.0.1:9999".parse().unwrap();
        server.listen(server_addr).await.unwrap();

        // Client connects
        let mut client_conn = client.connect(server_addr).await.unwrap();

        // Server accepts
        let mut server_conn = server.accept().await.unwrap();

        // Client → Server
        client_conn.send(Bytes::from("hello")).await.unwrap();
        let received = server_conn.recv().await.unwrap().unwrap();
        assert_eq!(received, Bytes::from("hello"));

        // Server → Client
        server_conn.send(Bytes::from("world")).await.unwrap();
        let received = client_conn.recv().await.unwrap().unwrap();
        assert_eq!(received, Bytes::from("world"));
    }

    #[tokio::test]
    async fn test_mock_multiple_clients() {
        let registry = MockRegistry::new();
        let mut server = MockTransport::new(registry.clone());
        let server_addr: SocketAddr = "127.0.0.1:8888".parse().unwrap();
        server.listen(server_addr).await.unwrap();

        for i in 0..3 {
            let mut client = MockTransport::new(registry.clone());
            let mut conn = client.connect(server_addr).await.unwrap();
            let mut srv_conn = server.accept().await.unwrap();

            let msg = format!("client-{}", i);
            conn.send(Bytes::from(msg.clone())).await.unwrap();
            let received = srv_conn.recv().await.unwrap().unwrap();
            assert_eq!(received, Bytes::from(msg));
        }
    }

    #[tokio::test]
    async fn test_mock_connect_no_listener_fails() {
        let registry = MockRegistry::new();
        let mut client = MockTransport::new(registry);
        let result = client.connect("127.0.0.1:7777".parse().unwrap()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_close_connection() {
        let registry = MockRegistry::new();
        let mut server = MockTransport::new(registry.clone());
        let mut client = MockTransport::new(registry.clone());

        let addr: SocketAddr = "127.0.0.1:6666".parse().unwrap();
        server.listen(addr).await.unwrap();

        let mut conn = client.connect(addr).await.unwrap();
        assert!(conn.is_connected());

        conn.close().await.unwrap();
        assert!(!conn.is_connected());
    }
}
