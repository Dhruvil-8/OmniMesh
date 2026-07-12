# OmniMesh Architecture

## Layer Diagram

```
┌─────────────────────────────────────────────────┐
│               SDK Layer (Phase 9)                │
│     mesh.connect() / publish() / rpc()          │
├─────────────────────────────────────────────────┤
│           Service Layer (Phase 8)                │
│   Chat │ File │ RPC │ PubSub │ AI │ Stream      │
├─────────────────────────────────────────────────┤
│           Routing Layer (Phase 7)                │
│   Route Graph │ Metrics │ Multipath │ Relay     │
├─────────────────────────────────────────────────┤
│          Discovery Layer (Phase 6)               │
│   Kademlia │ mDNS │ Bootstrap │ LAN │ BT        │
├─────────────────────────────────────────────────┤
│          Transport Layer (Phase 4)               │
│   QUIC (quinn) │ Mock │ WebRTC (future)         │
├─────────────────────────────────────────────────┤
│           Crypto Layer (Phase 3)                 │
│   Noise_XX │ ChaCha20 │ HKDF │ Ratchet         │
├─────────────────────────────────────────────────┤
│          Identity Layer (Phase 2)                │
│   Ed25519 │ PeerId │ VirtualIP │ KeyStore       │
├─────────────────────────────────────────────────┤
│              Core Layer (Phase 1)                │
│   Config │ Error │ Types │ Telemetry │ Retry    │
└─────────────────────────────────────────────────┘
```

## Crate Dependency Graph

```
omnimesh-node ──▶ omnimesh-sdk ──▶ omnimesh-service
                                        │
                                        ▼
                                  omnimesh-routing
                                        │
                                        ▼
                                 omnimesh-discovery
                                        │
                                        ▼
                                 omnimesh-transport
                                        │
                                        ▼
                                  omnimesh-crypto
                                        │
                                        ▼
                                 omnimesh-identity
                                        │
                                        ▼
                                  omnimesh-core
```

**Rule:** No circular dependencies. Each layer only depends on layers below it.

## Design Principles

1. **One file, one responsibility** — Keep files under 300 lines for AI maintainability
2. **Trait-based abstraction** — Every layer defines a trait; implementations are pluggable
3. **No custom crypto** — Use audited libraries (RustCrypto, snow)
4. **Zeroize secrets** — All key material is zeroized on drop
5. **Structured logging** — `tracing` spans for every subsystem
6. **Error taxonomy** — Unified `OmniMeshError` with retryable/security classification

## Extension Points

- **Custom transports** — Implement `Transport` trait (e.g., Bluetooth, LoRa)
- **Custom services** — Implement `Service` trait
- **Custom discovery** — Add new discovery backends
- **Custom key storage** — Implement `KeyStore` trait (e.g., HSM, cloud KMS)
