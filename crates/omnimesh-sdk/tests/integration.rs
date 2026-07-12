use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use omnimesh_core::config::Config;
use omnimesh_core::error::{OmniMeshError, Result};
use omnimesh_core::types::PeerId;
use omnimesh_identity::keypair::Keypair;
use omnimesh_sdk::OmniMeshBuilder;
use omnimesh_service::{ChatService, PubSubService, RpcHandler, RpcService};
use omnimesh_transport::mock::{MockRegistry, MockTransport};

// Mock RPC handler
struct CalculatorHandler;

#[async_trait]
impl RpcHandler for CalculatorHandler {
    async fn handle_request(
        &self,
        _from: PeerId,
        method: &str,
        request: Vec<u8>,
    ) -> Result<Vec<u8>> {
        if method == "add" {
            let nums: (i32, i32) = bincode::deserialize(&request)
                .map_err(|e| OmniMeshError::Serialization(format!("deserialize request: {}", e)))?;
            let sum = nums.0 + nums.1;
            let reply = bincode::serialize(&sum).unwrap();
            Ok(reply)
        } else {
            Err(OmniMeshError::Service("unknown method".into()))
        }
    }
}

#[tokio::test]
async fn test_sdk_services_integration() {
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Configure Mock addresses
    let alice_addr: SocketAddr = "127.0.0.1:2001".parse().unwrap();
    let bob_addr: SocketAddr = "127.0.0.1:2002".parse().unwrap();

    let mut config_alice = Config::default();
    config_alice.transport.listen_addr = alice_addr;

    let mut config_bob = Config::default();
    config_bob.transport.listen_addr = bob_addr;

    let kp_alice = Keypair::generate();
    let kp_bob = Keypair::generate();

    let registry = MockRegistry::new();

    // 2. Build Alice
    let alice = OmniMeshBuilder::new()
        .config(config_alice)
        .keypair(kp_alice)
        .transport(MockTransport::new(registry.clone()))
        .build()
        .await
        .unwrap();

    // 3. Build Bob
    let bob = OmniMeshBuilder::new()
        .config(config_bob)
        .keypair(kp_bob)
        .transport(MockTransport::new(registry.clone()))
        .build()
        .await
        .unwrap();

    // 4. Register Services for Alice
    let alice_chat = Arc::new(ChatService::new(alice.clone()));
    let alice_rpc = Arc::new(RpcService::new(alice.clone(), Arc::new(CalculatorHandler)));
    let alice_pubsub = Arc::new(PubSubService::new(alice.clone()));
    alice.register_service(alice_chat.clone());
    alice.register_service(alice_rpc.clone());
    alice.register_service(alice_pubsub.clone());

    // 5. Register Services for Bob
    let bob_chat = Arc::new(ChatService::new(bob.clone()));
    let bob_rpc = Arc::new(RpcService::new(bob.clone(), Arc::new(CalculatorHandler)));
    let bob_pubsub = Arc::new(PubSubService::new(bob.clone()));
    bob.register_service(bob_chat.clone());
    bob.register_service(bob_rpc.clone());
    bob.register_service(bob_pubsub.clone());

    // 6. Connect Alice to Bob
    let bob_peer_id = alice.connect_addr(bob_addr).await.unwrap();
    assert_eq!(bob_peer_id, bob.peer_id());

    // Let connection settle
    tokio::time::sleep(Duration::from_millis(100)).await;

    // 7. Test Chat Service (Alice -> Bob)
    let mut bob_chat_rx = bob_chat.subscribe();
    alice_chat.send_chat("Hello, Bob!").await.unwrap();

    let (from_peer, text) = tokio::time::timeout(Duration::from_secs(2), bob_chat_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(from_peer, alice.peer_id());
    assert_eq!(text, "Hello, Bob!");

    // 8. Test RPC Service (Alice -> Bob)
    let req_payload = bincode::serialize(&(12, 30)).unwrap();
    let reply_bytes = alice_rpc
        .call(bob.peer_id(), "add", req_payload)
        .await
        .unwrap();
    let sum: i32 = bincode::deserialize(&reply_bytes).unwrap();
    assert_eq!(sum, 42);

    // 9. Test PubSub Service (Alice -> Bob)
    let mut bob_sub_rx = bob_pubsub.subscribe("metrics");
    alice_pubsub
        .publish("metrics", b"test_payload".to_vec())
        .await
        .unwrap();

    let pub_payload = tokio::time::timeout(Duration::from_secs(2), bob_sub_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(pub_payload, b"test_payload".to_vec());

    // 10. Clean shutdown
    alice.shutdown().await.unwrap();
    bob.shutdown().await.unwrap();
}
