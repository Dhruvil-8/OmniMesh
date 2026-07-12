//! Service layer containing traits and built-in service implementations.
//!
//! Provides Chat, RPC, and PubSub functionality on top of the secure peer channels.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use omnimesh_core::error::{OmniMeshError, Result};
use omnimesh_core::types::PeerId;

/// Trait implemented by the transport/SDK orchestrator to allow services to send messages.
#[async_trait]
pub trait MessageSender: Send + Sync + 'static {
    /// Send a message to a specific peer.
    async fn send_message(
        &self,
        to: PeerId,
        service_id: &'static str,
        payload: Vec<u8>,
    ) -> Result<()>;

    /// Broadcast a message to all active/connected peers.
    async fn broadcast_message(&self, service_id: &'static str, payload: Vec<u8>) -> Result<()>;
}

/// A lifecycle trait representing an application service running on the network.
#[async_trait]
pub trait Service: Send + Sync {
    /// Unique identifier for this service (e.g. "chat", "rpc", "pubsub").
    fn id(&self) -> &'static str;

    /// Process an incoming message received from a remote peer.
    async fn handle_message(&self, from: PeerId, payload: Vec<u8>) -> Result<()>;
}

// ==========================================
// 1. CHAT SERVICE
// ==========================================

/// Chat message packet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub text: String,
}

/// Service handling multi-user real-time chat.
pub struct ChatService {
    sender: Arc<dyn MessageSender>,
    message_tx: tokio::sync::broadcast::Sender<(PeerId, String)>,
}

impl ChatService {
    /// Create a new chat service instance.
    pub fn new(sender: Arc<dyn MessageSender>) -> Self {
        let (tx, _) = tokio::sync::broadcast::channel(100);
        Self {
            sender,
            message_tx: tx,
        }
    }

    /// Subscribe to incoming chat messages.
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<(PeerId, String)> {
        self.message_tx.subscribe()
    }

    /// Send a chat message to all connected peers.
    pub async fn send_chat(&self, text: &str) -> Result<()> {
        let payload = Self::format_message(text);
        self.sender.broadcast_message("chat", payload).await
    }

    /// Helper to package a chat message into serialize bytes.
    pub fn format_message(text: &str) -> Vec<u8> {
        bincode::serialize(&ChatMessage {
            text: text.to_string(),
        })
        .unwrap()
    }
}

#[async_trait]
impl Service for ChatService {
    fn id(&self) -> &'static str {
        "chat"
    }

    async fn handle_message(&self, from: PeerId, payload: Vec<u8>) -> Result<()> {
        let msg: ChatMessage = bincode::deserialize(&payload)
            .map_err(|e| OmniMeshError::Serialization(format!("chat deserialize: {}", e)))?;

        let _ = self.message_tx.send((from, msg.text));
        Ok(())
    }
}

// ==========================================
// 2. RPC SERVICE
// ==========================================

/// RPC request/response packet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RpcMessage {
    Request {
        request_id: u64,
        method: String,
        payload: Vec<u8>,
    },
    Response {
        request_id: u64,
        payload: std::result::Result<Vec<u8>, String>,
    },
}

/// A handler trait implemented by the application to process incoming RPC requests.
#[async_trait]
pub trait RpcHandler: Send + Sync {
    async fn handle_request(&self, from: PeerId, method: &str, request: Vec<u8>)
        -> Result<Vec<u8>>;
}

type RpcResponseSender = tokio::sync::oneshot::Sender<std::result::Result<Vec<u8>, String>>;
type RpcResponseReceiver = tokio::sync::oneshot::Receiver<std::result::Result<Vec<u8>, String>>;

/// Service managing remote procedure calls.
pub struct RpcService {
    sender: Arc<dyn MessageSender>,
    pending_requests: DashMap<u64, RpcResponseSender>,
    next_request_id: AtomicU64,
    handler: Arc<dyn RpcHandler>,
}

impl RpcService {
    /// Create a new RPC service with a custom request handler.
    pub fn new(sender: Arc<dyn MessageSender>, handler: Arc<dyn RpcHandler>) -> Self {
        Self {
            sender,
            pending_requests: DashMap::new(),
            next_request_id: AtomicU64::new(1),
            handler,
        }
    }

    /// Perform a remote procedure call to a remote peer.
    pub async fn call(&self, to: PeerId, method: &str, request: Vec<u8>) -> Result<Vec<u8>> {
        let (_request_id, bytes, rx) = self.create_request(method, request);
        self.sender.send_message(to, "rpc", bytes).await?;

        let response = rx
            .await
            .map_err(|_| OmniMeshError::Connection("RPC response channel closed".into()))?;

        response.map_err(|e| OmniMeshError::Service(format!("RPC error: {}", e)))
    }

    fn create_request(
        &self,
        method: &str,
        request: Vec<u8>,
    ) -> (u64, Vec<u8>, RpcResponseReceiver) {
        let request_id = self.next_request_id.fetch_add(1, Ordering::SeqCst);
        let msg = RpcMessage::Request {
            request_id,
            method: method.to_string(),
            payload: request,
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        self.pending_requests.insert(request_id, tx);
        let bytes = bincode::serialize(&msg).unwrap();
        (request_id, bytes, rx)
    }
}

#[async_trait]
impl Service for RpcService {
    fn id(&self) -> &'static str {
        "rpc"
    }

    async fn handle_message(&self, from: PeerId, payload: Vec<u8>) -> Result<()> {
        let msg: RpcMessage = bincode::deserialize(&payload)
            .map_err(|e| OmniMeshError::Serialization(format!("rpc deserialize: {}", e)))?;

        match msg {
            RpcMessage::Request {
                request_id,
                method,
                payload: req_payload,
            } => {
                let handler = self.handler.clone();
                let sender = self.sender.clone();
                tokio::spawn(async move {
                    let result = handler
                        .handle_request(from.clone(), &method, req_payload)
                        .await;
                    let response = match result {
                        Ok(res) => RpcMessage::Response {
                            request_id,
                            payload: Ok(res),
                        },
                        Err(err) => RpcMessage::Response {
                            request_id,
                            payload: Err(err.to_string()),
                        },
                    };
                    let bytes = bincode::serialize(&response).unwrap();
                    let _ = sender.send_message(from, "rpc", bytes).await;
                });
                Ok(())
            }
            RpcMessage::Response {
                request_id,
                payload: res_payload,
            } => {
                if let Some((_, tx)) = self.pending_requests.remove(&request_id) {
                    let _ = tx.send(res_payload);
                }
                Ok(())
            }
        }
    }
}

// ==========================================
// 3. PUBSUB SERVICE
// ==========================================

/// PubSub publication packet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PubSubMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}

/// Service managing topic-based publish/subscribe distribution.
pub struct PubSubService {
    sender: Arc<dyn MessageSender>,
    subscriptions: DashMap<String, Vec<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>>,
}

impl PubSubService {
    /// Create a new PubSub service instance.
    pub fn new(sender: Arc<dyn MessageSender>) -> Self {
        Self {
            sender,
            subscriptions: DashMap::new(),
        }
    }

    /// Subscribe to a topic locally.
    pub fn subscribe(&self, topic: &str) -> tokio::sync::mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.subscriptions
            .entry(topic.to_string())
            .or_default()
            .push(tx);
        rx
    }

    /// Publish a message to a topic.
    pub async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<()> {
        let wire = Self::format_publish(topic, payload);
        self.sender.broadcast_message("pubsub", wire).await
    }

    /// Format a publish message into wire bytes.
    pub fn format_publish(topic: &str, payload: Vec<u8>) -> Vec<u8> {
        bincode::serialize(&PubSubMessage {
            topic: topic.to_string(),
            payload,
        })
        .unwrap()
    }
}

#[async_trait]
impl Service for PubSubService {
    fn id(&self) -> &'static str {
        "pubsub"
    }

    async fn handle_message(&self, _from: PeerId, payload: Vec<u8>) -> Result<()> {
        let msg: PubSubMessage = bincode::deserialize(&payload)
            .map_err(|e| OmniMeshError::Serialization(format!("pubsub deserialize: {}", e)))?;

        // Forward to all local subscribers
        if let Some(subs) = self.subscriptions.get(&msg.topic) {
            for sub in subs.iter() {
                let _ = sub.send(msg.payload.clone());
            }
        }
        Ok(())
    }
}
