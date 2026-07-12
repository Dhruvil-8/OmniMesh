# OmniMesh Roadmap

## Timeline Overview

```
Phase 0-1: Foundation          ██████████████  [DONE]
Phase 2: Identity              ██████████████  [DONE]
Phase 3: Crypto                ██████████████  [DONE]
Phase 4: Transport             ██████████████  [DONE]
Phase 5: Discovery             ██████████████  [DONE]
Phase 6: Routing               ██████████████  [DONE]
Phase 7: Service Layer         ██████████████  [DONE]
Phase 8: SDK                   ██████████████  [DONE]
Phase 9: Examples              ██████████████  [DONE]
Phase 10: Benchmarks           ██████████████  [DONE]
Phase 11: Fuzzing              ██████████████  [DONE]
Phase 12: Security Review      ██████████████  [DONE]
Phase 13: Documentation        ██████████████  [DONE]
```

## Dependency Graph (Parallelizable Phases)

```
Phase 0-1 (Foundation)
    │
    ├──▶ Phase 2 (Identity)
    │         │
    │         ▼
    │    Phase 3 (Crypto) ──────────────┐
    │         │                         │
    │         ▼                         ▼
    │    Phase 4 (Transport)      Phase 11 (Fuzzing)
    │         │                         │
    │         ├──▶ Phase 5 (Discovery)  │
    │         │         │               │
    │         │         ▼               │
    │         │    Phase 6 (Routing)    │
    │         │         │               │
    │         │         ▼               ▼
    │         │    Phase 7 (Services)  Phase 12 (Security)
    │         │         │
    │         │         ▼
    │         └──▶ Phase 8 (SDK) ──▶ Phase 9 (Examples)
    │                                    │
    │                                    ▼
    └──────────────────────────────▶ Phase 10 (Benchmarks)
                                         │
                                         ▼
                                    Phase 13 (Docs)
```

**Parallelizable:**
- Fuzzing (Phase 11) can start as soon as crypto/transport exist
- Benchmarks (Phase 10) can start after transport is working
- Security review (Phase 12) is continuous

## Milestones

### M1: "Two Nodes Connect" (Phases 0-4)
- ✅ Identity generation and storage
- ✅ Noise-authenticated key exchange
- ✅ Encrypted data transfer over QUIC
- ✅ Mock transport for testing
- **Acceptance:** Two nodes exchange encrypted messages on localhost

### M2: "Peer Discovery" (Phase 5-6)
- ✅ DHT-based peer discovery
- ✅ Bootstrap node support
- ✅ LAN discovery via mDNS
- ✅ NAT traversal (STUN + relay)
- **Acceptance:** Two nodes behind NAT find and connect to each other

### M3: "Mesh Network" (Phase 6-7)
- ✅ Multi-hop routing with path selection
- ✅ Relay forwarding
- ✅ Multipath support
- **Acceptance:** Message routes through 3+ hops to reach destination

### M4: "Application Platform" (Phase 7-8)
- ✅ Service registration and discovery
- ✅ Chat, file transfer, RPC services
- ✅ High-level SDK API
- **Acceptance:** Chat demo works between 5+ nodes

### M5: "Production Ready" (Phase 9-13)
- ✅ All examples working
- ✅ Benchmarks passing performance targets
- ✅ Fuzz tests clean
- ✅ Security review complete
- ✅ Full documentation
- **Acceptance:** ✅ 100-node stress test passes all benchmarks
