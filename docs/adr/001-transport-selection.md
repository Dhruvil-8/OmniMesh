# 001 — Transport Layer Selection

## Status: Accepted

## Context

OmniMesh needs a transport layer that provides:
- Reliable, encrypted connections
- NAT traversal (hole punching, relay fallback)
- Multiplexed streams
- Cross-platform support (Linux, macOS, Windows, mobile)

Three main options were evaluated:

| Criteria | quinn | iroh | libp2p |
|---|---|---|---|
| **Layer** | QUIC only | P2P toolkit on QUIC | Full P2P framework |
| **NAT traversal** | None (manual) | Built-in (hole punch + relay) | DCUtR + Circuit Relay |
| **DHT/Discovery** | None | Basic (relay-based) | Kademlia, mDNS, etc. |
| **Cross-language** | Rust only | Rust only | Go, JS, Rust, etc. |
| **Complexity** | Low | Medium | High |
| **Modularity** | Excellent | Good | Fair (tightly coupled) |
| **Ecosystem** | Mature | Growing | Massive |

## Decision

Use **quinn** (QUIC) as the base transport with custom trait abstraction, keeping
the door open for libp2p integration at the discovery/routing layer when cross-language
interop is needed.

**Rationale:**
1. quinn gives maximum control over the transport without framework lock-in
2. OmniMesh's trait-based `Transport` abstraction means we can add libp2p as
   another transport implementation later without changing application code
3. NAT traversal will be implemented as a separate concern in the discovery layer,
   rather than being tightly coupled to the transport
4. libp2p's Kademlia and Gossipsub can still be used at the discovery/routing
   layer without using libp2p's transport

## Consequences

- We implement our own NAT traversal (STUN + relay) in Phase 6
- We maintain our own Transport trait instead of using libp2p's Swarm
- Adding libp2p interop later requires a libp2p Transport adapter
- Maximum flexibility and modularity for AI agents to maintain each layer independently
