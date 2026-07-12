# OmniMesh

> A modular, cross-platform peer-to-peer mesh networking framework in Rust.

> [!IMPORTANT]
> **This entire codebase is an AI-generated and experimental prototype.** It is intended for research, testing, and evaluation purposes and should not be used in production environments without comprehensive independent security and code audits.

[![License: Apache 2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

## Vision

OmniMesh is a mesh networking library that enables any device to communicate with any other device — regardless of network topology, NAT configuration, or transport layer.

**Key Differentiators:**
- **Transport Agnostic** — QUIC, UDP, WebRTC, Bluetooth (pluggable via trait)
- **NAT Traversal Built-In** — Automatic hole punching with relay fallback
- **Smart Routing** — Multipath routing with latency/bandwidth/cost optimization
- **Security First** — Noise protocol, no custom cryptography, post-quantum ready
- **Cross-Platform SDK** — Rust, Python, Swift, Kotlin, C#, Go

## Architecture

```
┌─────────────────────────────────────────────┐
│                  SDK Layer                   │
│          (Rust / Python / Swift / etc.)      │
├─────────────────────────────────────────────┤
│               Service Layer                  │
│      (Chat / File / RPC / PubSub / AI)      │
├─────────────────────────────────────────────┤
│              Routing Layer                   │
│    (Route Graph / Metrics / Multipath)      │
├─────────────────────────────────────────────┤
│             Discovery Layer                  │
│   (Kademlia / LAN / Bluetooth / DNS)        │
├─────────────────────────────────────────────┤
│             Transport Layer                  │
│    (QUIC / UDP / WebRTC / Mock)             │
├─────────────────────────────────────────────┤
│              Crypto Layer                    │
│   (Noise / ChaCha20 / HKDF / MLS)          │
├─────────────────────────────────────────────┤
│             Identity Layer                   │
│   (Ed25519 / PeerId / KeyStore)             │
├─────────────────────────────────────────────┤
│               Core Layer                     │
│  (Config / Errors / Types / Observability)  │
└─────────────────────────────────────────────┘
```

## Workspace Structure

```
OmniMesh/
├── crates/
│   ├── omnimesh-core        # Shared types, errors, config, observability
│   ├── omnimesh-identity    # Ed25519 keys, PeerId, key storage
│   ├── omnimesh-crypto      # Noise protocol, AEAD, key derivation
│   ├── omnimesh-transport   # QUIC, mock transports, connection pooling
│   ├── omnimesh-routing     # Route graph, metrics, multipath
│   ├── omnimesh-discovery   # Peer discovery (Kademlia, LAN, DNS)
│   ├── omnimesh-service     # Service trait, chat, file transfer, RPC
│   ├── omnimesh-sdk         # High-level API for application developers
│   └── omnimesh-node        # CLI binary / daemon
├── docs/                    # Architecture & design documents
├── examples/                # Reference implementations
└── benchmarks/              # Performance benchmarks
```

## Quick Start

```bash
# Build the workspace
cargo build --workspace

# Run tests
cargo test --workspace

# Start a node
cargo run -p omnimesh-node -- --config config.toml
```

## License

- Apache License, Version 2.0 ([LICENSE](LICENSE))

