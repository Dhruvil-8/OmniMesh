# Transport Layer Design

## Overview

The transport layer provides pluggable, protocol-agnostic bidirectional connections.
Application code programs against the `Transport` and `Connection` traits, never
knowing which underlying protocol is in use.

## Transport Trait

```rust
#[async_trait]
pub trait Transport: Send + Sync + 'static {
    type Conn: Connection;
    async fn listen(&mut self, addr: SocketAddr) -> Result<()>;
    async fn accept(&mut self) -> Result<Self::Conn>;
    async fn connect(&mut self, addr: SocketAddr) -> Result<Self::Conn>;
    async fn shutdown(&mut self) -> Result<()>;
    fn local_addr(&self) -> Option<SocketAddr>;
}
```

## Implementations

### QUIC Transport (Primary)
- Built on `quinn` (pure Rust QUIC)
- TLS 1.3 encryption via `rustls`
- Multiplexed bidirectional streams
- Length-prefixed framing (4-byte big-endian + payload)
- Self-signed certs (authentication handled by Noise layer)

### Mock Transport (Testing)
- In-memory channels via `tokio::sync::mpsc`
- Shared registry for cross-connect
- No network I/O — deterministic, fast

### Future: WebRTC, Bluetooth, UDP Raw

## NAT Traversal Strategy

```
1. STUN query → discover public IP:port
2. Exchange addresses via signaling (bootstrap node or relay)
3. Simultaneous hole punch attempt (UDP)
4. If hole punch fails → relay via bootstrap node
5. Periodically retry direct connection
```

NAT traversal is implemented in the **discovery** layer, not the transport layer.
The transport only needs `connect(addr)` — the discovery layer resolves the
address via STUN/relay first.

## Protocol Versioning

Every handshake begins with version negotiation:

```
┌──────────┬───────────┬──────────────┐
│ Magic(4) │ Version(3)│ Payload ...  │
│ "OMSH"   │ 0.1.0     │              │
└──────────┴───────────┴──────────────┘
```

- Same major version → compatible (negotiate highest common minor)
- Different major version → reject with `ProtocolMismatch` error

## Why QUIC Over TCP

| Feature | TCP | QUIC |
|---|---|---|
| Head-of-line blocking | Yes (entire connection) | No (per-stream) |
| Connection setup | 1-3 RTT (TCP + TLS) | 1 RTT (0-RTT with resume) |
| Multiplexing | Requires framing | Native streams |
| Connection migration | No | Yes (via connection ID) |
| Encryption | Optional (TLS) | Mandatory |

## Performance Goals

- Connection setup: < 100ms on LAN, < 500ms on WAN
- Throughput: > 1 Gbps on localhost, > 500 Mbps on LAN
- Latency overhead: < 1ms over raw UDP on localhost
